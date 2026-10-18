//! Delivery to `POST /ingest/chunks`, with an on-disk spool while offline.
//!
//! Each report is one self-contained batch (it starts with `DeviceInfo`).
//! When the server can't be reached, batches go to `<state_dir>/spool/` and
//! are sent oldest first once it can. The spool is capped; the oldest
//! reports go first. 401/403/404/429 are treated like an outage (a wrong API
//! key or URL gets fixed, rate limits pass); any other 4xx means the server
//! will never take the batch, so it is dropped rather than retried forever.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

pub trait Transport {
    fn send(&self, body: &[u8]) -> Delivery;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Accepted,
    /// The server refused the batch itself: retrying won't help.
    Rejected(u16),
    /// Network error, 5xx, or a fixable 4xx: keep the batch and retry later.
    Failed,
}

pub struct HttpTransport {
    agent: ureq::Agent,
    url: String,
    api_key: Option<String>,
}

impl HttpTransport {
    pub fn new(server_url: &str, api_key: Option<String>, timeout: Duration) -> Result<Self> {
        let agent = ureq::AgentBuilder::new()
            .timeout(timeout)
            .user_agent(concat!("ferrite-agent/", env!("CARGO_PKG_VERSION")))
            .build();
        Ok(Self {
            agent,
            url: format!("{}/ingest/chunks", server_url.trim_end_matches('/')),
            api_key,
        })
    }
}

impl Transport for HttpTransport {
    fn send(&self, body: &[u8]) -> Delivery {
        let mut req = self
            .agent
            .post(&self.url)
            .set("Content-Type", "application/octet-stream");
        if let Some(key) = &self.api_key {
            req = req.set("X-API-Key", key);
        }
        match req.send_bytes(body) {
            Ok(resp) => classify_status(resp.status()),
            Err(ureq::Error::Status(status, _)) => classify_status(status),
            Err(e) => {
                tracing::warn!(error = %e, "server unreachable");
                Delivery::Failed
            }
        }
    }
}

pub fn classify_status(status: u16) -> Delivery {
    match status {
        200..=299 => Delivery::Accepted,
        401 | 403 | 404 | 429 => {
            tracing::warn!(
                status,
                "server refused the report (check api_key and server_url); keeping it"
            );
            Delivery::Failed
        }
        400..=499 => Delivery::Rejected(status),
        _ => {
            tracing::warn!(status, "server error");
            Delivery::Failed
        }
    }
}

pub struct Spool {
    dir: PathBuf,
    max: usize,
    next: u64,
}

impl Spool {
    pub fn open(dir: &Path, max: usize) -> Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let next = Self::entries(dir)?.last().map_or(0, |(n, _)| n + 1);
        Ok(Self {
            dir: dir.to_path_buf(),
            max,
            next,
        })
    }

    /// Spooled batches, oldest first.
    fn entries(dir: &Path) -> Result<Vec<(u64, PathBuf)>> {
        let mut out: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let n = name.strip_suffix(".chunks")?.parse().ok()?;
                Some((n, e.path()))
            })
            .collect();
        out.sort();
        Ok(out)
    }

    pub fn len(&self) -> usize {
        Self::entries(&self.dir).map_or(0, |e| e.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, batch: &[u8]) -> Result<()> {
        let path = self.dir.join(format!("{:020}.chunks", self.next));
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, batch)?;
        std::fs::rename(&tmp, &path)?;
        self.next += 1;
        let entries = Self::entries(&self.dir)?;
        if entries.len() > self.max {
            let excess = entries.len() - self.max;
            for (_, p) in &entries[..excess] {
                let _ = std::fs::remove_file(p);
            }
            tracing::warn!(dropped = excess, "spool full; dropped oldest reports");
        }
        Ok(())
    }

    /// Send spooled batches oldest first; stop at the first failure.
    /// Returns how many were delivered (or dropped as rejected).
    pub fn flush(&mut self, transport: &dyn Transport) -> Result<usize> {
        let mut done = 0;
        for (_, path) in Self::entries(&self.dir)? {
            let body = std::fs::read(&path)?;
            match transport.send(&body) {
                Delivery::Accepted => {}
                Delivery::Rejected(status) => {
                    tracing::warn!(status, file = %path.display(), "server rejected spooled report; dropped");
                }
                Delivery::Failed => break,
            }
            std::fs::remove_file(&path)?;
            done += 1;
        }
        Ok(done)
    }
}

/// Send a fresh batch after any spooled ones, spooling it if that fails.
pub fn deliver(transport: &dyn Transport, spool: &mut Spool, batch: &[u8]) -> Result<Delivery> {
    if !spool.is_empty() {
        let sent = spool.flush(transport)?;
        if sent > 0 {
            tracing::info!(sent, "delivered spooled reports");
        }
        if !spool.is_empty() {
            // Still offline: keep order by queueing behind the backlog.
            spool.push(batch)?;
            return Ok(Delivery::Failed);
        }
    }
    let result = transport.send(batch);
    match result {
        Delivery::Accepted => {}
        Delivery::Rejected(status) => {
            tracing::error!(status, "server rejected report; dropped");
        }
        Delivery::Failed => spool.push(batch)?,
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        up: RefCell<bool>,
        reject: bool,
        got: RefCell<Vec<Vec<u8>>>,
    }

    impl Fake {
        fn new(up: bool) -> Self {
            Self {
                up: RefCell::new(up),
                reject: false,
                got: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for Fake {
        fn send(&self, body: &[u8]) -> Delivery {
            if !*self.up.borrow() {
                return Delivery::Failed;
            }
            if self.reject {
                return Delivery::Rejected(400);
            }
            self.got.borrow_mut().push(body.to_vec());
            Delivery::Accepted
        }
    }

    #[test]
    fn spools_while_offline_and_delivers_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 10).unwrap();
        let fake = Fake::new(false);
        for b in [b"a", b"b", b"c"] {
            assert_eq!(deliver(&fake, &mut spool, b).unwrap(), Delivery::Failed);
        }
        assert_eq!(spool.len(), 3);
        *fake.up.borrow_mut() = true;
        assert_eq!(
            deliver(&fake, &mut spool, b"d").unwrap(),
            Delivery::Accepted
        );
        assert!(spool.is_empty());
        assert_eq!(
            *fake.got.borrow(),
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]
        );
    }

    #[test]
    fn spool_cap_drops_oldest_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 2).unwrap();
        for b in [b"1", b"2", b"3"] {
            spool.push(b).unwrap();
        }
        drop(spool);
        let mut spool = Spool::open(dir.path(), 2).unwrap();
        spool.push(b"4").unwrap();
        let fake = Fake::new(true);
        spool.flush(&fake).unwrap();
        assert_eq!(*fake.got.borrow(), vec![b"3".to_vec(), b"4".to_vec()]);
    }

    #[test]
    fn status_classes() {
        assert_eq!(classify_status(200), Delivery::Accepted);
        assert_eq!(classify_status(401), Delivery::Failed);
        assert_eq!(classify_status(404), Delivery::Failed);
        assert_eq!(classify_status(400), Delivery::Rejected(400));
        assert_eq!(classify_status(413), Delivery::Rejected(413));
        assert_eq!(classify_status(503), Delivery::Failed);
    }

    #[test]
    fn rejected_reports_are_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 10).unwrap();
        spool.push(b"old").unwrap();
        let fake = Fake {
            reject: true,
            ..Fake::new(true)
        };
        assert_eq!(
            deliver(&fake, &mut spool, b"new").unwrap(),
            Delivery::Rejected(400)
        );
        assert!(spool.is_empty());
    }
}
