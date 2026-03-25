use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
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
    pub device_key: Option<i64>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub tags: Option<String>,
    pub provisioned_by: Option<String>,
    pub provisioned_at: Option<String>,
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
    pub crash_group_id: Option<i64>,
}

/// A crash group that deduplicates faults by signature (fault_type + PC).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashGroup {
    pub id: i64,
    pub signature_hash: String,
    pub pc: u32,
    pub fault_type: u8,
    pub first_seen: String,
    pub last_seen: String,
    pub occurrence_count: i64,
    pub affected_device_count: i64,
    pub title: Option<String>,
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

/// A device group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub device_count: i64,
    pub created_at: String,
}

/// An OTA firmware target for a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaTarget {
    pub id: i64,
    pub device_id: String,
    pub target_version: String,
    pub target_build_id: i64,
    pub firmware_url: Option<String>,
    pub created_at: String,
}

/// An OTA campaign for rolling out firmware to a fleet of devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaCampaign {
    pub id: i64,
    pub name: String,
    pub firmware_id: i64,
    pub target_version: String,
    pub strategy: String,
    pub target_group_id: Option<i64>,
    pub target_tags: Option<String>,
    pub rollout_percent: i64,
    pub failure_threshold: f64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A device's status within an OTA campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaCampaignDevice {
    pub id: i64,
    pub campaign_id: i64,
    pub device_id: String,
    pub status: String,
    pub updated_at: String,
}

/// A campaign with aggregated device status counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignSummary {
    pub campaign: OtaCampaign,
    pub pending: i64,
    pub downloading: i64,
    pub installed: i64,
    pub failed: i64,
}

/// A firmware binary artifact stored on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareArtifact {
    pub id: i64,
    pub version: String,
    pub build_id: i64,
    pub sha256: String,
    pub size: i64,
    pub filename: String,
    pub signer: Option<String>,
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
        store.migrate();
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.create_tables()?;
        store.migrate();
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
                device_rowid    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
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
                device_rowid    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
                key             TEXT NOT NULL,
                metric_type     INTEGER NOT NULL,
                value_json      TEXT NOT NULL,
                timestamp_ticks INTEGER NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS reboot_events (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                device_rowid          INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
                reason                INTEGER NOT NULL,
                extra                 INTEGER NOT NULL DEFAULT 0,
                boot_sequence         INTEGER NOT NULL DEFAULT 0,
                uptime_before_reboot  INTEGER NOT NULL DEFAULT 0,
                created_at            TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_fault_device ON fault_events(device_rowid);
            CREATE INDEX IF NOT EXISTS idx_metrics_device ON metrics(device_rowid);
            CREATE INDEX IF NOT EXISTS idx_reboot_device ON reboot_events(device_rowid);

            CREATE TABLE IF NOT EXISTS device_groups (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                description TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS ota_targets (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id       TEXT NOT NULL UNIQUE,
                target_version  TEXT NOT NULL,
                target_build_id INTEGER NOT NULL DEFAULT 0,
                firmware_url    TEXT,
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS device_group_members (
                group_id    INTEGER NOT NULL REFERENCES device_groups(id) ON DELETE CASCADE,
                device_id   INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
                added_at    TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (group_id, device_id)
            );

            CREATE TABLE IF NOT EXISTS crash_groups (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                signature_hash        TEXT NOT NULL UNIQUE,
                pc                    INTEGER NOT NULL,
                fault_type            INTEGER NOT NULL,
                first_seen            TEXT NOT NULL DEFAULT (datetime('now')),
                last_seen             TEXT NOT NULL DEFAULT (datetime('now')),
                occurrence_count      INTEGER NOT NULL DEFAULT 1,
                affected_device_count INTEGER NOT NULL DEFAULT 1,
                title                 TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_crash_groups_count ON crash_groups(occurrence_count DESC);

            CREATE TABLE IF NOT EXISTS firmware_artifacts (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                version     TEXT NOT NULL,
                build_id    INTEGER NOT NULL DEFAULT 0,
                sha256      TEXT NOT NULL,
                size        INTEGER NOT NULL,
                filename    TEXT NOT NULL,
                signer      TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS ota_campaigns (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                name              TEXT NOT NULL,
                firmware_id       INTEGER NOT NULL REFERENCES firmware_artifacts(id),
                target_version    TEXT NOT NULL,
                strategy          TEXT NOT NULL DEFAULT 'immediate',
                target_group_id   INTEGER REFERENCES device_groups(id),
                target_tags       TEXT,
                rollout_percent   INTEGER NOT NULL DEFAULT 100,
                failure_threshold REAL NOT NULL DEFAULT 5.0,
                status            TEXT NOT NULL DEFAULT 'created',
                created_at        TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS ota_campaign_devices (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                campaign_id     INTEGER NOT NULL REFERENCES ota_campaigns(id) ON DELETE CASCADE,
                device_id       TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'pending',
                updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(campaign_id, device_id)
            );
            ",
        )?;
        Ok(())
    }

    /// Run idempotent migrations (ALTER TABLE ADD COLUMN, ignoring errors for existing columns).
    fn migrate(&self) {
        let columns = [
            "ALTER TABLE devices ADD COLUMN device_key INTEGER",
            "ALTER TABLE devices ADD COLUMN name TEXT",
            "ALTER TABLE devices ADD COLUMN status TEXT DEFAULT 'unknown'",
            "ALTER TABLE devices ADD COLUMN tags TEXT",
            "ALTER TABLE devices ADD COLUMN provisioned_by TEXT",
            "ALTER TABLE devices ADD COLUMN provisioned_at TEXT",
            "ALTER TABLE fault_events ADD COLUMN crash_group_id INTEGER REFERENCES crash_groups(id)",
        ];
        for sql in &columns {
            let _ = self.conn.execute(sql, []);
        }
        // Index on device_key for lookups
        let _ = self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_devices_key ON devices(device_key)",
            [],
        );
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

    const DEVICE_COLUMNS: &str =
        "id, device_id, firmware_version, build_id, first_seen, last_seen, device_key, name, status, tags, provisioned_by, provisioned_at";

    fn device_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
        Ok(Device {
            id: row.get(0)?,
            device_id: row.get(1)?,
            firmware_version: row.get(2)?,
            build_id: row.get::<_, i64>(3)? as u64,
            first_seen: row.get(4)?,
            last_seen: row.get(5)?,
            device_key: row.get(6)?,
            name: row.get(7)?,
            status: row.get(8)?,
            tags: row.get(9)?,
            provisioned_by: row.get(10)?,
            provisioned_at: row.get(11)?,
        })
    }

    pub fn list_devices(&self) -> SqlResult<Vec<Device>> {
        let sql = format!(
            "SELECT {} FROM devices ORDER BY last_seen DESC",
            Self::DEVICE_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::device_from_row)?;
        rows.collect()
    }

    pub fn get_device_by_id(&self, device_id: &str) -> SqlResult<Option<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE device_id = ?1",
            Self::DEVICE_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![device_id], Self::device_from_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn get_device_by_key(&self, device_key: i64) -> SqlResult<Option<Device>> {
        let sql = format!(
            "SELECT {} FROM devices WHERE device_key = ?1",
            Self::DEVICE_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![device_key], Self::device_from_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Begin a transaction. Returns a guard that auto-rolls-back on drop unless committed.
    pub fn begin_transaction(&self) -> SqlResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")
    }

    /// Commit the current transaction.
    pub fn commit_transaction(&self) -> SqlResult<()> {
        self.conn.execute_batch("COMMIT")
    }

    /// Rollback the current transaction.
    pub fn rollback_transaction(&self) -> SqlResult<()> {
        self.conn.execute_batch("ROLLBACK")
    }

    /// Register a new device by device_key. Returns the row id.
    pub fn register_device(
        &self,
        device_key: i64,
        name: Option<&str>,
        tags: Option<&str>,
        provisioned_by: Option<&str>,
    ) -> SqlResult<i64> {
        let hex_id = format!("{:08X}", device_key as u32);
        self.conn.execute(
            "INSERT INTO devices (device_id, device_key, name, tags, provisioned_by, provisioned_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), 'provisioned')
             ON CONFLICT(device_id) DO UPDATE SET
                device_key = excluded.device_key,
                name = COALESCE(excluded.name, devices.name),
                tags = COALESCE(excluded.tags, devices.tags),
                provisioned_by = COALESCE(excluded.provisioned_by, devices.provisioned_by),
                provisioned_at = COALESCE(excluded.provisioned_at, devices.provisioned_at),
                status = COALESCE(excluded.status, devices.status),
                last_seen = datetime('now')",
            params![hex_id, device_key, name, tags, provisioned_by],
        )?;
        let rowid = self.conn.query_row(
            "SELECT id FROM devices WHERE device_id = ?1",
            params![hex_id],
            |row| row.get(0),
        )?;
        Ok(rowid)
    }

    /// Update a device's mutable fields by device_key.
    pub fn update_device(
        &self,
        device_key: i64,
        name: Option<&str>,
        tags: Option<&str>,
    ) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE devices SET
                name = COALESCE(?2, name),
                tags = COALESCE(?3, tags),
                last_seen = datetime('now')
             WHERE device_key = ?1",
            params![device_key, name, tags],
        )?;
        Ok(changed > 0)
    }

    /// Delete a device by device_key.
    pub fn delete_device(&self, device_key: i64) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "DELETE FROM devices WHERE device_key = ?1",
            params![device_key],
        )?;
        Ok(changed > 0)
    }

    /// Delete a device by device_id string.
    pub fn delete_device_by_id(&self, device_id: &str) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "DELETE FROM devices WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(changed > 0)
    }

    /// Touch a device by device_key and update its status.
    pub fn touch_device_by_key(&self, device_key: i64, status: &str) -> SqlResult<Option<i64>> {
        let changed = self.conn.execute(
            "UPDATE devices SET status = ?2, last_seen = datetime('now') WHERE device_key = ?1",
            params![device_key, status],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let rowid = self.conn.query_row(
            "SELECT id FROM devices WHERE device_key = ?1",
            params![device_key],
            |row| row.get(0),
        )?;
        Ok(Some(rowid))
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
        crash_group_id: Option<i64>,
    ) -> SqlResult<i64> {
        let snapshot_json = serde_json::to_string(stack_snapshot).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO fault_events
             (device_rowid, fault_type, pc, lr, cfsr, hfsr, mmfar, bfar, sp, stack_snapshot, symbol, crash_group_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                crash_group_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn fault_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FaultEvent> {
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
            crash_group_id: row.get(14)?,
        })
    }

    pub fn list_faults_for_device(&self, device_id: &str) -> SqlResult<Vec<FaultEvent>> {
        self.list_faults_for_device_paginated(device_id, 100, 0, None, None)
    }

    pub fn list_faults_for_device_paginated(
        &self,
        device_id: &str,
        limit: usize,
        offset: usize,
        since: Option<&str>,
        until: Option<&str>,
    ) -> SqlResult<Vec<FaultEvent>> {
        let mut sql = String::from(
            "SELECT f.id, f.device_rowid, d.device_id, f.fault_type, f.pc, f.lr,
                    f.cfsr, f.hfsr, f.mmfar, f.bfar, f.sp, f.stack_snapshot, f.symbol, f.created_at, f.crash_group_id
             FROM fault_events f
             JOIN devices d ON d.id = f.device_rowid
             WHERE d.device_id = ?1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(device_id.to_string()));
        if let Some(s) = since {
            param_values.push(Box::new(s.to_string()));
            sql.push_str(&format!(" AND f.created_at >= ?{}", param_values.len()));
        }
        if let Some(u) = until {
            param_values.push(Box::new(u.to_string()));
            sql.push_str(&format!(" AND f.created_at < ?{}", param_values.len()));
        }
        param_values.push(Box::new(limit as i64));
        let limit_idx = param_values.len();
        param_values.push(Box::new(offset as i64));
        let offset_idx = param_values.len();
        sql.push_str(&format!(
            " ORDER BY f.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        ));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_ref.as_slice(), Self::fault_from_row)?;
        rows.collect()
    }

    pub fn list_all_faults(&self, limit: usize) -> SqlResult<Vec<FaultEvent>> {
        self.list_all_faults_paginated(limit, 0, None, None)
    }

    pub fn list_all_faults_paginated(
        &self,
        limit: usize,
        offset: usize,
        since: Option<&str>,
        until: Option<&str>,
    ) -> SqlResult<Vec<FaultEvent>> {
        let mut sql = String::from(
            "SELECT f.id, f.device_rowid, d.device_id, f.fault_type, f.pc, f.lr,
                    f.cfsr, f.hfsr, f.mmfar, f.bfar, f.sp, f.stack_snapshot, f.symbol, f.created_at, f.crash_group_id
             FROM fault_events f
             JOIN devices d ON d.id = f.device_rowid
             WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(s) = since {
            param_values.push(Box::new(s.to_string()));
            sql.push_str(&format!(" AND f.created_at >= ?{}", param_values.len()));
        }
        if let Some(u) = until {
            param_values.push(Box::new(u.to_string()));
            sql.push_str(&format!(" AND f.created_at < ?{}", param_values.len()));
        }
        param_values.push(Box::new(limit as i64));
        let limit_idx = param_values.len();
        param_values.push(Box::new(offset as i64));
        let offset_idx = param_values.len();
        sql.push_str(&format!(
            " ORDER BY f.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        ));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_ref.as_slice(), Self::fault_from_row)?;
        rows.collect()
    }

    // ---- Crash groups ----

    /// Compute a deterministic signature hash from fault_type and PC.
    fn compute_signature_hash(fault_type: u8, pc: u32) -> String {
        format!("{:02x}{:08x}", fault_type, pc)
    }

    /// Find or create a crash group for the given fault signature.
    /// Upserts the crash group and returns its id.
    pub fn find_or_create_crash_group(
        &self,
        fault_type: u8,
        pc: u32,
        title: Option<&str>,
        _device_rowid: i64,
    ) -> SqlResult<i64> {
        let sig = Self::compute_signature_hash(fault_type, pc);
        self.conn.execute(
            "INSERT INTO crash_groups (signature_hash, pc, fault_type, title)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(signature_hash) DO UPDATE SET
                last_seen = datetime('now'),
                occurrence_count = crash_groups.occurrence_count + 1,
                title = COALESCE(?4, crash_groups.title)",
            params![sig, pc as i64, fault_type as i64, title],
        )?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM crash_groups WHERE signature_hash = ?1",
            params![sig],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Update the affected_device_count for a crash group based on distinct devices in fault_events.
    pub fn update_crash_group_device_count(&self, crash_group_id: i64) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE crash_groups SET affected_device_count = (
                SELECT COUNT(DISTINCT device_rowid) FROM fault_events WHERE crash_group_id = ?1
             ) WHERE id = ?1",
            params![crash_group_id],
        )?;
        Ok(())
    }

    /// List crash groups ordered by occurrence_count descending.
    pub fn list_crash_groups(&self, limit: usize, offset: usize) -> SqlResult<Vec<CrashGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, signature_hash, pc, fault_type, first_seen, last_seen,
                    occurrence_count, affected_device_count, title
             FROM crash_groups
             ORDER BY occurrence_count DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(CrashGroup {
                id: row.get(0)?,
                signature_hash: row.get(1)?,
                pc: row.get::<_, i64>(2)? as u32,
                fault_type: row.get::<_, i64>(3)? as u8,
                first_seen: row.get(4)?,
                last_seen: row.get(5)?,
                occurrence_count: row.get(6)?,
                affected_device_count: row.get(7)?,
                title: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    /// Get a single crash group by id.
    pub fn get_crash_group(&self, id: i64) -> SqlResult<Option<CrashGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, signature_hash, pc, fault_type, first_seen, last_seen,
                    occurrence_count, affected_device_count, title
             FROM crash_groups
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(CrashGroup {
                id: row.get(0)?,
                signature_hash: row.get(1)?,
                pc: row.get::<_, i64>(2)? as u32,
                fault_type: row.get::<_, i64>(3)? as u8,
                first_seen: row.get(4)?,
                last_seen: row.get(5)?,
                occurrence_count: row.get(6)?,
                affected_device_count: row.get(7)?,
                title: row.get(8)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// List fault events belonging to a specific crash group.
    pub fn list_faults_for_crash_group(
        &self,
        crash_group_id: i64,
        limit: usize,
        offset: usize,
    ) -> SqlResult<Vec<FaultEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.device_rowid, d.device_id, f.fault_type, f.pc, f.lr,
                    f.cfsr, f.hfsr, f.mmfar, f.bfar, f.sp, f.stack_snapshot, f.symbol, f.created_at, f.crash_group_id
             FROM fault_events f
             JOIN devices d ON d.id = f.device_rowid
             WHERE f.crash_group_id = ?1
             ORDER BY f.created_at DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(
            params![crash_group_id, limit as i64, offset as i64],
            Self::fault_from_row,
        )?;
        rows.collect()
    }

    /// Count total number of crash groups.
    pub fn count_crash_groups(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM crash_groups", [], |row| row.get(0))
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
        self.list_metrics_for_device_paginated(device_id, 200, 0, None, None)
    }

    pub fn list_metrics_for_device_paginated(
        &self,
        device_id: &str,
        limit: usize,
        offset: usize,
        since: Option<&str>,
        until: Option<&str>,
    ) -> SqlResult<Vec<MetricRow>> {
        let mut sql = String::from(
            "SELECT m.id, m.device_rowid, d.device_id, m.key, m.metric_type,
                    m.value_json, m.timestamp_ticks, m.created_at
             FROM metrics m
             JOIN devices d ON d.id = m.device_rowid
             WHERE d.device_id = ?1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(device_id.to_string()));
        if let Some(s) = since {
            param_values.push(Box::new(s.to_string()));
            sql.push_str(&format!(" AND m.created_at >= ?{}", param_values.len()));
        }
        if let Some(u) = until {
            param_values.push(Box::new(u.to_string()));
            sql.push_str(&format!(" AND m.created_at < ?{}", param_values.len()));
        }
        param_values.push(Box::new(limit as i64));
        let limit_idx = param_values.len();
        param_values.push(Box::new(offset as i64));
        let offset_idx = param_values.len();
        sql.push_str(&format!(
            " ORDER BY m.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        ));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
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
        self.list_all_metrics_paginated(limit, 0, None, None)
    }

    pub fn list_all_metrics_paginated(
        &self,
        limit: usize,
        offset: usize,
        since: Option<&str>,
        until: Option<&str>,
    ) -> SqlResult<Vec<MetricRow>> {
        let mut sql = String::from(
            "SELECT m.id, m.device_rowid, d.device_id, m.key, m.metric_type,
                    m.value_json, m.timestamp_ticks, m.created_at
             FROM metrics m
             JOIN devices d ON d.id = m.device_rowid
             WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(s) = since {
            param_values.push(Box::new(s.to_string()));
            sql.push_str(&format!(" AND m.created_at >= ?{}", param_values.len()));
        }
        if let Some(u) = until {
            param_values.push(Box::new(u.to_string()));
            sql.push_str(&format!(" AND m.created_at < ?{}", param_values.len()));
        }
        param_values.push(Box::new(limit as i64));
        let limit_idx = param_values.len();
        param_values.push(Box::new(offset as i64));
        let offset_idx = param_values.len();
        sql.push_str(&format!(
            " ORDER BY m.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        ));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
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

    /// Update device status by row id.
    pub fn update_device_status(&self, device_id: i64, status: &str) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE devices SET status = ?2 WHERE id = ?1",
            params![device_id, status],
        )?;
        Ok(changed > 0)
    }

    /// Update device status and last_seen by device_id string.
    pub fn update_device_status_by_id(&self, device_id: &str, status: &str) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE devices SET status = ?2, last_seen = datetime('now') WHERE device_id = ?1",
            params![device_id, status],
        )?;
        Ok(changed > 0)
    }

    /// Get datetime('now', modifier) from SQLite. Used for time comparisons.
    pub fn datetime_now_offset(&self, modifier: &str) -> SqlResult<String> {
        self.conn.query_row(
            &format!("SELECT datetime('now', '{}')", modifier.replace('\'', "")),
            [],
            |row| row.get(0),
        )
    }

    // ---- Global counts (for Prometheus) ----

    pub fn count_all_faults(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM fault_events", [], |row| row.get(0))
    }

    pub fn count_all_metrics(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM metrics", [], |row| row.get(0))
    }

    pub fn count_all_reboots(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM reboot_events", [], |row| row.get(0))
    }

    pub fn count_all_groups(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM device_groups", [], |row| row.get(0))
    }

    // ---- OTA targets ----

    /// Set (upsert) an OTA firmware target for a device.
    pub fn set_ota_target(
        &self,
        device_id: &str,
        target_version: &str,
        target_build_id: i64,
        firmware_url: Option<&str>,
    ) -> SqlResult<OtaTarget> {
        self.conn.execute(
            "INSERT INTO ota_targets (device_id, target_version, target_build_id, firmware_url)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(device_id) DO UPDATE SET
                target_version = excluded.target_version,
                target_build_id = excluded.target_build_id,
                firmware_url = COALESCE(excluded.firmware_url, ota_targets.firmware_url),
                created_at = datetime('now')",
            params![device_id, target_version, target_build_id, firmware_url],
        )?;
        self.get_ota_target_for_device(device_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Get the OTA target for a specific device.
    pub fn get_ota_target_for_device(&self, device_id: &str) -> SqlResult<Option<OtaTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, device_id, target_version, target_build_id, firmware_url, created_at
             FROM ota_targets WHERE device_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![device_id], |row| {
            Ok(OtaTarget {
                id: row.get(0)?,
                device_id: row.get(1)?,
                target_version: row.get(2)?,
                target_build_id: row.get(3)?,
                firmware_url: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// List all OTA targets.
    pub fn list_ota_targets(&self) -> SqlResult<Vec<OtaTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, device_id, target_version, target_build_id, firmware_url, created_at
             FROM ota_targets ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(OtaTarget {
                id: row.get(0)?,
                device_id: row.get(1)?,
                target_version: row.get(2)?,
                target_build_id: row.get(3)?,
                firmware_url: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Delete an OTA target for a device.
    pub fn delete_ota_target(&self, device_id: &str) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "DELETE FROM ota_targets WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(changed > 0)
    }

    // ---- Firmware Artifacts ----

    pub fn insert_firmware_artifact(
        &self,
        version: &str,
        build_id: i64,
        sha256: &str,
        size: i64,
        filename: &str,
        signer: Option<&str>,
    ) -> SqlResult<FirmwareArtifact> {
        self.conn.execute(
            "INSERT INTO firmware_artifacts (version, build_id, sha256, size, filename, signer)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![version, build_id, sha256, size, filename, signer],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.query_row(
            "SELECT id, version, build_id, sha256, size, filename, signer, created_at
             FROM firmware_artifacts WHERE id = ?1",
            params![id],
            |row| {
                Ok(FirmwareArtifact {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    build_id: row.get(2)?,
                    sha256: row.get(3)?,
                    size: row.get(4)?,
                    filename: row.get(5)?,
                    signer: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
    }

    pub fn list_firmware_artifacts(&self) -> SqlResult<Vec<FirmwareArtifact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, version, build_id, sha256, size, filename, signer, created_at
             FROM firmware_artifacts ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FirmwareArtifact {
                id: row.get(0)?,
                version: row.get(1)?,
                build_id: row.get(2)?,
                sha256: row.get(3)?,
                size: row.get(4)?,
                filename: row.get(5)?,
                signer: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_firmware_artifact(&self, id: i64) -> SqlResult<Option<FirmwareArtifact>> {
        self.conn
            .query_row(
                "SELECT id, version, build_id, sha256, size, filename, signer, created_at
                 FROM firmware_artifacts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(FirmwareArtifact {
                        id: row.get(0)?,
                        version: row.get(1)?,
                        build_id: row.get(2)?,
                        sha256: row.get(3)?,
                        size: row.get(4)?,
                        filename: row.get(5)?,
                        signer: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()
    }

    pub fn delete_firmware_artifact(&self, id: i64) -> SqlResult<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM firmware_artifacts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    // ---- OTA Campaigns ----

    #[allow(clippy::too_many_arguments)]
    pub fn create_campaign(
        &self,
        name: &str,
        firmware_id: i64,
        target_version: &str,
        strategy: &str,
        target_group_id: Option<i64>,
        target_tags: Option<&str>,
        rollout_percent: i64,
        failure_threshold: f64,
    ) -> SqlResult<OtaCampaign> {
        self.conn.execute(
            "INSERT INTO ota_campaigns (name, firmware_id, target_version, strategy, target_group_id, target_tags, rollout_percent, failure_threshold)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![name, firmware_id, target_version, strategy, target_group_id, target_tags, rollout_percent, failure_threshold],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_campaign(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    }

    const CAMPAIGN_COLUMNS: &str =
        "id, name, firmware_id, target_version, strategy, target_group_id, target_tags, rollout_percent, failure_threshold, status, created_at, updated_at";

    fn campaign_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OtaCampaign> {
        Ok(OtaCampaign {
            id: row.get(0)?,
            name: row.get(1)?,
            firmware_id: row.get(2)?,
            target_version: row.get(3)?,
            strategy: row.get(4)?,
            target_group_id: row.get(5)?,
            target_tags: row.get(6)?,
            rollout_percent: row.get(7)?,
            failure_threshold: row.get(8)?,
            status: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }

    pub fn get_campaign(&self, id: i64) -> SqlResult<Option<OtaCampaign>> {
        let sql = format!(
            "SELECT {} FROM ota_campaigns WHERE id = ?1",
            Self::CAMPAIGN_COLUMNS
        );
        self.conn
            .query_row(&sql, params![id], Self::campaign_from_row)
            .optional()
    }

    pub fn list_campaigns(&self) -> SqlResult<Vec<OtaCampaign>> {
        let sql = format!(
            "SELECT {} FROM ota_campaigns ORDER BY created_at DESC",
            Self::CAMPAIGN_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::campaign_from_row)?;
        rows.collect()
    }

    pub fn update_campaign_status(&self, id: i64, status: &str) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE ota_campaigns SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![id, status],
        )?;
        Ok(changed > 0)
    }

    pub fn add_devices_to_campaign(
        &self,
        campaign_id: i64,
        device_ids: &[String],
    ) -> SqlResult<usize> {
        let mut count = 0usize;
        for device_id in device_ids {
            let result = self.conn.execute(
                "INSERT OR IGNORE INTO ota_campaign_devices (campaign_id, device_id) VALUES (?1, ?2)",
                params![campaign_id, device_id],
            )?;
            count += result;
        }
        Ok(count)
    }

    pub fn get_campaign_device_status(
        &self,
        campaign_id: i64,
        device_id: &str,
    ) -> SqlResult<Option<OtaCampaignDevice>> {
        self.conn
            .query_row(
                "SELECT id, campaign_id, device_id, status, updated_at
                 FROM ota_campaign_devices
                 WHERE campaign_id = ?1 AND device_id = ?2",
                params![campaign_id, device_id],
                |row| {
                    Ok(OtaCampaignDevice {
                        id: row.get(0)?,
                        campaign_id: row.get(1)?,
                        device_id: row.get(2)?,
                        status: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn update_campaign_device_status(
        &self,
        campaign_id: i64,
        device_id: &str,
        status: &str,
    ) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE ota_campaign_devices SET status = ?3, updated_at = datetime('now')
             WHERE campaign_id = ?1 AND device_id = ?2",
            params![campaign_id, device_id, status],
        )?;
        Ok(changed > 0)
    }

    pub fn get_campaign_summary(&self, campaign_id: i64) -> SqlResult<Option<CampaignSummary>> {
        let campaign = match self.get_campaign(campaign_id)? {
            Some(c) => c,
            None => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT
                 COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN status = 'downloading' THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN status = 'installed' THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0)
             FROM ota_campaign_devices WHERE campaign_id = ?1",
        )?;
        let summary = stmt.query_row(params![campaign_id], |row| {
            Ok(CampaignSummary {
                campaign,
                pending: row.get(0)?,
                downloading: row.get(1)?,
                installed: row.get(2)?,
                failed: row.get(3)?,
            })
        })?;
        Ok(Some(summary))
    }

    pub fn list_campaign_devices(&self, campaign_id: i64) -> SqlResult<Vec<OtaCampaignDevice>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, campaign_id, device_id, status, updated_at
             FROM ota_campaign_devices
             WHERE campaign_id = ?1
             ORDER BY device_id",
        )?;
        let rows = stmt.query_map(params![campaign_id], |row| {
            Ok(OtaCampaignDevice {
                id: row.get(0)?,
                campaign_id: row.get(1)?,
                device_id: row.get(2)?,
                status: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Find active campaigns that include a given device.
    pub fn find_active_campaigns_for_device(&self, device_id: &str) -> SqlResult<Vec<OtaCampaign>> {
        let sql =
            "SELECT c.id, c.name, c.firmware_id, c.target_version, c.strategy, c.target_group_id,
                    c.target_tags, c.rollout_percent, c.failure_threshold, c.status, c.created_at, c.updated_at
             FROM ota_campaigns c
             JOIN ota_campaign_devices cd ON cd.campaign_id = c.id
             WHERE cd.device_id = ?1 AND c.status = 'active'"
            .to_string();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![device_id], |row| Self::campaign_from_row(row))?;
        rows.collect()
    }

    /// Check if all devices in a campaign are done (installed or failed).
    /// Returns true if no devices are still pending or downloading.
    pub fn campaign_all_devices_done(&self, campaign_id: i64) -> SqlResult<bool> {
        let remaining: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM ota_campaign_devices
             WHERE campaign_id = ?1 AND status IN ('pending', 'downloading')",
            params![campaign_id],
            |row| row.get(0),
        )?;
        Ok(remaining == 0)
    }

    /// Delete all OTA targets for devices in a campaign.
    pub fn delete_ota_targets_for_campaign(&self, campaign_id: i64) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM ota_targets WHERE device_id IN (
                SELECT device_id FROM ota_campaign_devices WHERE campaign_id = ?1
             )",
            params![campaign_id],
        )
    }

    // ---- Data retention purge ----

    /// Delete metrics older than the given SQLite date modifier (e.g. "-90 days").
    pub fn purge_old_metrics(&self, age_modifier: &str) -> SqlResult<usize> {
        self.conn.execute(
            &format!(
                "DELETE FROM metrics WHERE created_at < datetime('now', '{}')",
                age_modifier.replace('\'', "")
            ),
            [],
        )
    }

    /// Delete fault events older than the given SQLite date modifier.
    pub fn purge_old_faults(&self, age_modifier: &str) -> SqlResult<usize> {
        self.conn.execute(
            &format!(
                "DELETE FROM fault_events WHERE created_at < datetime('now', '{}')",
                age_modifier.replace('\'', "")
            ),
            [],
        )
    }

    /// Delete reboot events older than the given SQLite date modifier.
    pub fn purge_old_reboots(&self, age_modifier: &str) -> SqlResult<usize> {
        self.conn.execute(
            &format!(
                "DELETE FROM reboot_events WHERE created_at < datetime('now', '{}')",
                age_modifier.replace('\'', "")
            ),
            [],
        )
    }

    // ---- Device groups ----

    pub fn create_group(&self, name: &str, description: Option<&str>) -> SqlResult<DeviceGroup> {
        self.conn.execute(
            "INSERT INTO device_groups (name, description) VALUES (?1, ?2)",
            params![name, description],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_group(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn list_groups(&self) -> SqlResult<Vec<DeviceGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.name, g.description,
                    (SELECT COUNT(*) FROM device_group_members m WHERE m.group_id = g.id),
                    g.created_at
             FROM device_groups g
             ORDER BY g.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DeviceGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                device_count: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_group(&self, group_id: i64) -> SqlResult<Option<DeviceGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.name, g.description,
                    (SELECT COUNT(*) FROM device_group_members m WHERE m.group_id = g.id),
                    g.created_at
             FROM device_groups g
             WHERE g.id = ?1",
        )?;
        let mut rows = stmt.query_map(params![group_id], |row| {
            Ok(DeviceGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                device_count: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_group(
        &self,
        group_id: i64,
        name: Option<&str>,
        description: Option<&str>,
    ) -> SqlResult<bool> {
        let changed = self.conn.execute(
            "UPDATE device_groups SET
                name = COALESCE(?2, name),
                description = COALESCE(?3, description)
             WHERE id = ?1",
            params![group_id, name, description],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_group(&self, group_id: i64) -> SqlResult<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM device_groups WHERE id = ?1", params![group_id])?;
        Ok(changed > 0)
    }

    pub fn list_group_devices(&self, group_id: i64) -> SqlResult<Vec<Device>> {
        let sql =
            "SELECT d.id, d.device_id, d.firmware_version, d.build_id, d.first_seen, d.last_seen,
                    d.device_key, d.name, d.status, d.tags, d.provisioned_by, d.provisioned_at
             FROM devices d
             JOIN device_group_members m ON m.device_id = d.id
             WHERE m.group_id = ?1
             ORDER BY d.last_seen DESC";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![group_id], Self::device_from_row)?;
        rows.collect()
    }

    pub fn add_device_to_group(&self, group_id: i64, device_id: &str) -> SqlResult<bool> {
        // Resolve device_id string to row id.
        let dev_rowid: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM devices WHERE device_id = ?1",
                params![device_id],
                |row| row.get(0),
            )
            .ok();

        let Some(dev_rowid) = dev_rowid else {
            return Ok(false);
        };

        // Check group exists.
        let group_exists: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM device_groups WHERE id = ?1",
                params![group_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;

        if !group_exists {
            return Ok(false);
        }

        self.conn.execute(
            "INSERT INTO device_group_members (group_id, device_id) VALUES (?1, ?2)",
            params![group_id, dev_rowid],
        )?;
        Ok(true)
    }

    pub fn remove_device_from_group(&self, group_id: i64, device_id: &str) -> SqlResult<bool> {
        let dev_rowid: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM devices WHERE device_id = ?1",
                params![device_id],
                |row| row.get(0),
            )
            .ok();

        let Some(dev_rowid) = dev_rowid else {
            return Ok(false);
        };

        let changed = self.conn.execute(
            "DELETE FROM device_group_members WHERE group_id = ?1 AND device_id = ?2",
            params![group_id, dev_rowid],
        )?;
        Ok(changed > 0)
    }

    // ---- Backup ----

    /// Create a backup of the database and return it as bytes.
    pub fn backup_to_bytes(&self) -> SqlResult<Vec<u8>> {
        let tmp = std::env::temp_dir().join(format!("ferrite-backup-{}.db", std::process::id()));
        {
            let mut dest = Connection::open(&tmp)?;
            let backup = rusqlite::backup::Backup::new(&self.conn, &mut dest)?;
            backup.run_to_completion(100, std::time::Duration::from_millis(10), None)?;
        }
        let output = std::fs::read(&tmp)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let _ = std::fs::remove_file(&tmp);
        Ok(output)
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
                None,
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
            .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None, None)
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
                .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None, None)
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

    // ---- Device groups tests ----

    #[test]
    fn create_and_list_groups() {
        let store = Store::open_in_memory().unwrap();
        let group = store.create_group("fleet-a", Some("Main fleet")).unwrap();
        assert_eq!(group.name, "fleet-a");
        assert_eq!(group.description, Some("Main fleet".into()));
        assert_eq!(group.device_count, 0);

        let groups = store.list_groups().unwrap();
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn update_and_delete_group() {
        let store = Store::open_in_memory().unwrap();
        let group = store.create_group("fleet-b", None).unwrap();

        assert!(store
            .update_group(group.id, Some("fleet-b-renamed"), Some("desc"))
            .unwrap());
        let updated = store.get_group(group.id).unwrap().unwrap();
        assert_eq!(updated.name, "fleet-b-renamed");

        assert!(store.delete_group(group.id).unwrap());
        assert!(store.get_group(group.id).unwrap().is_none());
    }

    #[test]
    fn group_membership() {
        let store = Store::open_in_memory().unwrap();
        let group = store.create_group("fleet-c", None).unwrap();
        store.upsert_device("dev-001", "1.0.0", 1).unwrap();
        store.upsert_device("dev-002", "1.0.0", 1).unwrap();

        // Add devices
        assert!(store.add_device_to_group(group.id, "dev-001").unwrap());
        assert!(store.add_device_to_group(group.id, "dev-002").unwrap());

        let members = store.list_group_devices(group.id).unwrap();
        assert_eq!(members.len(), 2);

        // Check device count
        let g = store.get_group(group.id).unwrap().unwrap();
        assert_eq!(g.device_count, 2);

        // Remove one
        assert!(store.remove_device_from_group(group.id, "dev-001").unwrap());
        let members = store.list_group_devices(group.id).unwrap();
        assert_eq!(members.len(), 1);
    }

    #[test]
    fn add_nonexistent_device_to_group_returns_false() {
        let store = Store::open_in_memory().unwrap();
        let group = store.create_group("fleet-d", None).unwrap();
        assert!(!store.add_device_to_group(group.id, "nonexistent").unwrap());
    }

    // ---- Global count tests ----

    #[test]
    fn global_counts() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.count_all_faults().unwrap(), 0);
        assert_eq!(store.count_all_metrics().unwrap(), 0);
        assert_eq!(store.count_all_reboots().unwrap(), 0);
        assert_eq!(store.count_all_groups().unwrap(), 0);

        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();
        store
            .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None, None)
            .unwrap();
        store.insert_metric(dev, "k", 0, "{}", 0).unwrap();
        store.insert_reboot(dev, 1, 0, 1, 0).unwrap();
        store.create_group("g1", None).unwrap();

        assert_eq!(store.count_all_faults().unwrap(), 1);
        assert_eq!(store.count_all_metrics().unwrap(), 1);
        assert_eq!(store.count_all_reboots().unwrap(), 1);
        assert_eq!(store.count_all_groups().unwrap(), 1);
    }

    // ---- Retention purge tests ----

    #[test]
    fn purge_old_data() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        // Insert some data
        store.insert_metric(dev, "old", 0, "{}", 0).unwrap();
        store
            .insert_fault(dev, 0, 0, 0, 0, 0, 0, 0, 0, &[], None, None)
            .unwrap();
        store.insert_reboot(dev, 1, 0, 1, 0).unwrap();

        assert_eq!(store.count_all_metrics().unwrap(), 1);
        assert_eq!(store.count_all_faults().unwrap(), 1);
        assert_eq!(store.count_all_reboots().unwrap(), 1);

        // Purge with "+1 day" — means cutoff is tomorrow, so everything is "older" than that
        let metrics_purged = store.purge_old_metrics("+1 day").unwrap();
        let faults_purged = store.purge_old_faults("+1 day").unwrap();
        let reboots_purged = store.purge_old_reboots("+1 day").unwrap();

        assert_eq!(metrics_purged, 1);
        assert_eq!(faults_purged, 1);
        assert_eq!(reboots_purged, 1);
        assert_eq!(store.count_all_metrics().unwrap(), 0);
        assert_eq!(store.count_all_faults().unwrap(), 0);
        assert_eq!(store.count_all_reboots().unwrap(), 0);
    }

    // ---- Backup test ----

    #[test]
    fn backup_to_bytes_produces_valid_sqlite() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        let bytes = store.backup_to_bytes().unwrap();
        assert!(!bytes.is_empty());
        // SQLite files start with "SQLite format 3\0"
        assert!(bytes.starts_with(b"SQLite format 3\0"));
    }

    #[test]
    fn ota_target_crud() {
        let store = Store::open_in_memory().unwrap();

        // No target initially
        let target = store.get_ota_target_for_device("dev-001").unwrap();
        assert!(target.is_none());

        // Set a target
        let t = store
            .set_ota_target("dev-001", "2.0.0", 42, Some("https://example.com/fw.bin"))
            .unwrap();
        assert_eq!(t.device_id, "dev-001");
        assert_eq!(t.target_version, "2.0.0");
        assert_eq!(t.target_build_id, 42);
        assert_eq!(
            t.firmware_url.as_deref(),
            Some("https://example.com/fw.bin")
        );

        // List targets
        let targets = store.list_ota_targets().unwrap();
        assert_eq!(targets.len(), 1);

        // Upsert updates existing
        let t2 = store.set_ota_target("dev-001", "2.1.0", 43, None).unwrap();
        assert_eq!(t2.target_version, "2.1.0");
        assert_eq!(t2.target_build_id, 43);
        // firmware_url preserved via COALESCE
        assert_eq!(
            t2.firmware_url.as_deref(),
            Some("https://example.com/fw.bin")
        );

        // Still only 1 target
        assert_eq!(store.list_ota_targets().unwrap().len(), 1);

        // Delete
        assert!(store.delete_ota_target("dev-001").unwrap());
        assert!(!store.delete_ota_target("dev-001").unwrap()); // already gone
        assert!(store.list_ota_targets().unwrap().is_empty());
    }

    // ---- Crash group tests ----

    #[test]
    fn test_crash_group_deduplication() {
        let store = Store::open_in_memory().unwrap();
        let dev = store.upsert_device("dev-001", "1.0.0", 1).unwrap();

        // Insert two faults with the same PC and fault_type.
        let cg1 = store
            .find_or_create_crash_group(3, 0x0800_2000, Some("HardFault at main+0x20"), dev)
            .unwrap();
        store
            .insert_fault(dev, 3, 0x0800_2000, 0, 0, 0, 0, 0, 0, &[], None, Some(cg1))
            .unwrap();

        let cg2 = store
            .find_or_create_crash_group(3, 0x0800_2000, None, dev)
            .unwrap();
        store
            .insert_fault(dev, 3, 0x0800_2000, 0, 0, 0, 0, 0, 0, &[], None, Some(cg2))
            .unwrap();

        // Same crash group should be reused.
        assert_eq!(cg1, cg2);

        let group = store.get_crash_group(cg1).unwrap().unwrap();
        assert_eq!(group.occurrence_count, 2);
        assert_eq!(group.pc, 0x0800_2000);
        assert_eq!(group.fault_type, 3);
        // Title should be preserved from the first insert.
        assert_eq!(group.title, Some("HardFault at main+0x20".to_string()));

        // Only one crash group total.
        assert_eq!(store.count_crash_groups().unwrap(), 1);

        // List should return it.
        let groups = store.list_crash_groups(100, 0).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].occurrence_count, 2);
    }

    #[test]
    fn test_crash_group_affected_devices() {
        let store = Store::open_in_memory().unwrap();
        let dev1 = store.upsert_device("dev-001", "1.0.0", 1).unwrap();
        let dev2 = store.upsert_device("dev-002", "1.0.0", 1).unwrap();

        // Insert faults from two different devices with the same signature.
        let cg = store
            .find_or_create_crash_group(3, 0x0800_3000, Some("UsageFault"), dev1)
            .unwrap();
        store
            .insert_fault(dev1, 3, 0x0800_3000, 0, 0, 0, 0, 0, 0, &[], None, Some(cg))
            .unwrap();
        store.update_crash_group_device_count(cg).unwrap();

        let _ = store
            .find_or_create_crash_group(3, 0x0800_3000, None, dev2)
            .unwrap();
        store
            .insert_fault(dev2, 3, 0x0800_3000, 0, 0, 0, 0, 0, 0, &[], None, Some(cg))
            .unwrap();
        store.update_crash_group_device_count(cg).unwrap();

        let group = store.get_crash_group(cg).unwrap().unwrap();
        assert_eq!(group.occurrence_count, 2);
        assert_eq!(group.affected_device_count, 2);

        // Verify faults for crash group.
        let faults = store.list_faults_for_crash_group(cg, 100, 0).unwrap();
        assert_eq!(faults.len(), 2);
    }

    // ---- OTA Campaign tests ----

    #[test]
    fn test_campaign_create_and_summary() {
        let store = Store::open_in_memory().unwrap();

        // Create a firmware artifact first (required by FK).
        let fw = store
            .insert_firmware_artifact("2.0.0", 200, "abc123", 1024, "fw.bin", None)
            .unwrap();

        // Create some devices.
        store.upsert_device("dev-001", "1.0.0", 100).unwrap();
        store.upsert_device("dev-002", "1.0.0", 100).unwrap();
        store.upsert_device("dev-003", "1.0.0", 100).unwrap();

        // Create campaign.
        let campaign = store
            .create_campaign(
                "rollout-v2",
                fw.id,
                "2.0.0",
                "immediate",
                None,
                None,
                100,
                5.0,
            )
            .unwrap();
        assert_eq!(campaign.name, "rollout-v2");
        assert_eq!(campaign.status, "created");
        assert_eq!(campaign.strategy, "immediate");
        assert_eq!(campaign.rollout_percent, 100);

        // Add devices to campaign.
        let device_ids = vec![
            "dev-001".to_string(),
            "dev-002".to_string(),
            "dev-003".to_string(),
        ];
        let added = store
            .add_devices_to_campaign(campaign.id, &device_ids)
            .unwrap();
        assert_eq!(added, 3);

        // Verify summary: all pending.
        let summary = store.get_campaign_summary(campaign.id).unwrap().unwrap();
        assert_eq!(summary.pending, 3);
        assert_eq!(summary.downloading, 0);
        assert_eq!(summary.installed, 0);
        assert_eq!(summary.failed, 0);

        // List campaigns.
        let campaigns = store.list_campaigns().unwrap();
        assert_eq!(campaigns.len(), 1);

        // List campaign devices.
        let devices = store.list_campaign_devices(campaign.id).unwrap();
        assert_eq!(devices.len(), 3);
    }

    #[test]
    fn test_campaign_device_status_update() {
        let store = Store::open_in_memory().unwrap();

        let fw = store
            .insert_firmware_artifact("2.0.0", 200, "abc123", 1024, "fw.bin", None)
            .unwrap();

        store.upsert_device("dev-001", "1.0.0", 100).unwrap();
        store.upsert_device("dev-002", "1.0.0", 100).unwrap();

        let campaign = store
            .create_campaign(
                "test-campaign",
                fw.id,
                "2.0.0",
                "canary",
                None,
                None,
                50,
                10.0,
            )
            .unwrap();

        let device_ids = vec!["dev-001".to_string(), "dev-002".to_string()];
        store
            .add_devices_to_campaign(campaign.id, &device_ids)
            .unwrap();

        // Update dev-001 to downloading.
        assert!(store
            .update_campaign_device_status(campaign.id, "dev-001", "downloading")
            .unwrap());

        let summary = store.get_campaign_summary(campaign.id).unwrap().unwrap();
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.downloading, 1);

        // Update dev-001 to installed.
        assert!(store
            .update_campaign_device_status(campaign.id, "dev-001", "installed")
            .unwrap());

        // Update dev-002 to failed.
        assert!(store
            .update_campaign_device_status(campaign.id, "dev-002", "failed")
            .unwrap());

        let summary = store.get_campaign_summary(campaign.id).unwrap().unwrap();
        assert_eq!(summary.pending, 0);
        assert_eq!(summary.downloading, 0);
        assert_eq!(summary.installed, 1);
        assert_eq!(summary.failed, 1);

        // All devices are done.
        assert!(store.campaign_all_devices_done(campaign.id).unwrap());

        // Verify individual device status.
        let dev_status = store
            .get_campaign_device_status(campaign.id, "dev-001")
            .unwrap()
            .unwrap();
        assert_eq!(dev_status.status, "installed");

        // Verify campaign status update.
        assert!(store
            .update_campaign_status(campaign.id, "completed")
            .unwrap());
        let c = store.get_campaign(campaign.id).unwrap().unwrap();
        assert_eq!(c.status, "completed");

        // Find active campaigns (should be empty since status is 'completed').
        let active = store.find_active_campaigns_for_device("dev-001").unwrap();
        assert!(active.is_empty());

        // Set to active, then find.
        store.update_campaign_status(campaign.id, "active").unwrap();
        let active = store.find_active_campaigns_for_device("dev-001").unwrap();
        assert_eq!(active.len(), 1);
    }
}
