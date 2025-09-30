use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    networking::PeerMessage,
    protocol::{
        header::{ContentType, P2ProtHeader},
        header_ext::HeaderExt,
    },
};

pub mod header;
pub mod header_ext;
pub mod proof_of_work;

#[derive(Debug, Serialize, Deserialize)]
pub struct P2Protocol {
    pub header: P2ProtHeader,
    pub header_exts: Vec<HeaderExt>,
    pub payload: Vec<u8>,
}

impl P2Protocol {
    pub fn new(message: &PeerMessage) -> Self {
        let payload = bincode::serde::encode_to_vec(message, bincode::config::standard())
            .expect("Internal Error: Parsing message");
        let mut header = P2ProtHeader::default();
        header.content_type = ContentType::from_message(message);
        header.size = payload.len();
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let hash = hasher.finalize();
        let checksum = u32::from_le_bytes(
            hash[0..4]
                .try_into()
                .expect("Internal Error: Parsing message"),
        );
        header.checksum = checksum;
        Self {
            header,
            header_exts: vec![],
            payload,
        }
    }

    pub fn new_with_peer(message: &PeerMessage, peer_id: u64) -> Self {
        let mut prot = P2Protocol::new(message);
        prot.header.session_id = peer_id;
        prot
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buffer = bincode::serde::encode_to_vec(
            &self.header,
            bincode::config::standard().with_fixed_int_encoding(),
        )?;
        // for ext in self.header_exts.iter() {
        //     bincode::serde::encode_into_slice(ext,&mut buffer,bincode::config::standard())?;
        // }
        buffer.append(&mut self.payload.clone());
        Ok(buffer)
    }

    #[allow(dead_code)]
    pub fn add_extention(&mut self, ext: HeaderExt) {
        // Inefficient but eh.. it is a small structure
        let config = bincode::config::standard();
        let val = bincode::serde::encode_to_vec(&ext, config);
        self.header.header_ext_len += val.iter().len() as u32;
        self.header_exts.push(ext)
    }
}
