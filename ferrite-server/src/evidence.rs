//! Durable evidence log for releases, update outcomes and incidents.
//!
//! Compliance records (what firmware was released, which devices received
//! it and when, what failed and what was done about it) must outlive
//! telemetry retention and demo resets, and must not be silently editable.
//! So the log:
//!
//! - lives in its own SQLite file (`--evidence-db`), untouched by retention
//!   purges and the demo database reset;
//! - is append-only: triggers reject UPDATE and DELETE;
//! - is hash-chained: each entry's SHA-256 covers the previous entry's hash
//!   and the entry's stored fields, so `verify` detects any edit, reorder or
//!   deletion done behind the triggers' back. Recording the head hash
//!   elsewhere (an export, a ticket) pins everything before it.

use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

/// `prev_hash` of the first entry.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One recorded event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceEntry {
    pub seq: i64,
    /// UTC, `YYYY-MM-DD HH:MM:SS`.
    pub recorded_at: String,
    /// Dotted event kind, e.g. `firmware.uploaded`, `device.update_installed`.
    pub kind: String,
    /// What the event is about, e.g. `firmware:3`, `campaign:1`, `device:dev-1`.
    pub subject: String,
    /// Who caused it: a user, `device:<id>`, or `system`.
    pub actor: String,
    /// Event details as JSON.
    pub details: serde_json::Value,
    pub prev_hash: String,
    pub hash: String,
}

/// Result of checking the whole chain.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerifyReport {
    pub entries: i64,
    pub valid: bool,
    /// First entry whose hash or link doesn't check out.
    pub first_invalid_seq: Option<i64>,
    pub head_seq: Option<i64>,
    pub head_hash: Option<String>,
}

/// Filter for [`EvidenceLog::list`].
#[derive(Debug, Default, Clone, Deserialize)]
pub struct EvidenceFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub kind: Option<String>,
    pub subject: Option<String>,
    pub limit: Option<i64>,
}

pub struct EvidenceLog {
    conn: Mutex<Connection>,
}

impl EvidenceLog {
    pub fn open(path: &Path) -> SqlResult<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> SqlResult<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS evidence_log (
                 seq         INTEGER PRIMARY KEY,
                 recorded_at TEXT NOT NULL,
                 kind        TEXT NOT NULL,
                 subject     TEXT NOT NULL,
                 actor       TEXT NOT NULL,
                 details     TEXT NOT NULL,
                 prev_hash   TEXT NOT NULL,
                 hash        TEXT NOT NULL UNIQUE
             );
             CREATE INDEX IF NOT EXISTS idx_evidence_time ON evidence_log(recorded_at);
             CREATE INDEX IF NOT EXISTS idx_evidence_subject ON evidence_log(subject);
             CREATE TRIGGER IF NOT EXISTS evidence_no_update BEFORE UPDATE ON evidence_log
                 BEGIN SELECT RAISE(ABORT, 'evidence log is append-only'); END;
             CREATE TRIGGER IF NOT EXISTS evidence_no_delete BEFORE DELETE ON evidence_log
                 BEGIN SELECT RAISE(ABORT, 'evidence log is append-only'); END;",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Append an event and return the stored entry.
    pub fn append(
        &self,
        kind: &str,
        subject: &str,
        actor: &str,
        details: serde_json::Value,
    ) -> SqlResult<EvidenceEntry> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.unchecked_transaction()?;
        let (prev_seq, prev_hash): (i64, String) = tx
            .query_row(
                "SELECT seq, hash FROM evidence_log ORDER BY seq DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .unwrap_or((0, GENESIS_HASH.to_string()));
        let seq = prev_seq + 1;
        let recorded_at: String = tx.query_row("SELECT datetime('now')", [], |row| row.get(0))?;
        let details_text = details.to_string();
        let hash = entry_hash(
            &prev_hash,
            seq,
            &recorded_at,
            kind,
            subject,
            actor,
            &details_text,
        );
        tx.execute(
            "INSERT INTO evidence_log (seq, recorded_at, kind, subject, actor, details, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![seq, recorded_at, kind, subject, actor, details_text, prev_hash, hash],
        )?;
        tx.commit()?;
        Ok(EvidenceEntry {
            seq,
            recorded_at,
            kind: kind.to_string(),
            subject: subject.to_string(),
            actor: actor.to_string(),
            details,
            prev_hash,
            hash,
        })
    }

    /// Entries matching `filter`, oldest first.
    ///
    /// `from`/`to` accept anything SQLite's `datetime()` parses (`2026-10-18`,
    /// `2026-10-18 09:00:00`, RFC 3339 with an offset); other values are an
    /// [`rusqlite::Error::InvalidParameterName`] error.
    pub fn list(&self, filter: &EvidenceFilter) -> SqlResult<Vec<EvidenceEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        for bound in [&filter.from, &filter.to].into_iter().flatten() {
            let parsed: Option<String> =
                conn.query_row("SELECT datetime(?1)", [bound], |r| r.get(0))?;
            if parsed.is_none() {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "invalid time: {bound}"
                )));
            }
        }
        let mut stmt = conn.prepare(
            "SELECT seq, recorded_at, kind, subject, actor, details, prev_hash, hash
             FROM evidence_log
             WHERE (?1 IS NULL OR recorded_at >= datetime(?1))
               AND (?2 IS NULL OR recorded_at <= datetime(?2))
               AND (?3 IS NULL OR kind = ?3 OR kind LIKE ?3 || '.%')
               AND (?4 IS NULL OR subject = ?4)
             ORDER BY seq
             LIMIT ?5",
        )?;
        let limit = filter.limit.unwrap_or(10_000).clamp(1, 100_000);
        let rows = stmt.query_map(
            params![filter.from, filter.to, filter.kind, filter.subject, limit],
            row_to_entry,
        )?;
        rows.collect()
    }

    /// Recompute every hash and link from the stored fields.
    pub fn verify(&self) -> SqlResult<VerifyReport> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT seq, recorded_at, kind, subject, actor, details, prev_hash, hash
             FROM evidence_log ORDER BY seq",
        )?;
        let mut rows = stmt.query([])?;
        let mut expected_prev = GENESIS_HASH.to_string();
        let mut expected_seq = 1i64;
        let mut entries = 0i64;
        let mut first_invalid_seq = None;
        let mut head = None;
        while let Some(row) = rows.next()? {
            let seq: i64 = row.get(0)?;
            let recorded_at: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let subject: String = row.get(3)?;
            let actor: String = row.get(4)?;
            let details: String = row.get(5)?;
            let prev_hash: String = row.get(6)?;
            let hash: String = row.get(7)?;
            entries += 1;
            let recomputed = entry_hash(
                &prev_hash,
                seq,
                &recorded_at,
                &kind,
                &subject,
                &actor,
                &details,
            );
            if first_invalid_seq.is_none()
                && (seq != expected_seq || prev_hash != expected_prev || hash != recomputed)
            {
                first_invalid_seq = Some(seq);
            }
            expected_prev = hash.clone();
            expected_seq = seq + 1;
            head = Some((seq, hash));
        }
        Ok(VerifyReport {
            entries,
            valid: first_invalid_seq.is_none(),
            first_invalid_seq,
            head_seq: head.as_ref().map(|h| h.0),
            head_hash: head.map(|h| h.1),
        })
    }
}

/// Who caused a request-driven event: the authenticated user, else `api`.
pub fn user_actor(claims: Option<&crate::auth::UserClaims>) -> String {
    match claims {
        Some(c) => c
            .email
            .clone()
            .or_else(|| c.name.clone())
            .unwrap_or_else(|| c.sub.clone()),
        None => "api".to_string(),
    }
}

/// Append an event; a failed write is logged loudly but doesn't fail the
/// request that caused it.
pub fn record(
    log: &EvidenceLog,
    kind: &str,
    subject: &str,
    actor: &str,
    details: serde_json::Value,
) {
    if let Err(e) = log.append(kind, subject, actor, details) {
        tracing::error!(kind, subject, error = %e, "failed to write evidence log entry");
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceEntry> {
    let details: String = row.get(5)?;
    Ok(EvidenceEntry {
        seq: row.get(0)?,
        recorded_at: row.get(1)?,
        kind: row.get(2)?,
        subject: row.get(3)?,
        actor: row.get(4)?,
        details: serde_json::from_str(&details).unwrap_or(serde_json::Value::String(details)),
        prev_hash: row.get(6)?,
        hash: row.get(7)?,
    })
}

/// SHA-256 over the previous hash and the entry's stored fields, as hex.
/// Fields are length-prefixed so no two different entries hash alike.
fn entry_hash(
    prev_hash: &str,
    seq: i64,
    recorded_at: &str,
    kind: &str,
    subject: &str,
    actor: &str,
    details: &str,
) -> String {
    let mut h = Sha256::new();
    h.update(b"ferrite-evidence-v1");
    for field in [
        prev_hash,
        &seq.to_string(),
        recorded_at,
        kind,
        subject,
        actor,
        details,
    ] {
        h.update((field.len() as u64).to_le_bytes());
        h.update(field.as_bytes());
    }
    hex::encode(h.finalize())
}

// ---------------------------------------------------------------------------
// HTTP API
// ---------------------------------------------------------------------------

/// GET /evidence?from=&to=&kind=&subject=&limit= — entries oldest first,
/// with the current chain head so an export can be pinned.
pub async fn list_evidence(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::extract::Query(filter): axum::extract::Query<EvidenceFilter>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let result = state
        .evidence
        .list(&filter)
        .and_then(|entries| Ok((entries, state.evidence.verify()?)));
    match result {
        Ok((entries, report)) => axum::Json(serde_json::json!({
            "entries": entries,
            "head_seq": report.head_seq,
            "head_hash": report.head_hash,
            "chain_valid": report.valid,
        }))
        .into_response(),
        Err(rusqlite::Error::InvalidParameterName(msg)) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /evidence/verify — recompute the whole hash chain.
pub async fn verify_evidence(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match state.evidence.verify() {
        Ok(report) => axum::Json(report).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log_with(n: usize) -> EvidenceLog {
        let log = EvidenceLog::open_in_memory().unwrap();
        for i in 0..n {
            log.append(
                "device.update_installed",
                &format!("device:d{i}"),
                "system",
                json!({"build_id": i}),
            )
            .unwrap();
        }
        log
    }

    #[test]
    fn appends_chain_from_genesis() {
        let log = EvidenceLog::open_in_memory().unwrap();
        let a = log
            .append(
                "firmware.uploaded",
                "firmware:1",
                "alice",
                json!({"v": "1.0"}),
            )
            .unwrap();
        let b = log
            .append("campaign.created", "campaign:1", "alice", json!({}))
            .unwrap();
        assert_eq!((a.seq, b.seq), (1, 2));
        assert_eq!(a.prev_hash, GENESIS_HASH);
        assert_eq!(b.prev_hash, a.hash);
        let report = log.verify().unwrap();
        assert!(report.valid);
        assert_eq!(report.entries, 2);
        assert_eq!(report.head_hash.as_deref(), Some(b.hash.as_str()));
    }

    #[test]
    fn update_and_delete_are_rejected() {
        let log = log_with(2);
        let conn = log.conn.lock().unwrap();
        let upd = conn.execute(
            "UPDATE evidence_log SET actor = 'mallory' WHERE seq = 1",
            [],
        );
        assert!(upd.unwrap_err().to_string().contains("append-only"));
        let del = conn.execute("DELETE FROM evidence_log WHERE seq = 2", []);
        assert!(del.unwrap_err().to_string().contains("append-only"));
    }

    /// Simulate tampering by someone who can bypass the triggers.
    fn tamper(log: &EvidenceLog, sql: &str) {
        let conn = log.conn.lock().unwrap();
        conn.execute_batch(&format!(
            "DROP TRIGGER evidence_no_update; DROP TRIGGER evidence_no_delete; {sql}"
        ))
        .unwrap();
    }

    #[test]
    fn verify_detects_edits() {
        let log = log_with(4);
        tamper(
            &log,
            "UPDATE evidence_log SET details = '{\"build_id\":99}' WHERE seq = 3;",
        );
        let r = log.verify().unwrap();
        assert!(!r.valid);
        assert_eq!(r.first_invalid_seq, Some(3));
    }

    #[test]
    fn verify_detects_deletion() {
        let log = log_with(4);
        tamper(&log, "DELETE FROM evidence_log WHERE seq = 2;");
        let r = log.verify().unwrap();
        assert_eq!(r.first_invalid_seq, Some(3));
    }

    #[test]
    fn verify_detects_rewritten_hash_without_relinking() {
        let log = log_with(3);
        // Recompute entry 2's own hash after editing it: entry 3 no longer links.
        let e2 = log.list(&EvidenceFilter::default()).unwrap()[1].clone();
        let new_hash = entry_hash(
            &e2.prev_hash,
            2,
            &e2.recorded_at,
            &e2.kind,
            "device:evil",
            &e2.actor,
            &e2.details.to_string(),
        );
        tamper(
            &log,
            &format!("UPDATE evidence_log SET subject = 'device:evil', hash = '{new_hash}' WHERE seq = 2;"),
        );
        assert_eq!(log.verify().unwrap().first_invalid_seq, Some(3));
    }

    #[test]
    fn list_filters() {
        let log = EvidenceLog::open_in_memory().unwrap();
        log.append("campaign.created", "campaign:1", "a", json!({}))
            .unwrap();
        log.append("campaign.activated", "campaign:1", "a", json!({}))
            .unwrap();
        log.append("device.update_installed", "device:x", "device:x", json!({}))
            .unwrap();
        let all = log.list(&EvidenceFilter::default()).unwrap();
        assert_eq!(all.len(), 3);
        let campaign = log
            .list(&EvidenceFilter {
                kind: Some("campaign".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(campaign.len(), 2);
        let exact = log
            .list(&EvidenceFilter {
                kind: Some("campaign.activated".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(exact.len(), 1);
        let device = log
            .list(&EvidenceFilter {
                subject: Some("device:x".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(device[0].kind, "device.update_installed");
        let limited = log
            .list(&EvidenceFilter {
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited[0].seq, 1);
    }

    /// Offline verification of a JSON export relies on `details` re-serializing
    /// to exactly the stored text (compact, keys sorted).
    #[test]
    fn exported_entries_rehash_without_the_database() {
        let log = EvidenceLog::open_in_memory().unwrap();
        log.append(
            "campaign.created",
            "campaign:1",
            "ops@example.com",
            json!({"zeta": 1, "alpha": {"b": [1, 2], "a": null}, "name": "Q4 \"fix\""}),
        )
        .unwrap();
        let exported: Vec<EvidenceEntry> = serde_json::from_str(
            &serde_json::to_string(&log.list(&EvidenceFilter::default()).unwrap()).unwrap(),
        )
        .unwrap();
        for e in exported {
            let details = serde_json::to_string(&e.details).unwrap();
            assert!(details.starts_with(r#"{"alpha":{"a":null"#), "{details}");
            let h = entry_hash(
                &e.prev_hash,
                e.seq,
                &e.recorded_at,
                &e.kind,
                &e.subject,
                &e.actor,
                &details,
            );
            assert_eq!(h, e.hash);
        }
    }

    #[test]
    fn time_bounds_accept_rfc3339_and_reject_garbage() {
        let log = log_with(2);
        let now = first_recorded_at(&log);
        // RFC 3339 with 'T' and an offset must compare as a time, not a string.
        let rfc = format!("{}T{}+00:00", &now[..10], &now[11..]);
        let hits = log
            .list(&EvidenceFilter {
                from: Some(rfc),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 2);
        let day = log
            .list(&EvidenceFilter {
                from: Some(now[..10].to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(day.len(), 2);
        let err = log
            .list(&EvidenceFilter {
                from: Some("yesterday-ish".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, rusqlite::Error::InvalidParameterName(_)));
    }

    fn first_recorded_at(log: &EvidenceLog) -> String {
        log.list(&EvidenceFilter::default()).unwrap()[0]
            .recorded_at
            .clone()
    }

    #[test]
    fn survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.db");
        {
            let log = EvidenceLog::open(&path).unwrap();
            log.append("firmware.uploaded", "firmware:1", "a", json!({}))
                .unwrap();
        }
        let log = EvidenceLog::open(&path).unwrap();
        let e = log
            .append("firmware.deleted", "firmware:1", "a", json!({}))
            .unwrap();
        assert_eq!(e.seq, 2);
        assert!(log.verify().unwrap().valid);
    }
}
