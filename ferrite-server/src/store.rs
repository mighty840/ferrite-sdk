use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A known device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: i64,
    pub device_id: String,
    pub firmware_version: String,
    pub build_id: u64,
    pub first_seen: String,
    pub last_seen: String,
}

/// A stored fault event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub fault_type: u8,
    pub pc: u32,
    pub lr: u32,
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
    pub sp: u32,
    pub stack_snapshot: String,
    pub symbol: Option<String>,
    pub created_at: String,
}

/// A stored metric row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRow {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub key: String,
    pub metric_type: u8,
    pub value_json: String,
    pub timestamp_ticks: u64,
    pub created_at: String,
}

/// A stored reboot event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebootEvent {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub reason: u8,
    pub extra: u8,
    pub boot_sequence: u32,
    pub uptime_before_reboot: u32,
    pub created_at: String,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the SQLite database at the given path and ensure tables exist.
    pub fn open(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.create_tables()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.create_tables()?;
        Ok(store)
    }

    fn create_tables(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS devices (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id       TEXT NOT NULL UNIQUE,
                firmware_version TEXT NOT NULL DEFAULT '',
                build_id        INTEGER NOT NULL DEFAULT 0,
                first_seen      TEXT NOT NULL DEFAULT (datetime('now')),
                last_seen       TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS fault_events (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                device_rowid    INTEGER NOT NULL REFERENCES devices(id),
                fault_type      INTEGER NOT NULL,
                pc              INTEGER NOT NULL,
                lr              INTEGER NOT NULL,
                cfsr            INTEGER NOT NULL,
                hfsr            INTEGER NOT NULL,
                mmfar           INTEGER NOT NULL,
                bfar            INTEGER NOT NULL,
                sp              INTEGER NOT NULL,
                stack_snapshot  TEXT NOT NULL DEFAULT '[]',
                symbol          TEXT,
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS metrics (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                device_rowid    INTEGER NOT NULL REFERENCES devices(id),
                key             TEXT NOT NULL,
                metric_type     INTEGER NOT NULL,
                value_json      TEXT NOT NULL,
                timestamp_ticks INTEGER NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS reboot_events (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                device_rowid          INTEGER NOT NULL REFERENCES devices(id),
                reason                INTEGER NOT NULL,
                extra                 INTEGER NOT NULL DEFAULT 0,
                boot_sequence         INTEGER NOT NULL DEFAULT 0,
                uptime_before_reboot  INTEGER NOT NULL DEFAULT 0,
                created_at            TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_fault_device ON fault_events(device_rowid);
            CREATE INDEX IF NOT EXISTS idx_metrics_device ON metrics(device_rowid);
            CREATE INDEX IF NOT EXISTS idx_reboot_device ON reboot_events(device_rowid);
            ",
        )?;
        Ok(())
    }

    // ---- Device CRUD ----

    /// Upsert a device by device_id. Returns the row id.
    pub fn upsert_device(
        &self,
        device_id: &str,
        firmware_version: &str,
        build_id: u64,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO devices (device_id, firmware_version, build_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET
                firmware_version = excluded.firmware_version,
                build_id = excluded.build_id,
                last_seen = datetime('now')",
            params![device_id, firmware_version, build_id as i64],
        )?;
        let rowid = self.conn.query_row(
            "SELECT id FROM devices WHERE device_id = ?1",
            params![device_id],
            |row| row.get(0),
        )?;
        Ok(rowid)
    }

    /// Touch a device's last_seen timestamp. Returns the row id, creating
    /// a placeholder device record if one does not exist yet.
    pub fn touch_device(&self, device_id: &str) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO devices (device_id)
             VALUES (?1)
             ON CONFLICT(device_id) DO UPDATE SET last_seen = datetime('now')",
            params![device_id],
        )?;
        let rowid = self.conn.query_row(
            "SELECT id FROM devices WHERE device_id = ?1",
            params![device_id],
            |row| row.get(0),
        )?;
        Ok(rowid)
    }

    pub fn list_devices(&self) -> SqlResult<Vec<Device>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, device_id, firmware_version, build_id, first_seen, last_seen
             FROM devices ORDER BY last_seen DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Device {
                id: row.get(0)?,
                device_id: row.get(1)?,
                firmware_version: row.get(2)?,
                build_id: row.get::<_, i64>(3)? as u64,
                first_seen: row.get(4)?,
                last_seen: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_device_by_id(&self, device_id: &str) -> SqlResult<Option<Device>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, device_id, firmware_version, build_id, first_seen, last_seen
             FROM devices WHERE device_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![device_id], |row| {
            Ok(Device {
                id: row.get(0)?,
                device_id: row.get(1)?,
                firmware_version: row.get(2)?,
                build_id: row.get::<_, i64>(3)? as u64,
                first_seen: row.get(4)?,
                last_seen: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    // ---- Fault events ----

    #[allow(clippy::too_many_arguments)]
    pub fn insert_fault(
        &self,
        device_rowid: i64,
        fault_type: u8,
        pc: u32,
        lr: u32,
        cfsr: u32,
        hfsr: u32,
        mmfar: u32,
        bfar: u32,
        sp: u32,
        stack_snapshot: &[u32],
        symbol: Option<&str>,
    ) -> SqlResult<i64> {
        let snapshot_json = serde_json::to_string(stack_snapshot).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO fault_events
             (device_rowid, fault_type, pc, lr, cfsr, hfsr, mmfar, bfar, sp, stack_snapshot, symbol)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                device_rowid,
                fault_type as i64,
                pc as i64,
                lr as i64,
                cfsr as i64,
                hfsr as i64,
                mmfar as i64,
                bfar as i64,
                sp as i64,
                snapshot_json,
                symbol,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_faults_for_device(&self, device_id: &str) -> SqlResult<Vec<FaultEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.device_rowid, d.device_id, f.fault_type, f.pc, f.lr,
                    f.cfsr, f.hfsr, f.mmfar, f.bfar, f.sp, f.stack_snapshot, f.symbol, f.created_at
             FROM fault_events f
             JOIN devices d ON d.id = f.device_rowid
             WHERE d.device_id = ?1
             ORDER BY f.created_at DESC
             LIMIT 100",
        )?;
        let rows = stmt.query_map(params![device_id], |row| {
            Ok(FaultEvent {
                id: row.get(0)?,
                device_rowid: row.get(1)?,
                device_id: row.get(2)?,
                fault_type: row.get::<_, i64>(3)? as u8,
                pc: row.get::<_, i64>(4)? as u32,
                lr: row.get::<_, i64>(5)? as u32,
                cfsr: row.get::<_, i64>(6)? as u32,
                hfsr: row.get::<_, i64>(7)? as u32,
                mmfar: row.get::<_, i64>(8)? as u32,
                bfar: row.get::<_, i64>(9)? as u32,
                sp: row.get::<_, i64>(10)? as u32,
                stack_snapshot: row.get(11)?,
                symbol: row.get(12)?,
                created_at: row.get(13)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_all_faults(&self, limit: usize) -> SqlResult<Vec<FaultEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.device_rowid, d.device_id, f.fault_type, f.pc, f.lr,
                    f.cfsr, f.hfsr, f.mmfar, f.bfar, f.sp, f.stack_snapshot, f.symbol, f.created_at
             FROM fault_events f
             JOIN devices d ON d.id = f.device_rowid
             ORDER BY f.created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(FaultEvent {
                id: row.get(0)?,
                device_rowid: row.get(1)?,
                device_id: row.get(2)?,
                fault_type: row.get::<_, i64>(3)? as u8,
                pc: row.get::<_, i64>(4)? as u32,
                lr: row.get::<_, i64>(5)? as u32,
                cfsr: row.get::<_, i64>(6)? as u32,
                hfsr: row.get::<_, i64>(7)? as u32,
                mmfar: row.get::<_, i64>(8)? as u32,
                bfar: row.get::<_, i64>(9)? as u32,
                sp: row.get::<_, i64>(10)? as u32,
                stack_snapshot: row.get(11)?,
                symbol: row.get(12)?,
                created_at: row.get(13)?,
            })
        })?;
        rows.collect()
    }

    // ---- Metrics ----

    pub fn insert_metric(
        &self,
        device_rowid: i64,
        key: &str,
        metric_type: u8,
        value_json: &str,
        timestamp_ticks: u64,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO metrics (device_rowid, key, metric_type, value_json, timestamp_ticks)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                device_rowid,
                key,
                metric_type as i64,
                value_json,
                timestamp_ticks as i64
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_metrics_for_device(&self, device_id: &str) -> SqlResult<Vec<MetricRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.device_rowid, d.device_id, m.key, m.metric_type,
                    m.value_json, m.timestamp_ticks, m.created_at
             FROM metrics m
             JOIN devices d ON d.id = m.device_rowid
             WHERE d.device_id = ?1
             ORDER BY m.created_at DESC
             LIMIT 200",
        )?;
        let rows = stmt.query_map(params![device_id], |row| {
            Ok(MetricRow {
                id: row.get(0)?,
                device_rowid: row.get(1)?,
                device_id: row.get(2)?,
                key: row.get(3)?,
                metric_type: row.get::<_, i64>(4)? as u8,
                value_json: row.get(5)?,
                timestamp_ticks: row.get::<_, i64>(6)? as u64,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_all_metrics(&self, limit: usize) -> SqlResult<Vec<MetricRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.device_rowid, d.device_id, m.key, m.metric_type,
                    m.value_json, m.timestamp_ticks, m.created_at
             FROM metrics m
             JOIN devices d ON d.id = m.device_rowid
             ORDER BY m.created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(MetricRow {
                id: row.get(0)?,
                device_rowid: row.get(1)?,
                device_id: row.get(2)?,
                key: row.get(3)?,
                metric_type: row.get::<_, i64>(4)? as u8,
                value_json: row.get(5)?,
                timestamp_ticks: row.get::<_, i64>(6)? as u64,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    // ---- Reboot events ----

    pub fn insert_reboot(
        &self,
        device_rowid: i64,
        reason: u8,
        extra: u8,
        boot_sequence: u32,
        uptime_before_reboot: u32,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO reboot_events (device_rowid, reason, extra, boot_sequence, uptime_before_reboot)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                device_rowid,
                reason as i64,
                extra as i64,
                boot_sequence as i64,
                uptime_before_reboot as i64,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_reboots_for_device(&self, device_id: &str) -> SqlResult<Vec<RebootEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.device_rowid, d.device_id, r.reason, r.extra,
                    r.boot_sequence, r.uptime_before_reboot, r.created_at
             FROM reboot_events r
             JOIN devices d ON d.id = r.device_rowid
             WHERE d.device_id = ?1
             ORDER BY r.created_at DESC
             LIMIT 100",
        )?;
        let rows = stmt.query_map(params![device_id], |row| {
            Ok(RebootEvent {
                id: row.get(0)?,
                device_rowid: row.get(1)?,
                device_id: row.get(2)?,
                reason: row.get::<_, i64>(3)? as u8,
                extra: row.get::<_, i64>(4)? as u8,
                boot_sequence: row.get::<_, i64>(5)? as u32,
                uptime_before_reboot: row.get::<_, i64>(6)? as u32,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Count of fault events for a device.
    pub fn count_faults_for_device(&self, device_rowid: i64) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM fault_events WHERE device_rowid = ?1",
            params![device_rowid],
            |row| row.get(0),
        )
    }

    /// Count of metric rows for a device.
    pub fn count_metrics_for_device(&self, device_rowid: i64) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM metrics WHERE device_rowid = ?1",
            params![device_rowid],
            |row| row.get(0),
        )
    }

    /// Count of reboot events for a device.
    pub fn count_reboots_for_device(&self, device_rowid: i64) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM reboot_events WHERE device_rowid = ?1",
            params![device_rowid],
            |row| row.get(0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_tables_idempotent() {
        let store = Store::open_in_memory().unwrap();
        // calling create_tables again should be fine
        store.create_tables().unwrap();
    }

    #[test]
    fn upsert_and_list_devices() {
        let store = Store::open_in_memory().unwrap();
        let id1 = store.upsert_device("dev-001", "1.0.0", 0xABCD).unwrap();
        let id2 = store.upsert_device("dev-002", "1.1.0", 0x1234).unwrap();
        assert_ne!(id1, id2);

        let devices = store.list_devices().unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn upsert_device_updates_firmware() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_device("dev-001", "1.0.0", 100).unwrap();
        store.upsert_device("dev-001", "2.0.0", 200).unwrap();

        let dev = store.get_device_by_id("dev-001").unwrap().unwrap();
        assert_eq!(dev.firmware_version, "2.0.0");
        assert_eq!(dev.build_id, 200);
    }

    #[test]
    fn touch_device_creates_placeholder() {
        let store = Store::open_in_memory().unwrap();
        let rowid = store.touch_device("new-device").unwrap();
        assert!(rowid > 0);

        let dev = store.get_device_by_id("new-device").unwrap().unwrap();
        assert_eq!(dev.firmware_version, "");
    }

    #[test]
    fn insert_and_list_faults() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        let f1 = store
            .insert_fault(
                dev,
                0,
                0x0800_2000,
                0x0800_1000,
                0x400,
                0x4000_0000,
                0,
                0,
                0x2000_3F00,
                &[0xDEAD; 4],
                Some("main+0x20"),
            )
            .unwrap();
        assert!(f1 > 0);

        let faults = store.list_faults_for_device("dev-001").unwrap();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].pc, 0x0800_2000);
        assert_eq!(faults[0].symbol, Some("main+0x20".to_string()));
    }

    #[test]
    fn insert_and_list_metrics() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        store
            .insert_metric(dev, "temperature", 1, r#"{"value":23.5}"#, 1000)
            .unwrap();
        store
            .insert_metric(dev, "uptime", 0, r#"{"value":42}"#, 2000)
            .unwrap();

        let metrics = store.list_metrics_for_device("dev-001").unwrap();
        assert_eq!(metrics.len(), 2);
    }

    #[test]
    fn insert_and_list_reboots() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        store.insert_reboot(dev, 4, 0, 42, 100_000).unwrap();

        let reboots = store.list_reboots_for_device("dev-001").unwrap();
        assert_eq!(reboots.len(), 1);
        assert_eq!(reboots[0].reason, 4);
        assert_eq!(reboots[0].boot_sequence, 42);
    }

    #[test]
    fn count_queries() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        assert_eq!(store.count_faults_for_device(dev).unwrap(), 0);
        assert_eq!(store.count_metrics_for_device(dev).unwrap(), 0);
        assert_eq!(store.count_reboots_for_device(dev).unwrap(), 0);

        store
            .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None)
            .unwrap();
        store.insert_metric(dev, "k", 0, "{}", 0).unwrap();
        store.insert_reboot(dev, 1, 0, 1, 0).unwrap();

        assert_eq!(store.count_faults_for_device(dev).unwrap(), 1);
        assert_eq!(store.count_metrics_for_device(dev).unwrap(), 1);
        assert_eq!(store.count_reboots_for_device(dev).unwrap(), 1);
    }

    #[test]
    fn list_all_faults_with_limit() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        for _ in 0..5 {
            store
                .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None)
                .unwrap();
        }

        let faults = store.list_all_faults(3).unwrap();
        assert_eq!(faults.len(), 3);
    }

    #[test]
    fn list_all_metrics_with_limit() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        for i in 0..5 {
            store
                .insert_metric(dev, &format!("key_{i}"), 0, "{}", i as u64)
                .unwrap();
        }

        let metrics = store.list_all_metrics(2).unwrap();
        assert_eq!(metrics.len(), 2);
    }

    #[test]
    fn get_nonexistent_device_returns_none() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_device_by_id("nope").unwrap().is_none());
    }
}
