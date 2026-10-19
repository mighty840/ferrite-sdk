# Linux agent

`ferrite-agent` reports an embedded Linux device to ferrite-server: which image it runs, how the system is doing, why it last rebooted, and whether the services you care about (for example the CODESYS Control runtime) are up. It is the Linux counterpart of the MCU SDK and speaks the same chunk format over `POST /ingest/chunks`. Linux devices therefore show up in the device list, metrics, OTA campaigns and update-reach reports with no server changes.

It runs as an unprivileged systemd service (a static `ferrite-agent` system user) and uses no database. On x86_64 the stripped binary is 3.7 MB with the default release profile, or 2.3 MB with `lto = true`, `opt-level = "s"`, `codegen-units = 1` and `panic = "abort"`.

## Install

```bash
cargo build -p ferrite-agent --release            # or cross-compile, see below
sudo install -m 0755 target/release/ferrite-agent /usr/bin/
sudo install -D -m 0644 ferrite-agent/dist/agent.conf.example /etc/ferrite/agent.conf
sudo install -D -m 0644 ferrite-agent/dist/ferrite-agent.sysusers /usr/lib/sysusers.d/ferrite-agent.conf
sudo systemd-sysusers                             # creates the ferrite-agent system user
sudo install -m 0644 ferrite-agent/dist/ferrite-agent.service /etc/systemd/system/
sudoedit /etc/ferrite/agent.conf                  # set server_url (and api_key)
sudo systemctl enable --now ferrite-agent
```

The unit deliberately avoids `DynamicUser=`: on images without `nss-systemd` in `/etc/nsswitch.conf` (Raspbian 10, many minimal builds), D-Bus can't resolve dynamic UIDs and refuses `systemctl show`, so unit metrics would be missing. If `systemd-sysusers` isn't available, create the user with `useradd --system --home-dir /var/lib/ferrite-agent --shell /usr/sbin/nologin ferrite-agent`.

A Yocto layer (`meta-ferrite`, [#85](https://github.com/mighty840/ferrite-sdk/issues/85)) will package the same files.

For 32-bit ARM, `cargo build --target armv7-unknown-linux-gnueabihf` needs an ARM linker. Add `--no-default-features` to leave out TLS: the agent then speaks plain HTTP only, for example to a ferrite-gateway on the local network, and needs no cross C compiler.

## Configuration

`/etc/ferrite/agent.conf` is TOML. Only `server_url` is required:

```toml
server_url = "https://ferrite.example.com"
api_key = "..."            # when the server sets INGEST_API_KEY
interval_secs = 60
disks = ["/", "/data"]

# [[units]] tables go last: TOML assigns any key after them to the table.
[[units]]
name = "codesyscontrol.service"
key = "codesys"
```

| Key | Default | Meaning |
|---|---|---|
| `server_url` | — | ferrite-server, or a gateway, base URL |
| `api_key` | none | Sent as `X-API-Key` |
| `interval_secs` | `60` | Seconds between reports |
| `disks` | `["/"]` | Filesystems to report |
| `units` | none | systemd units to watch (`name`, optional `key`) |
| `device_id`, `firmware_version`, `build_id` | derived | Identity overrides, see below |
| `state_dir` | `/var/lib/ferrite-agent` | Boot tracking and offline spool |
| `max_spooled_reports` | `1440` | Reports kept while offline (one day at 60 s) |
| `timeout_secs` | `15` | HTTP timeout |

`ferrite-agent identity` prints what the server will see. `ferrite-agent once` sends a single report and exits non-zero if it couldn't be delivered. `RUST_LOG=debug` raises the log level.

## Identity

The server tracks devices by ID, firmware version and a numeric `build_id`. Campaigns target a `build_id`, and update-reach reports group by it. The agent derives all three; set them in the config to override.

| Field | Source |
|---|---|
| `firmware_version` | `IMAGE_VERSION` from `/etc/os-release`, else `VERSION_ID`, else `VERSION` |
| `build_id` | `BUILD_ID`, else `IMAGE_VERSION`, else `VERSION_ID`, mapped as below |
| `device_id` | `linux-` + the first 16 hex digits of SHA-256(`ferrite-agent:` + `/etc/machine-id`) |

**`build_id` mapping:** a value of decimal digits that fits a `u64` is used as is. Yocto's default `BUILD_ID` is the build's `DATETIME` (e.g. `20261018093000`), which is numeric and increases with every build. Any other value maps to the first 8 bytes of its SHA-256, read as a little-endian `u64`:

```bash
python3 -c 'import hashlib,sys; print(int.from_bytes(hashlib.sha256(sys.argv[1].encode()).digest()[:8], "little"))' 2.3.1
```

To roll out a new image with a campaign, use the `build_id` that `ferrite-agent identity` prints on a device running that image.

The device ID hashes the machine ID because systemd asks applications not to expose it directly. Rename devices on the server, or set `device_id` (at most 127 bytes).

## What gets reported

Every report is one batch of chunks: `DeviceInfo` (marked as a Linux device, so the dashboard shows a Linux badge), a `RebootReason` on the first report after a reboot, a `Heartbeat` and the metrics.

| Metric | Type | Meaning |
|---|---|---|
| `uptime_seconds` | gauge | Seconds since boot |
| `cpu_pct` | gauge | CPU busy % since the last report |
| `load1` | gauge | 1-minute load average |
| `mem_used_pct`, `mem_avail_mib` | gauge | Memory use (from `MemAvailable`) |
| `cpu_temp` | gauge | Hottest thermal zone, °C |
| `disk_used_pct`, `disk_<name>_used_pct` | gauge | Filesystem use, as `df` counts it |
| `<key>_active` | gauge | 1 while the unit is active |
| `<key>_restarts` | counter | systemd's `NRestarts` for the unit |
| `units_failed` | gauge | Units in the failed state |
| `coredumps` | counter | systemd-coredump files written this boot |

The heartbeat's free-memory field carries available memory in bytes, capped at 4 GiB.

### Reboot reasons

The agent stores the kernel's `boot_id`. When it changes, it reports why the previous boot ended, using the SDK's reboot codes:

| Evidence | Reported as |
|---|---|
| Kernel panic/oops record in pstore since the last run | `HardFault` |
| Watchdog `bootstatus` has `WDIOF_CARDRESET` | `WatchdogTimeout` |
| `build_id` changed | `FirmwareUpdate` |
| The agent saw an orderly system shutdown | `SoftwareReset` |
| None of these (power cut, hang, reset button) | `Unknown` |

The previous boot's uptime is recorded every 10 minutes and at shutdown, so `uptime_before_reboot` can be up to 10 minutes short after a power cut. Reading pstore usually needs root. Under the shipped unit, panics show up as `Unknown` unless the agent can read `/sys/fs/pstore` or `/var/lib/systemd/pstore`.

## Offline behaviour

When the server can't be reached, or answers 401, 403, 404 or 429 (a fixable key, URL or rate-limit problem), reports are written to `<state_dir>/spool/`. They're sent oldest first once the server answers. Beyond `max_spooled_reports`, the oldest are dropped. Any other 4xx means the server will never accept that report, so it's dropped and logged.

Spooled metrics carry the device's uptime as their timestamp. The server stamps them with the time they arrive.

## Not yet covered

- Crash details (process, signal, backtrace) from systemd-coredump. Only the count is reported until the server gains a Linux crash record.
- Image updates. Use RAUC or SWUpdate; [#87](https://github.com/mighty840/ferrite-sdk/issues/87) tracks driving them from ferrite campaigns.
