use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::header::P2ProtHeader;

#[derive(Debug)]
#[repr(u8)]
pub enum PoWAlgo {
    None = 0,
    Sha256LeadingZero = 1,
}
impl PoWAlgo {
    pub fn from_u8(byte: u8) -> Self {
        match byte {
            1 => PoWAlgo::Sha256LeadingZero,
            _ => PoWAlgo::None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoWExt {
    algo: u8,
    difficulty: u8,
    timestamp: u128,
    nonce: u64,
}

impl PoWExt {
    pub async fn new(header: &P2ProtHeader, difficulty: u8) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut nonce : u64 = 0;
        loop {
            nonce += 1;
            let hash = Self::gen_hash(header, &timestamp, &nonce);
            if hash.starts_with(&b"0".repeat(difficulty as usize)) {
                break;
            }
        }
        Self {
            algo: PoWAlgo::Sha256LeadingZero as u8,
            difficulty,
            timestamp,
            nonce,
        }
    }

    fn gen_hash(header: &P2ProtHeader, timestamp: &u128, nonce: &u64) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(header.size.to_le_bytes());
        hasher.update(header.checksum.to_le_bytes());
        hasher.update(timestamp.to_le_bytes());
        hasher.update(nonce.to_le_bytes());
        hasher.finalize().to_vec()
    }

    pub fn update_peer_difficulty(&self, self_diff: &mut u8) {
        *self_diff = self.difficulty;
    }

    pub fn validate(&self, packet_header: &P2ProtHeader, self_diff: u8) -> bool {
        match self.algo {
            0 => {
                if self_diff < self.difficulty {
                    true
                } else {
                    if Self::gen_hash(packet_header, &self.timestamp, &self.nonce).starts_with(&b"0".repeat(self.difficulty as usize)) {
                        true
                    }else {
                        false
                    }
                }
            },
            _ => false,
        }
    }
}
