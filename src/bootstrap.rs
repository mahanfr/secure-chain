use core::fmt;
use std::fs::File;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Node {
    public_key: String,
    adress: String,
    port: u16,
}
impl Node {
    pub fn from_config_uri(uri: String) -> Result<Self, ParseError> {
        if !uri.starts_with("peer://") {
            return Err(ParseError::MissingScheme);
        }

        let parsable = uri.strip_prefix("peer://").ok_or(ParseError::MissingScheme)?;

        let at_pos = parsable.find('@').ok_or(ParseError::MissingAtSign)?;
        let (pk, mut addr_port) = parsable.split_at(at_pos);
        addr_port = &addr_port[1..];
        if pk.is_empty() {
            return Err(ParseError::EmptyPublicKey);
        }
        if !pk.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ParseError::InvalidPublicKeyHex);
        }
        if addr_port.is_empty() {
            return Err(ParseError::MissingHostPort);
        }
        let (host_str, port_str) = if addr_port.starts_with('[') {
            // must be [ipv6]:port
            let close = addr_port.find(']').ok_or(ParseError::UnclosedIpv6Bracket)?;
            let host = &addr_port[1..close];
            let remainder = &addr_port[close + 1..];
            let port = remainder.strip_prefix(':').ok_or(ParseError::MissingPort)?;
            (host, port)
        } else {
            // hostname or IPv4: split at the last ':'
            let mut it = addr_port.rsplitn(2, ':');
            let port = it.next().ok_or(ParseError::MissingPort)?;
            let host = it.next().ok_or(ParseError::MissingHostPort)?;
            (host, port)
        };
        if host_str.is_empty() {
            return Err(ParseError::EmptyHost);
        }

        let port: u16 = port_str
            .parse::<u16>()
            .map_err(|_| ParseError::InvalidPort)?;

        Ok(Self{
            public_key: pk.to_string(),
            adress: host_str.to_string(),
            port,
        })
    }

    pub fn addr(&self) -> String {
        format!("{}:{}",self.adress, self.port)
    }
}

pub fn read_nodes_list(file_path: impl ToString) -> Vec<Node> {
    let file = File::open(file_path.to_string())
        .expect("Can not find network starting nodes configuration file");
    let json: Value = serde_json::from_reader(file)
        .expect("Invalid configuration file");
    let mut nodes = Vec::new();
    if let Some(array) = json.as_array() {
        for item in array {
            if let Some(s) = item.as_str() {
                match Node::from_config_uri(s.to_string()) {
                    Ok(node) => nodes.push(node),
                    Err(e) => eprintln!("{}",e.to_string())
                }
            }
        }
    }
    nodes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MissingScheme,
    MissingAtSign,
    EmptyPublicKey,
    InvalidPublicKeyHex,
    MissingHostPort,
    MissingPort,
    InvalidPort,
    EmptyHost,
    UnclosedIpv6Bracket,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ParseError::*;
        let msg = match self {
            MissingScheme => "missing 'peer://' scheme",
            MissingAtSign => "missing '@' between public key and address",
            EmptyPublicKey => "public key is empty",
            InvalidPublicKeyHex => "public key must be hex (0-9a-fA-F)",
            MissingHostPort => "missing host:port after '@'",
            MissingPort => "missing port",
            InvalidPort => "invalid port (must be 0-65535)",
            EmptyHost => "host is empty",
            UnclosedIpv6Bracket => "IPv6 host has no closing ']'",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ParseError {}

