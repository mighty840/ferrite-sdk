//! Per-device credentials (#118): each device's AES-128-CCM key, and which
//! sealed chunks the server has already accepted.
//!
//! They live in their own SQLite file, like the evidence log: retention and
//! the demo reset never touch it, so a device keeps its key when its
//! telemetry is wiped.
//!
//! Replay protection works per key and boot epoch (see
//! `ferrite_sdk::encryption`): the newest [`RECENT_EPOCHS`] epochs keep a
//! window of [`REPLAY_WINDOW`] counters, so chunks that arrive out of order
//! (a gateway draining its buffer) are still accepted once. Older epochs keep
//! only their highest counter and accept nothing at or below it.

use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

/// Counters per boot epoch the server remembers individually.
pub const REPLAY_WINDOW: u32 = 4096;
/// Boot epochs per key that keep their full window.
const RECENT_EPOCHS: i64 = 32;
const WINDOW_BYTES: usize = (REPLAY_WINDOW / 8) as usize;

/// What the server tells clients about a credential. Never the key.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialInfo {
    pub device_id: String,
    pub key_id: u32,
    pub created_at: String,
    pub created_by: String,
    pub last_used_at: Option<String>,
}

/// A newly issued key, returned to the provisioner once.
pub struct IssuedCredential {
    pub key_id: u32,
    pub key: [u8; 16],
    /// The device had a key before; this one replaces it.
    pub replaced_key_id: Option<u32>,
}

/// Outcome of [`CredentialStore::accept_counter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Not seen before; now recorded.
    Fresh,
    /// This exact chunk was accepted before.
    Replayed,
    /// Too old to tell; rejected.
    Stale,
}

pub struct CredentialStore {
    conn: Mutex<Connection>,
}

impl CredentialStore {
    pub fn open(path: &Path) -> SqlResult<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> SqlResult<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS device_credentials (
                 device_id    TEXT PRIMARY KEY,
                 key_id       INTEGER NOT NULL UNIQUE,
                 secret       BLOB NOT NULL,
                 created_at   TEXT NOT NULL DEFAULT (datetime('now')),
                 created_by   TEXT NOT NULL,
                 last_used_at TEXT
             );
             CREATE TABLE IF NOT EXISTS credential_epochs (
                 key_id      INTEGER NOT NULL,
                 boot_epoch  INTEGER NOT NULL,
                 max_counter INTEGER NOT NULL,
                 seen        BLOB NOT NULL,
                 updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                 PRIMARY KEY (key_id, boot_epoch)
             );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Issue a new key for `device_id`, replacing any key it had.
    pub fn issue(&self, device_id: &str, actor: &str) -> SqlResult<IssuedCredential> {
        let conn = self.conn();
        let replaced_key_id: Option<u32> = conn
            .query_row(
                "SELECT key_id FROM device_credentials WHERE device_id = ?1",
                params![device_id],
                |r| r.get(0),
            )
            .optional()?;

        let mut key = [0u8; 16];
        getrandom::getrandom(&mut key).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(format!("no randomness: {e}").into())
        })?;
        // Key IDs travel in clear in every nonce; 0 marks the shared key.
        let key_id = loop {
            let mut b = [0u8; 4];
            getrandom::getrandom(&mut b).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(format!("no randomness: {e}").into())
            })?;
            let id = u32::from_le_bytes(b);
            let taken: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM device_credentials WHERE key_id = ?1)",
                params![id],
                |r| r.get(0),
            )?;
            if id != 0 && !taken {
                break id;
            }
        };

        conn.execute(
            "INSERT INTO device_credentials (device_id, key_id, secret, created_by)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(device_id) DO UPDATE SET
                 key_id = excluded.key_id, secret = excluded.secret,
                 created_at = datetime('now'), created_by = excluded.created_by,
                 last_used_at = NULL",
            params![device_id, key_id, key.as_slice(), actor],
        )?;
        if let Some(old) = replaced_key_id {
            conn.execute(
                "DELETE FROM credential_epochs WHERE key_id = ?1",
                params![old],
            )?;
        }
        Ok(IssuedCredential {
            key_id,
            key,
            replaced_key_id,
        })
    }

    /// Remove a device's key. Returns the key ID it had.
    pub fn revoke(&self, device_id: &str) -> SqlResult<Option<u32>> {
        let conn = self.conn();
        let key_id: Option<u32> = conn
            .query_row(
                "DELETE FROM device_credentials WHERE device_id = ?1 RETURNING key_id",
                params![device_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = key_id {
            conn.execute(
                "DELETE FROM credential_epochs WHERE key_id = ?1",
                params![id],
            )?;
        }
        Ok(key_id)
    }

    pub fn get(&self, device_id: &str) -> SqlResult<Option<CredentialInfo>> {
        self.conn()
            .query_row(
                "SELECT device_id, key_id, created_at, created_by, last_used_at
                 FROM device_credentials WHERE device_id = ?1",
                params![device_id],
                |r| {
                    Ok(CredentialInfo {
                        device_id: r.get(0)?,
                        key_id: r.get(1)?,
                        created_at: r.get(2)?,
                        created_by: r.get(3)?,
                        last_used_at: r.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn has_credential(&self, device_id: &str) -> SqlResult<bool> {
        self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM device_credentials WHERE device_id = ?1)",
            params![device_id],
            |r| r.get(0),
        )
    }

    /// The device and key behind a key ID.
    pub fn key_for(&self, key_id: u32) -> SqlResult<Option<(String, [u8; 16])>> {
        let row: Option<(String, Vec<u8>)> = self
            .conn()
            .query_row(
                "SELECT device_id, secret FROM device_credentials WHERE key_id = ?1",
                params![key_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.and_then(|(device, secret)| Some((device, secret.try_into().ok()?))))
    }

    /// Record `counter` for `(key_id, boot_epoch)` unless it was seen before
    /// or is too old to tell. Call only after the chunk's tag verified.
    pub fn accept_counter(
        &self,
        key_id: u32,
        boot_epoch: u32,
        counter: u32,
    ) -> SqlResult<Freshness> {
        let conn = self.conn();
        let row: Option<(u32, Vec<u8>)> = conn
            .query_row(
                "SELECT max_counter, seen FROM credential_epochs
                 WHERE key_id = ?1 AND boot_epoch = ?2",
                params![key_id, boot_epoch],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let (max, seen) = match row {
            None => {
                let mut seen = vec![0u8; WINDOW_BYTES];
                set_bit(&mut seen, 0);
                conn.execute(
                    "INSERT INTO credential_epochs (key_id, boot_epoch, max_counter, seen)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![key_id, boot_epoch, counter, seen],
                )?;
                // Older boots keep only their highest counter.
                conn.execute(
                    "UPDATE credential_epochs SET seen = x'' WHERE key_id = ?1 AND boot_epoch NOT IN
                         (SELECT boot_epoch FROM credential_epochs WHERE key_id = ?1
                          ORDER BY updated_at DESC, rowid DESC LIMIT ?2)",
                    params![key_id, RECENT_EPOCHS],
                )?;
                touch(&conn, key_id)?;
                return Ok(Freshness::Fresh);
            }
            Some(r) => r,
        };

        let outcome = window_accept(max, seen, counter);
        if let Some((new_max, new_seen)) = outcome.1 {
            conn.execute(
                "UPDATE credential_epochs SET max_counter = ?3, seen = ?4, updated_at = datetime('now')
                 WHERE key_id = ?1 AND boot_epoch = ?2",
                params![key_id, boot_epoch, new_max, new_seen],
            )?;
            touch(&conn, key_id)?;
        }
        Ok(outcome.0)
    }
}

fn touch(conn: &Connection, key_id: u32) -> SqlResult<()> {
    conn.execute(
        "UPDATE device_credentials SET last_used_at = datetime('now') WHERE key_id = ?1",
        params![key_id],
    )?;
    Ok(())
}

/// Sliding-window check: bit `i` of `seen` stands for counter `max - i`.
/// An empty `seen` (an old epoch) accepts only counters above `max`.
fn window_accept(max: u32, seen: Vec<u8>, counter: u32) -> (Freshness, Option<(u32, Vec<u8>)>) {
    let windowed = seen.len() == WINDOW_BYTES;
    if counter > max {
        let shift = counter - max;
        let mut next = vec![0u8; WINDOW_BYTES];
        if windowed && shift < REPLAY_WINDOW {
            for i in 0..REPLAY_WINDOW - shift {
                if get_bit(&seen, i) {
                    set_bit(&mut next, i + shift);
                }
            }
        }
        set_bit(&mut next, 0);
        return (Freshness::Fresh, Some((counter, next)));
    }
    let age = max - counter;
    if !windowed || age >= REPLAY_WINDOW {
        return (Freshness::Stale, None);
    }
    if get_bit(&seen, age) {
        return (Freshness::Replayed, None);
    }
    let mut seen = seen;
    set_bit(&mut seen, age);
    (Freshness::Fresh, Some((max, seen)))
}

fn get_bit(bits: &[u8], i: u32) -> bool {
    bits[(i / 8) as usize] & (1 << (i % 8)) != 0
}

fn set_bit(bits: &mut [u8], i: u32) {
    bits[(i / 8) as usize] |= 1 << (i % 8);
}

// ---------------------------------------------------------------------------
// HTTP API
// ---------------------------------------------------------------------------

use crate::auth::UserClaims;
use crate::{evidence, AppState};
use axum::{
    extract::{Path as UrlPath, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use std::sync::Arc;

/// `POST /devices/:id/credential` (provisioner or admin): issue a new key for
/// the device, replacing any key it had. The key is in this response only.
pub async fn issue_credential(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<UserClaims>>,
    UrlPath(device_id): UrlPath<String>,
) -> impl IntoResponse {
    if device_id.is_empty() || device_id.len() > 127 || device_id == "unknown" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid device_id" })),
        );
    }
    let actor = evidence::user_actor(claims.as_deref());
    match state.credentials.issue(&device_id, &actor) {
        Ok(issued) => {
            let kind = if issued.replaced_key_id.is_some() {
                "device.credential_rotated"
            } else {
                "device.credential_set"
            };
            evidence::record(
                &state.evidence,
                kind,
                &format!("device:{device_id}"),
                &actor,
                serde_json::json!({
                    "key_id": issued.key_id,
                    "replaced_key_id": issued.replaced_key_id,
                }),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "device_id": device_id,
                    "key_id": issued.key_id,
                    "key": hex::encode(issued.key),
                    "rotated": issued.replaced_key_id.is_some(),
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("issuing credential: {e}") })),
        ),
    }
}

/// `GET /devices/:id/credential`: whether the device has a key, and when it
/// was issued and last used. Never the key itself.
pub async fn get_credential(
    State(state): State<Arc<AppState>>,
    UrlPath(device_id): UrlPath<String>,
) -> impl IntoResponse {
    match state.credentials.get(&device_id) {
        Ok(Some(info)) => (StatusCode::OK, Json(serde_json::json!(info))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device has no credential" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("reading credential: {e}") })),
        ),
    }
}

/// `DELETE /devices/:id/credential` (admin): revoke the device's key. Its
/// sealed chunks are rejected from now on; unsealed ones are accepted again
/// unless `DEVICE_AUTH_REQUIRED` is set.
pub async fn revoke_credential(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<UserClaims>>,
    UrlPath(device_id): UrlPath<String>,
) -> impl IntoResponse {
    match state.credentials.revoke(&device_id) {
        Ok(Some(key_id)) => {
            evidence::record(
                &state.evidence,
                "device.credential_revoked",
                &format!("device:{device_id}"),
                &evidence::user_actor(claims.as_deref()),
                serde_json::json!({ "key_id": key_id }),
            );
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device has no credential" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("revoking credential: {e}") })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_rotate_revoke() {
        let store = CredentialStore::open_in_memory().unwrap();
        let first = store.issue("dev-1", "admin").unwrap();
        assert_ne!(first.key_id, 0);
        assert_eq!(first.replaced_key_id, None);
        assert_eq!(
            store.key_for(first.key_id).unwrap().unwrap(),
            ("dev-1".into(), first.key)
        );

        let second = store.issue("dev-1", "admin").unwrap();
        assert_eq!(second.replaced_key_id, Some(first.key_id));
        assert!(store.key_for(first.key_id).unwrap().is_none());
        assert!(store.has_credential("dev-1").unwrap());

        assert_eq!(store.revoke("dev-1").unwrap(), Some(second.key_id));
        assert!(!store.has_credential("dev-1").unwrap());
        assert_eq!(store.revoke("dev-1").unwrap(), None);
    }

    #[test]
    fn counters_are_accepted_once() {
        let store = CredentialStore::open_in_memory().unwrap();
        let k = store.issue("dev-1", "admin").unwrap().key_id;
        assert_eq!(store.accept_counter(k, 7, 0).unwrap(), Freshness::Fresh);
        assert_eq!(store.accept_counter(k, 7, 1).unwrap(), Freshness::Fresh);
        assert_eq!(store.accept_counter(k, 7, 1).unwrap(), Freshness::Replayed);
        // Out of order, e.g. a gateway draining its buffer late.
        assert_eq!(store.accept_counter(k, 7, 10).unwrap(), Freshness::Fresh);
        assert_eq!(store.accept_counter(k, 7, 5).unwrap(), Freshness::Fresh);
        assert_eq!(store.accept_counter(k, 7, 5).unwrap(), Freshness::Replayed);
        // Far behind the window.
        assert_eq!(
            store.accept_counter(k, 7, 10 + REPLAY_WINDOW).unwrap(),
            Freshness::Fresh
        );
        assert_eq!(store.accept_counter(k, 7, 9).unwrap(), Freshness::Stale);
        // A new boot starts its own window.
        assert_eq!(store.accept_counter(k, 8, 0).unwrap(), Freshness::Fresh);
    }

    #[test]
    fn old_epochs_accept_nothing_they_could_have_sent() {
        let store = CredentialStore::open_in_memory().unwrap();
        let k = store.issue("dev-1", "admin").unwrap().key_id;
        assert_eq!(store.accept_counter(k, 1, 3).unwrap(), Freshness::Fresh);
        for epoch in 2..(2 + RECENT_EPOCHS as u32) {
            store.accept_counter(k, epoch, 0).unwrap();
        }
        // Epoch 1 has dropped out of the recent set: anything up to its
        // highest counter is now stale, even a counter it never sent.
        assert_eq!(store.accept_counter(k, 1, 2).unwrap(), Freshness::Stale);
        assert_eq!(store.accept_counter(k, 1, 3).unwrap(), Freshness::Stale);
    }
}
