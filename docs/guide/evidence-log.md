# Evidence log

The server keeps an **append-only, hash-chained record** of every release and update decision: which firmware was uploaded (and whether its signature checked out), who targeted which devices, which builds the devices actually report, and which updates installed, failed or rolled back. It answers the question an auditor or a market-surveillance authority asks under the EU Cyber Resilience Act (Art. 13/14): *show that you shipped this security update, and that it reached the field.*

The log is separate from the operational database. Retention purges and the demo reset never touch it.

## What gets recorded

| Kind | Subject | Actor | Recorded when |
|---|---|---|---|
| `firmware.uploaded` | `firmware:<id>` | user | An image is uploaded. Details: version, build ID, SHA-256, size, security version, signer key ID, whether the signature was verified |
| `firmware.deleted` | `firmware:<id>` | user | An image is deleted |
| `ota.target_set` | `device:<id>` | user | A device is targeted directly |
| `ota.target_removed` | `device:<id>` | user | A direct target is removed |
| `campaign.created` | `campaign:<id>` | user | A rollout campaign is created, with its strategy, selection and devices |
| `campaign.activated` | `campaign:<id>` | user | A campaign starts; includes the image hash and how many targets were set |
| `campaign.paused` | `campaign:<id>` | user | A campaign is paused by hand |
| `campaign.auto_paused` | `campaign:<id>` | `system` | A campaign pauses itself because a device rolled back |
| `campaign.rolled_back` | `campaign:<id>` | user | A campaign is rolled back and its targets removed |
| `campaign.completed` | `campaign:<id>` | `system` | Every device in a campaign has installed |
| `device.first_seen` | `device:<id>` | `device:<id>` | A device reports its build for the first time |
| `device.build_changed` | `device:<id>` | `device:<id>` | A device reports a different build than before |
| `device.update_installed` | `device:<id>` | `device:<id>` | A device confirms a campaign's target build |
| `device.update_rolled_back` | `device:<id>` | `device:<id>` | A device's bootloader rolled an update back |
| `evidence.anchored` | `evidence:<head seq>` | `system` | The head was sent to a timestamp authority and/or webhook. Details: head hash, sink responses, the TSA token |

The actor is the signed-in user's email (or name or subject in Keycloak mode), the basic-auth user name, `device:<id>` for device reports, or `system` for automatic decisions.

## How tampering shows up

Each entry stores the hash of the previous entry and its own hash:

```
hash = SHA-256("ferrite-evidence-v1" ‖ prev_hash ‖ seq ‖ recorded_at ‖ kind ‖ subject ‖ actor ‖ details)
```

Each field is preceded by its UTF-8 byte length as a little-endian `u64`. `seq` is written as decimal text, `details` as compact JSON with sorted keys (the form a JSON export re-serializes to), and the hashes as lowercase hex. The first entry links to 64 zeros. SQLite triggers reject `UPDATE` and `DELETE` on the table, so the server itself can only append. Someone with direct write access to the file can still drop the triggers and edit rows, but then:

- an edited entry no longer matches its hash;
- a deleted entry breaks the `seq` sequence and the next entry's link;
- an entry whose hash was recomputed breaks the link from the entry after it.

The server checks the whole chain at startup and logs `Evidence log BROKEN` with the first bad sequence number if anything is off. To detect someone rewriting the **entire** tail, compare the head hash with an anchored copy kept somewhere else (see [Anchoring the head](#anchoring-the-head)).

## Reading the log

```bash
ferrite evidence list                                # everything, oldest first
ferrite evidence list --since 30d --kind campaign    # kind prefix: campaign.*
ferrite evidence list --subject device:h563-01
ferrite --format json evidence list --from 2026-10-01 --to 2026-10-31 > october.json
ferrite evidence verify                              # exits non-zero if the chain is broken
```

Over HTTP (admins and the read-only `auditor` role, see [roles](/guide/security#multi-user-rbac)):

| Method | Path | Description |
|---|---|---|
| GET | `/evidence` | Entries, oldest first, plus `head_seq`, `head_hash` and `chain_valid`. Query: `from`, `to` (`2026-10-18`, `2026-10-18 09:00:00` or RFC 3339), `kind` (exact or prefix), `subject`, `limit` (default 10 000, max 100 000) |
| GET | `/evidence/verify` | Recomputes the chain: `entries`, `valid`, `first_invalid_seq`, `head_seq`, `head_hash` |

A JSON export holds `prev_hash` and `hash` for every entry, so anyone can check it offline without access to the server.

## Storage and backups

The log lives in its own SQLite file, by default `evidence.db` next to the main database:

```bash
ferrite-server --db /data/ferrite.db                              # → /data/evidence.db
ferrite-server --db /data/ferrite.db --evidence-db /audit/evidence.db
```

Keep it on durable storage and include it in your backups. `GET /admin/backup` covers only the main database. Copy the evidence log with SQLite's online backup while the server runs:

```bash
sqlite3 /data/evidence.db ".backup '/backups/evidence-$(date +%F).db'"
```

Nothing purges the log. Deleting the file starts a new chain from genesis, and every earlier export then stops matching it.

## Anchoring the head

The hash chain proves the log is internally consistent. An **anchor** proves nobody replaced it wholesale: the server sends the current head hash to places it can't rewrite, and records each successful anchor in the log as `evidence.anchored`.

```bash
EVIDENCE_ANCHOR_TSA_URL=http://timestamp.digicert.com       # RFC 3161 timestamp authority
EVIDENCE_ANCHOR_WEBHOOK_URL=https://platform.example.com/... # and/or a webhook
EVIDENCE_ANCHOR_WEBHOOK_TOKEN=...                            # optional bearer token
EVIDENCE_ANCHOR_INTERVAL_SECS=86400                          # daily; 0 = on demand only
```

| Sink | What it gets | What it proves |
|---|---|---|
| Timestamp authority | The head hash as a SHA-256 message imprint (RFC 3161) | A signed token from a third party: this head existed at this time. Verifiable offline with `openssl`. |
| Webhook | `POST` JSON `{"kind":"ferrite.evidence.head","head_seq":…,"head_hash":"…"}` | The receiver (e.g. the BreakPilot platform) keeps its own copy of every head. |

Anchoring is off until a sink is set. Scheduled runs skip when nothing was added since the last anchor. To anchor at a specific moment, for example right after a release, use:

```bash
ferrite evidence anchor          # POST /evidence/anchor (admin)
```

### Verifying a timestamp

The `evidence.anchored` entry holds the authority's full reply. To check it:

```bash
ferrite --format json evidence list --kind evidence.anchored > anchors.json
jq -r '.[-1].details.tsa.response_der_b64' anchors.json | base64 -d > anchor.tsr
jq -r '.[-1].details.head_hash' anchors.json > head.txt

openssl ts -reply -in anchor.tsr -text | grep -A1 'Time stamp'
openssl ts -verify -sha256 -digest "$(cat head.txt)" -in anchor.tsr \
  -CAfile /etc/ssl/certs/ca-certificates.crt   # your system CA bundle
```

`Verification: OK` means the authority signed exactly this head hash at the printed time. A later log is genuine if its entry at that `head_seq` still has that hash; `ferrite evidence list` shows it.

Without a sink you can still anchor by hand: copy `head_seq` and `head_hash` into a release ticket or a signed git tag.

```bash
ferrite --format json evidence verify | jq -r '"\(.head_seq) \(.head_hash)"'
```
