use ferrite_sdk::trace::TraceBuffer;

pub fn write_and_iterate() -> Result<(), &'static str> {
    let mut buf: TraceBuffer<256> = TraceBuffer::new();
    buf.write_frame(2, 1000, b"hello");
    buf.write_frame(3, 2000, b"world");

    let mut count = 0;
    for frame in buf.iter_frames() {
        count += 1;
        if count == 1 && frame.level != 2 {
            return Err("first frame level wrong");
        }
        if count == 2 && frame.level != 3 {
            return Err("second frame level wrong");
        }
    }
    if count != 2 {
        return Err("expected 2 frames");
    }
    Ok(())
}

pub fn overflow_wrap() -> Result<(), &'static str> {
    let mut buf: TraceBuffer<32> = TraceBuffer::new();
    // Each frame with 5-byte payload = 12 bytes
    buf.write_frame(0, 100, b"hello");
    buf.write_frame(1, 200, b"world");
    buf.write_frame(2, 300, b"third");

    if buf.frames_lost() == 0 {
        return Err("expected frames_lost > 0");
    }
    if buf.bytes_used() > 32 {
        return Err("used bytes exceeds buffer size");
    }
    Ok(())
}
