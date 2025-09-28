use anyhow::Result;
use bincode::config;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

use crate::networking::PeerMessage;

#[derive(Debug, Serialize, Deserialize, IntoPrimitive, TryFromPrimitive, Clone, Copy, PartialEq, Eq)]
#[serde(into = "u16", try_from = "u16")]
#[repr(u16)]
pub enum ContentType {
    Bin,
    Json,
    HandShake,
    Unknown = 0xff,
}

impl ContentType {
    #[allow(dead_code)]
    pub fn from_message(msg: &PeerMessage) -> Self {
        use PeerMessage::*;
        match msg {
            Handshake(_) | HandshakeAck(_) => Self::HandShake,
            _ => Self::Bin,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
pub enum Flags {
    Partial = 0x1,
    Encryption = 0x2,
    ProofOfWork = 0x4,
    RESERVED = 0x8,
}
impl Flags {
    #[allow(dead_code)]
    pub fn check_in(&self, flags: u16) -> bool {
        flags & (*self as u16) != 0
    }

    #[allow(dead_code)]
    pub fn set_flag(flags: &mut u16, flag: Flags) {
        *flags |= flag as u16;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct P2ProtHeader {
    // offset: 0  len: 2
    magic: u16,
    // offset: 2  len: 2
    pub version: u16,
    // offset: 4  len: 2
    // |enc|pow|
    pub flags: u16,
    // offset: 6 len: 2
    pub content_type: ContentType,
    // offset: 8 len: 4
    pub header_ext_len: u32,
    // offset: 12 len: 4
    pub checksum: u32,
    // offset: 16 len: 8
    pub size: usize,
    // offset: 24 len: 8
    pub session_id: u64,
}

impl P2ProtHeader {
    pub fn from_bytes(slice: &[u8]) -> Result<Self> {
        let (header, n): (Self, usize) =
            bincode::serde::decode_from_slice(slice, config::standard().with_fixed_int_encoding())?;
        if n != 32 {
            anyhow::bail!("invalid header size")
        }
        Ok(header)
    }
}

impl Default for P2ProtHeader {
    fn default() -> Self {
        Self {
            magic: 0xE9BB,
            content_type: ContentType::Bin,
            version: 0,
            flags: 0,
            header_ext_len: 0,
            size: 0,
            checksum: 0,
            session_id: 0,
        }
    }
}
