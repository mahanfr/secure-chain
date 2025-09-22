use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{networking::PeerMessage, protocol::header::P2ProtHeader};

pub mod header;
pub mod proof_of_work;
pub mod header_ext;

#[derive(Debug, Serialize, Deserialize)]
pub struct P2Protocol {
    pub header: P2ProtHeader,
    pub payload: Vec<u8>,
}

impl P2Protocol {
    pub async fn new(message: &PeerMessage) -> Result<Self> {
        let s = serde_json::to_string(message)?;
        let payload = format!("{s}\r\n").as_bytes().to_vec();
        let mut header = P2ProtHeader::default();
        header.size = payload.len();
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let hash = hasher.finalize();
        let checksum = u32::from_le_bytes(hash[0..4].try_into()?);
        header.checksum = checksum;
        Ok(Self {header, payload})
    }

    pub fn set_pow_flag(&mut self, flag: bool) {
        if flag {
            self.header.flags |= 0x2;
        } else {
            self.header.flags &= !0x2;
        }
    }
    pub fn get_pow_flag(&self) -> bool {
        (self.header.flags & 0x2) != 0
    }
} 
