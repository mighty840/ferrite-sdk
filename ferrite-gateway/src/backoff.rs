//! Exponential backoff for reconnect loops (USB serial, BLE).

use std::time::Duration;

/// Doubling delay between `min` and `max`, reset after a successful connection.
#[derive(Debug, Clone)]
pub struct Backoff {
    min: Duration,
    max: Duration,
    current: Duration,
    failures: u32,
}

impl Backoff {
    pub fn new(min: Duration, max: Duration) -> Self {
        Self {
            min,
            max,
            current: min,
            failures: 0,
        }
    }

    /// Delay to wait before the next attempt; doubles the following one.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        self.failures = self.failures.saturating_add(1);
        delay
    }

    /// Consecutive failures since the last reset. Used to log the first
    /// failure loudly and repeats quietly.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn reset(&mut self) {
        self.current = self.min;
        self.failures = 0;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(Duration::from_secs(1), Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_up_to_max() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(5));
        let delays: Vec<u64> = (0..5).map(|_| b.next_delay().as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 5, 5]);
        assert_eq!(b.failures(), 5);
    }

    #[test]
    fn reset_returns_to_min() {
        let mut b = Backoff::default();
        b.next_delay();
        b.next_delay();
        b.reset();
        assert_eq!(b.failures(), 0);
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }
}
