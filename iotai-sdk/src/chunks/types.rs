/// Chunk type identifier in the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChunkType {
    Heartbeat = 0x01,
    Metrics = 0x02,
    FaultRecord = 0x03,
    TraceFragment = 0x04,
    RebootReason = 0x05,
    DeviceInfo = 0x06,
}

impl ChunkType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Heartbeat),
            0x02 => Some(Self::Metrics),
            0x03 => Some(Self::FaultRecord),
            0x04 => Some(Self::TraceFragment),
            0x05 => Some(Self::RebootReason),
            0x06 => Some(Self::DeviceInfo),
            _ => None,
        }
    }
}

/// Wire format chunk header (parsed, not the raw bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    pub magic: u8,
    pub version: u8,
    pub chunk_type: ChunkType,
    pub flags: u8,
    pub payload_len: u16,
    pub sequence_id: u16,
}

impl ChunkHeader {
    pub const MAGIC: u8 = 0xEC;
    pub const VERSION: u8 = 1;
    /// Size of the header in bytes on the wire.
    pub const WIRE_SIZE: usize = 8;

    pub fn is_last(&self) -> bool {
        self.flags & 0x01 != 0
    }

    pub fn is_fragment(&self) -> bool {
        self.flags & 0x02 != 0
    }
}

/// A decoded chunk with its payload.
#[derive(Debug, PartialEq)]
pub struct DecodedChunk {
    pub chunk_type: ChunkType,
    pub sequence_id: u16,
    pub is_last: bool,
    pub payload: heapless::Vec<u8, 248>,
}

/// Errors that can occur when decoding a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    BadMagic,
    BadVersion,
    CrcMismatch { expected: u16, got: u16 },
    UnknownType(u8),
    PayloadTruncated,
}
