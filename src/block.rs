use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::Bytes;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Block {
    pub id: String,
    pub data: Bytes,
    pub timestamp: u128,
    pub hash: String,
    pub prev_hash: String,
    pub nonce: u64,
}

impl Block {
    pub fn new(data: Bytes, prev_hash: String) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        Self {
            id: Uuid::new_v4().into(),
            data,
            timestamp,
            hash: String::new(),
            prev_hash,
            nonce: 0,
        }
    }

    pub fn genesis() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut hasher = Sha256::new();
        hasher.update("_GENESIS_");
        Self {
            id: Uuid::new_v4().into(),
            data: "_GENESIS_".as_bytes().to_vec(),
            timestamp,
            hash: format!("{:x}", hasher.finalize()),
            prev_hash: String::new(),
            nonce: 0,
        }
    }

    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(&self.data);
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.prev_hash.as_bytes());
        hasher.update(self.nonce.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn mine_block(&mut self, difficulty: u8) {
        let target = "0".repeat(difficulty as usize);
        let timestamp1 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut hash = String::new();
        while !hash.starts_with(&target) {
            self.nonce += 1;
            hash = self.calculate_hash();
        }
        self.hash = hash;
        let timestamp2 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let time = timestamp2 - timestamp1;
        println!("Block mined at {timestamp2} in {time}ms: {}", self.hash);
    }
}
