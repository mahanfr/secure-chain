use crate::{errors::PeerParseError, keys::PublicKey, log_error};
use serde_json::Value;
use std::{fs::File, net::SocketAddr, str::FromStr};

#[derive(Debug, Clone)]
pub struct Peer {
    pub addr: SocketAddr,
    pub pk: PublicKey,
}
impl Peer {
    pub fn new(addr: SocketAddr, pk: PublicKey) -> Self {
        Self { addr, pk }
    }

    pub fn _to_string(self) -> String {
        format!("peer://{}@{}", self.pk, self.addr)
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
        Ok(Peer { addr, pk })
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
