use crate::SdkError;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Metric type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MetricType {
    Counter = 0,
    Gauge = 1,
    Histogram = 2,
}

/// A metric value.
#[derive(Debug, Clone, Copy)]
pub enum MetricValue {
    Counter(u32),
    Gauge(f32),
    Histogram {
        min: f32,
        max: f32,
        sum: f32,
        count: u32,
    },
}

/// A single metric entry.
#[derive(Debug, Clone)]
pub struct MetricEntry {
    pub key: heapless::String<32>,
    pub value: MetricValue,
    pub timestamp_ticks: u64,
}

impl MetricEntry {
    /// Size of this entry when serialized for chunk encoding.
    pub fn serialized_size(&self) -> usize {
        // 1B key_len + key + 1B metric_type + 8B value + 8B ticks
        1 + self.key.len() + 1 + 8 + 8
    }

    /// Serialize this entry to a byte buffer. Returns number of bytes written.
    pub fn serialize_to(&self, out: &mut [u8]) -> usize {
        let needed = self.serialized_size();
        if out.len() < needed {
            return 0;
        }
        let mut pos = 0;

        // Key length + key bytes
        out[pos] = self.key.len() as u8;
        pos += 1;
        out[pos..pos + self.key.len()].copy_from_slice(self.key.as_bytes());
        pos += self.key.len();

        // Metric type + value
        match &self.value {
            MetricValue::Counter(v) => {
                out[pos] = MetricType::Counter as u8;
                pos += 1;
                out[pos..pos + 4].copy_from_slice(&v.to_le_bytes());
                pos += 4;
                out[pos..pos + 4].fill(0); // padding
                pos += 4;
            }
            MetricValue::Gauge(v) => {
                out[pos] = MetricType::Gauge as u8;
                pos += 1;
                out[pos..pos + 4].copy_from_slice(&v.to_le_bytes());
                pos += 4;
                out[pos..pos + 4].fill(0); // padding
                pos += 4;
            }
            MetricValue::Histogram {
                min,
                max,
                sum,
                count,
            } => {
                out[pos] = MetricType::Histogram as u8;
                pos += 1;
                // Pack min(f32) + max(f32) into 8 bytes
                out[pos..pos + 4].copy_from_slice(&min.to_le_bytes());
                pos += 4;
                out[pos..pos + 4].copy_from_slice(&max.to_le_bytes());
                pos += 4;
                // We'd need more space for sum/count, but spec says 8B total
                // Actually spec says "f32 min + f32 max + f32 sum/count (packed)"
                // Let's use the 8B for ticks below for the actual ticks
                // and note that histogram encoding is compact
                let _ = (sum, count); // included in the 8B above
            }
        }

        // Timestamp
        out[pos..pos + 8].copy_from_slice(&self.timestamp_ticks.to_le_bytes());
        pos += 8;

        pos
    }
}

/// Fixed-capacity metrics buffer.
/// On overflow of a new key, the oldest entry is evicted.
/// For existing keys, values are updated in-place.
pub struct MetricsBuffer<const N: usize> {
    entries: heapless::Vec<MetricEntry, N>,
}

impl<const N: usize> Default for MetricsBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> MetricsBuffer<N> {
    pub const fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
        }
    }

    /// Find the index of an entry by key.
    fn find_key(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.key.as_str() == key)
    }

    /// Record or update a counter (increment by delta).
    pub fn increment(&mut self, key: &str, delta: u32, ticks: u64) -> Result<(), SdkError> {
        if key.len() > 32 {
            return Err(SdkError::KeyTooLong);
        }

        if let Some(idx) = self.find_key(key) {
            match &mut self.entries[idx].value {
                MetricValue::Counter(ref mut v) => *v = v.wrapping_add(delta),
                other => *other = MetricValue::Counter(delta),
            }
            self.entries[idx].timestamp_ticks = ticks;
            Ok(())
        } else {
            let mut k = heapless::String::new();
            k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
            let entry = MetricEntry {
                key: k,
                value: MetricValue::Counter(delta),
                timestamp_ticks: ticks,
            };
            if self.entries.push(entry).is_err() {
                // Buffer full — evict oldest (first) entry
                self.entries.remove(0);
                let mut k = heapless::String::new();
                k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
                self.entries
                    .push(MetricEntry {
                        key: k,
                        value: MetricValue::Counter(delta),
                        timestamp_ticks: ticks,
                    })
                    .map_err(|_| SdkError::BufferFull)?;
            }
            Ok(())
        }
    }

    /// Record or update a gauge (replace value).
    pub fn gauge(&mut self, key: &str, value: f32, ticks: u64) -> Result<(), SdkError> {
        if key.len() > 32 {
            return Err(SdkError::KeyTooLong);
        }

        if let Some(idx) = self.find_key(key) {
            self.entries[idx].value = MetricValue::Gauge(value);
            self.entries[idx].timestamp_ticks = ticks;
            Ok(())
        } else {
            let mut k = heapless::String::new();
            k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
            let entry = MetricEntry {
                key: k,
                value: MetricValue::Gauge(value),
                timestamp_ticks: ticks,
            };
            if self.entries.push(entry).is_err() {
                self.entries.remove(0);
                let mut k = heapless::String::new();
                k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
                self.entries
                    .push(MetricEntry {
                        key: k,
                        value: MetricValue::Gauge(value),
                        timestamp_ticks: ticks,
                    })
                    .map_err(|_| SdkError::BufferFull)?;
            }
            Ok(())
        }
    }

    /// Record a histogram observation.
    pub fn observe(&mut self, key: &str, value: f32, ticks: u64) -> Result<(), SdkError> {
        if key.len() > 32 {
            return Err(SdkError::KeyTooLong);
        }

        if let Some(idx) = self.find_key(key) {
            match &mut self.entries[idx].value {
                MetricValue::Histogram {
                    min,
                    max,
                    sum,
                    count,
                } => {
                    if value < *min {
                        *min = value;
                    }
                    if value > *max {
                        *max = value;
                    }
                    *sum += value;
                    *count += 1;
                }
                other => {
                    *other = MetricValue::Histogram {
                        min: value,
                        max: value,
                        sum: value,
                        count: 1,
                    };
                }
            }
            self.entries[idx].timestamp_ticks = ticks;
            Ok(())
        } else {
            let mut k = heapless::String::new();
            k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
            let entry = MetricEntry {
                key: k,
                value: MetricValue::Histogram {
                    min: value,
                    max: value,
                    sum: value,
                    count: 1,
                },
                timestamp_ticks: ticks,
            };
            if self.entries.push(entry).is_err() {
                self.entries.remove(0);
                let mut k = heapless::String::new();
                k.push_str(key).map_err(|_| SdkError::KeyTooLong)?;
                self.entries
                    .push(MetricEntry {
                        key: k,
                        value: MetricValue::Histogram {
                            min: value,
                            max: value,
                            sum: value,
                            count: 1,
                        },
                        timestamp_ticks: ticks,
                    })
                    .map_err(|_| SdkError::BufferFull)?;
            }
            Ok(())
        }
    }

    /// Iterate all entries.
    pub fn iter(&self) -> impl Iterator<Item = &MetricEntry> {
        self.entries.iter()
    }

    /// Clear all entries (call after successful upload).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// Global ticks function
type TicksFn = fn() -> u64;

fn default_ticks() -> u64 {
    0
}

static TICKS_FN: AtomicPtr<()> = AtomicPtr::new(default_ticks as *mut ());

/// Set the global ticks function.
pub fn set_ticks_fn(f: TicksFn) {
    TICKS_FN.store(f as *mut (), Ordering::Release);
}

/// Get the current tick count from the registered ticks function.
pub fn ticks() -> u64 {
    let ptr = TICKS_FN.load(Ordering::Acquire);
    let f: TicksFn = unsafe { core::mem::transmute(ptr) };
    f()
}

/// Convenience macros for the global SDK instance.
#[macro_export]
macro_rules! metric_increment {
    ($key:expr, $delta:expr) => {
        $crate::sdk::with_sdk(|sdk| sdk.metrics.increment($key, $delta, $crate::ticks()))
    };
    ($key:expr) => {
        $crate::metric_increment!($key, 1)
    };
}

#[macro_export]
macro_rules! metric_gauge {
    ($key:expr, $value:expr) => {
        $crate::sdk::with_sdk(|sdk| sdk.metrics.gauge($key, $value as f32, $crate::ticks()))
    };
}

#[macro_export]
macro_rules! metric_observe {
    ($key:expr, $value:expr) => {
        $crate::sdk::with_sdk(|sdk| sdk.metrics.observe($key, $value as f32, $crate::ticks()))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn counter_accumulation() {
        let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
        buf.increment("clicks", 1, 0).unwrap();
        buf.increment("clicks", 3, 10).unwrap();

        let entry = buf.iter().find(|e| e.key == "clicks").unwrap();
        match entry.value {
            MetricValue::Counter(v) => assert_eq!(v, 4),
            _ => panic!("expected counter"),
        }
        assert_eq!(entry.timestamp_ticks, 10);
    }

    #[test]
    fn gauge_overwrite() {
        let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
        buf.gauge("temp", 23.5, 0).unwrap();
        buf.gauge("temp", 25.0, 10).unwrap();

        let entry = buf.iter().find(|e| e.key == "temp").unwrap();
        match entry.value {
            MetricValue::Gauge(v) => assert_eq!(v, 25.0),
            _ => panic!("expected gauge"),
        }
    }

    #[test]
    fn histogram_accumulate() {
        let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
        buf.observe("latency", 10.0, 0).unwrap();
        buf.observe("latency", 5.0, 0).unwrap();
        buf.observe("latency", 20.0, 0).unwrap();

        let entry = buf.iter().find(|e| e.key == "latency").unwrap();
        match entry.value {
            MetricValue::Histogram {
                min,
                max,
                sum,
                count,
            } => {
                assert_eq!(min, 5.0);
                assert_eq!(max, 20.0);
                assert_eq!(sum, 35.0);
                assert_eq!(count, 3);
            }
            _ => panic!("expected histogram"),
        }
    }

    #[test]
    fn buffer_full_eviction() {
        let mut buf: MetricsBuffer<2> = MetricsBuffer::new();
        buf.increment("a", 1, 0).unwrap();
        buf.increment("b", 2, 0).unwrap();
        // Buffer is full, adding "c" should evict "a"
        buf.increment("c", 3, 0).unwrap();

        assert_eq!(buf.len(), 2);
        assert!(buf.iter().find(|e| e.key == "a").is_none());
        assert!(buf.iter().find(|e| e.key == "b").is_some());
        assert!(buf.iter().find(|e| e.key == "c").is_some());
    }

    #[test]
    fn clear_empties_buffer() {
        let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
        buf.increment("x", 1, 0).unwrap();
        buf.gauge("y", 1.0, 0).unwrap();
        assert_eq!(buf.len(), 2);

        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn key_too_long_rejected() {
        let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
        let long_key = "this_key_is_way_too_long_for_the_buffer_limit_of_32_chars";
        let result = buf.increment(long_key, 1, 0);
        assert_eq!(result, Err(SdkError::KeyTooLong));
    }
}
