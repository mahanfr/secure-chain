use core::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerParseError {
    MissingScheme,
    MissingAtSign,
    EmptyPublicKey,
    InvalidPublicKeyHex,
    InvalidAddr,
}

impl fmt::Display for PeerParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PeerParseError::*;
        let msg = match self {
            MissingScheme => "missing 'peer://' scheme",
            MissingAtSign => "missing '@' between public key and address",
            EmptyPublicKey => "public key is empty",
            InvalidPublicKeyHex => "public key must be hex (0-9a-fA-F)",
            InvalidAddr => "invalid address",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for PeerParseError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkError {
    HandshakeRequired,
    Block404,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use NetworkError::*;
        let msg = match self {
            HandshakeRequired => "Handshake required to stablish a connection",
            Block404 => "Block not found in our chian",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for NetworkError {}
