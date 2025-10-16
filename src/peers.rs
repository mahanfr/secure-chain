use crate::{errors::PeerParseError, keys::PublicKey, log_error, utils::Bitmask};
use anyhow::Result;
use bincode::{Decode, Encode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs::File, net::SocketAddr, str::FromStr};

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PeerOptions(pub u8);

impl Bitmask for PeerOptions {
    type Repr = u8;
    type IndexKey = &'static str;
    fn set(&mut self, key: Self::IndexKey, value:bool) {
        if let Some(bit) = Self::index(key) {
            if value {
                *self.inner_mut() |= 1 << bit;
            } else {
                *self.inner_mut() &= !(1 << bit);
            }
        }
    }

    fn get(&self, key: Self::IndexKey) -> bool {
        if let Some(bit) = Self::index(key) {
            (self.inner() & (1<< bit)) != 0
        } else {
            false
        }
    }

    fn index(key: &str) -> Option<u8> {
        match key {
            "SERVING" => Some(0),
            _ => None,
        }
    }

    fn inner(&self) -> Self::Repr {
        self.0
    }

    fn inner_mut(&mut self) -> &mut Self::Repr {
        &mut self.0
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Peer {
    pub addr: SocketAddr,
    pub pk: PublicKey,
    pub options: PeerOptions,
}
impl Peer {
    pub fn new(addr: SocketAddr, pk: PublicKey) -> Self {
        Self { addr, pk, options: PeerOptions(0)}
    }

    pub fn _to_string(&self) -> String {
        format!("peer://{}@{}", self.pk, self.addr)
    }

    pub fn derive_unique_id(&self) -> Result<u128> {
        let hash = Sha256::digest(self.pk.as_bytes());

        let bytes: [u8; 16] = hash[..16].try_into()?;
        Ok(u128::from_le_bytes(bytes))
    }

    pub fn from_string(uri: String) -> Result<Self, PeerParseError> {
        if !uri.starts_with("peer://") {
            return Err(PeerParseError::MissingScheme);
        }

        let parsable = uri
            .strip_prefix("peer://")
            .ok_or(PeerParseError::MissingScheme)?;

        let at_pos = parsable.find('@').ok_or(PeerParseError::MissingAtSign)?;
        let (pk, addr_port) = parsable.split_at(at_pos);
        let pk = PublicKey::from_string(pk)?;
        let Ok(addr) = SocketAddr::from_str(&addr_port[1..]) else {
            return Err(PeerParseError::InvalidAddr);
        };
        Ok(Peer { addr, pk, options: PeerOptions(0) })
    }
}

pub fn bootstap_peers(file_path: impl ToString) -> Vec<Peer> {
    let file = File::open(file_path.to_string())
        .expect("Can not find network starting nodes configuration file");
    let json: Value = serde_json::from_reader(file).expect("Invalid configuration file");
    let mut nodes = Vec::new();
    if let Some(array) = json.as_array() {
        for item in array {
            if let Some(s) = item.as_str() {
                match Peer::from_string(s.to_string()) {
                    Ok(node) => nodes.push(node),
                    Err(e) => log_error!("{}", e.to_string()),
                }
            }
        }
    }
    nodes
}
