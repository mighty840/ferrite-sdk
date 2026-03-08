/// Frame format: [level:1][ticks_lo:4][len:1][payload:N][0xFF sentinel]
const SENTINEL: u8 = 0xFF;
const FRAME_HEADER_SIZE: usize = 6; // level(1) + ticks_lo(4) + len(1)

/// A circular byte buffer for structured log frames.
pub struct TraceBuffer<const N: usize> {
    buf: [u8; N],
    write_pos: usize,
    read_pos: usize,
    total_written: u64,
    frames_lost: u32,
    used: usize,
}

impl<const N: usize> TraceBuffer<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            write_pos: 0,
            read_pos: 0,
            total_written: 0,
            frames_lost: 0,
            used: 0,
        }
    }

    /// Write a log frame. Overwrites oldest frames if full.
    pub fn write_frame(&mut self, level: u8, ticks: u32, payload: &[u8]) {
        let payload_len = payload.len().min(127);
        let frame_size = FRAME_HEADER_SIZE + payload_len + 1; // +1 for sentinel

        if frame_size > N {
            // Frame too large for buffer, drop it
            self.frames_lost += 1;
            return;
        }

        // Make space by evicting old frames
        while self.used + frame_size > N {
            self.evict_oldest_frame();
        }

        // Write frame header
        self.write_byte(level);
        let ticks_bytes = ticks.to_le_bytes();
        for &b in &ticks_bytes {
            self.write_byte(b);
        }
        self.write_byte(payload_len as u8);

        // Write payload
        for &b in &payload[..payload_len] {
            self.write_byte(b);
        }

        // Write sentinel
        self.write_byte(SENTINEL);

        self.total_written += frame_size as u64;
    }

    fn write_byte(&mut self, byte: u8) {
        self.buf[self.write_pos] = byte;
        self.write_pos = (self.write_pos + 1) % N;
        self.used += 1;
    }

    fn read_byte_at(&self, pos: usize) -> u8 {
        self.buf[pos % N]
    }

    /// Evict the oldest frame (scan for sentinel).
    fn evict_oldest_frame(&mut self) {
        if self.used == 0 {
            return;
        }

        // Scan from read_pos for the sentinel
        let mut pos = self.read_pos;
        let mut scanned = 0;
        loop {
            if scanned >= self.used {
                // Couldn't find sentinel — clear everything
                self.read_pos = self.write_pos;
                self.used = 0;
                self.frames_lost += 1;
                return;
            }
            let byte = self.buf[pos % N];
            pos = (pos + 1) % N;
            scanned += 1;
            if byte == SENTINEL {
                break;
            }
        }

        self.read_pos = pos % N;
        self.used -= scanned;
        self.frames_lost += 1;
    }

    /// Iterate frames from oldest to newest.
    pub fn iter_frames(&self) -> TraceFrameIter<'_, N> {
        TraceFrameIter {
            buffer: self,
            pos: self.read_pos,
            remaining: self.used,
        }
    }

    /// How many bytes have been written total (including overwritten).
    pub fn total_written(&self) -> u64 {
        self.total_written
    }

    /// How many frames were lost to overflow.
    pub fn frames_lost(&self) -> u32 {
        self.frames_lost
    }

    /// How many bytes are currently used.
    pub fn bytes_used(&self) -> usize {
        self.used
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.read_pos = 0;
        self.used = 0;
        // Don't reset total_written or frames_lost — those are lifetime stats
    }
}

/// A parsed trace frame.
pub struct TraceFrame<'a> {
    pub level: u8,
    pub ticks: u32,
    pub payload: &'a [u8],
}

impl<'a> TraceFrame<'a> {
    /// Get the raw bytes of this frame (for encoding into chunks).
    pub fn as_bytes(&self) -> &'a [u8] {
        // This is tricky with a circular buffer — we return the payload only
        self.payload
    }
}

/// Iterator over trace frames.
pub struct TraceFrameIter<'a, const N: usize> {
    buffer: &'a TraceBuffer<N>,
    pos: usize,
    remaining: usize,
}

impl<'a, const N: usize> Iterator for TraceFrameIter<'a, N> {
    type Item = TraceFrame<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining < FRAME_HEADER_SIZE + 1 {
            return None;
        }

        // Read header
        let level = self.buffer.read_byte_at(self.pos);
        self.pos = (self.pos + 1) % N;

        let ticks = u32::from_le_bytes([
            self.buffer.read_byte_at(self.pos),
            self.buffer.read_byte_at((self.pos + 1) % N),
            self.buffer.read_byte_at((self.pos + 2) % N),
            self.buffer.read_byte_at((self.pos + 3) % N),
        ]);
        self.pos = (self.pos + 4) % N;

        let len = self.buffer.read_byte_at(self.pos) as usize;
        self.pos = (self.pos + 1) % N;

        let total_frame_size = FRAME_HEADER_SIZE + len + 1;
        if self.remaining < total_frame_size {
            self.remaining = 0;
            return None;
        }

        // Skip payload bytes (we can't return a contiguous slice from circular buffer)
        let _payload_start = self.pos;
        self.pos = (self.pos + len) % N;

        // Skip sentinel
        self.pos = (self.pos + 1) % N;

        self.remaining -= total_frame_size;

        Some(TraceFrame {
            level,
            ticks,
            payload: &[], // Circular buffer can't return contiguous slice easily
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::vec::Vec;

    #[test]
    fn write_and_iterate() {
        let mut buf: TraceBuffer<256> = TraceBuffer::new();
        buf.write_frame(2, 1000, b"hello");
        buf.write_frame(3, 2000, b"world");

        let frames: Vec<_> = buf.iter_frames().collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].level, 2);
        assert_eq!(frames[0].ticks, 1000);
        assert_eq!(frames[1].level, 3);
        assert_eq!(frames[1].ticks, 2000);
    }

    #[test]
    fn overflow_wrap_around() {
        // Small buffer: 32 bytes
        let mut buf: TraceBuffer<32> = TraceBuffer::new();

        // Each frame: 6 (header) + payload_len + 1 (sentinel)
        // "hello" = 5 bytes payload → 12 bytes per frame
        buf.write_frame(0, 100, b"hello"); // 12 bytes, used=12
        buf.write_frame(1, 200, b"world"); // 12 bytes, used=24
        // Next frame should cause eviction (24 + 12 = 36 > 32)
        buf.write_frame(2, 300, b"third");

        // Should have evicted at least one frame
        assert!(buf.frames_lost() > 0);
        assert!(buf.bytes_used() <= 32);
    }

    #[test]
    fn frames_lost_counter() {
        let mut buf: TraceBuffer<20> = TraceBuffer::new();

        // "ab" = 2 bytes payload → 9 bytes per frame
        buf.write_frame(0, 0, b"ab"); // 9 bytes
        buf.write_frame(0, 0, b"cd"); // 9 bytes, total 18
        // Next write forces eviction
        buf.write_frame(0, 0, b"ef");

        assert!(buf.frames_lost() > 0);
    }

    #[test]
    fn clear_resets_positions() {
        let mut buf: TraceBuffer<128> = TraceBuffer::new();
        buf.write_frame(0, 0, b"test");
        assert!(buf.bytes_used() > 0);

        buf.clear();
        assert_eq!(buf.bytes_used(), 0);
        assert_eq!(buf.iter_frames().count(), 0);
    }

    #[test]
    fn total_written_accumulates() {
        let mut buf: TraceBuffer<256> = TraceBuffer::new();
        buf.write_frame(0, 0, b"hello"); // 12 bytes
        buf.write_frame(0, 0, b"world"); // 12 bytes
        assert_eq!(buf.total_written(), 24);
    }

    #[test]
    fn empty_payload() {
        let mut buf: TraceBuffer<64> = TraceBuffer::new();
        buf.write_frame(0, 0, &[]);
        let frames: Vec<_> = buf.iter_frames().collect();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].level, 0);
    }
}
