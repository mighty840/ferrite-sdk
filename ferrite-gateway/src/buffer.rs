//! Offline chunk buffer backed by SQLite.
//!
//! When the server is unreachable, chunks are stored locally and forwarded
//! when connectivity is restored.

use anyhow::Result;
use rusqlite::Connection;

/// SQLite-backed offline chunk buffer.
pub struct ChunkBuffer {
    conn: Connection,
}

impl ChunkBuffer {
    /// Open (or create) the buffer database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS buffered_chunks (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 device_id  TEXT,
                 chunk_data BLOB NOT NULL,
                 received_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )?;
        Ok(Self { conn })
    }

    /// Open an in-memory buffer (for testing).
    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self> {
        Self::open(":memory:")
    }

    /// Store a chunk for later forwarding.
    pub fn enqueue(&self, device_id: Option<&str>, chunk: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO buffered_chunks (device_id, chunk_data) VALUES (?1, ?2)",
            rusqlite::params![device_id, chunk],
        )?;
        Ok(())
    }

    /// Retrieve the oldest N buffered chunks (id, data).
    pub fn peek(&self, limit: usize) -> Result<Vec<(i64, Vec<u8>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chunk_data FROM buffered_chunks ORDER BY id ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Delete a successfully forwarded chunk by id.
    pub fn remove(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM buffered_chunks WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Number of chunks currently buffered.
    pub fn count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM buffered_chunks", [], |row| {
                row.get(0)
            })?;
        Ok(count as usize)
    }

    /// Delete all buffered chunks.
    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM buffered_chunks", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_peek_remove() {
        let buf = ChunkBuffer::open_in_memory().unwrap();
        assert_eq!(buf.count().unwrap(), 0);

        buf.enqueue(Some("dev-1"), &[0xEC, 0x01, 0x02]).unwrap();
        buf.enqueue(Some("dev-1"), &[0xEC, 0x03, 0x04]).unwrap();
        assert_eq!(buf.count().unwrap(), 2);

        let items = buf.peek(10).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].1, vec![0xEC, 0x01, 0x02]);

        buf.remove(items[0].0).unwrap();
        assert_eq!(buf.count().unwrap(), 1);
    }

    #[test]
    fn peek_respects_limit() {
        let buf = ChunkBuffer::open_in_memory().unwrap();
        for i in 0..10 {
            buf.enqueue(None, &[i]).unwrap();
        }
        let items = buf.peek(3).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn clear_removes_all() {
        let buf = ChunkBuffer::open_in_memory().unwrap();
        buf.enqueue(None, &[1]).unwrap();
        buf.enqueue(None, &[2]).unwrap();
        buf.clear().unwrap();
        assert_eq!(buf.count().unwrap(), 0);
    }
}
