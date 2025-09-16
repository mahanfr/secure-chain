use std::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::PeerParseError;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct PublicKey(String);

impl PublicKey {
    pub fn new_dummy() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn from_string(pk: impl ToString) -> Result<Self, PeerParseError> {
        let pk = pk.to_string();
        if pk.is_empty() {
            return Err(PeerParseError::EmptyPublicKey);
        }
        if !pk.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PeerParseError::InvalidPublicKeyHex);
        }
        Ok(Self(pk))
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
