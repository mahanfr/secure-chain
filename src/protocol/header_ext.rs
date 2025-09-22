use serde::{de::Visitor, ser::SerializeTuple, Deserialize, Serialize};

use crate::protocol::proof_of_work::PoWExt;

#[derive(Debug)]
pub enum HeaderExt {
    PoW(PoWExt),
}

impl Serialize for HeaderExt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer {
        match self {
            Self::PoW(ext) => {
                let mut tuple = serializer.serialize_tuple(2)?;
                tuple.serialize_element(&0u8)?;
                tuple.serialize_element(ext)?;
                tuple.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for HeaderExt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de> {
        use serde::de::Error;

        struct HeaderExtVisitor;

        impl<'de> Visitor<'de> for HeaderExtVisitor {
            type Value = HeaderExt;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    formatter,
                    "A tuple of size 2 consisting of u8 discriminant and a value"
                )
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::SeqAccess<'de>, {
                let discriminant: u8 = seq.next_element()?
                    .ok_or_else(|| A::Error::invalid_length(0, &self))?;
                match discriminant {
                    0x0 => {
                        let x: PoWExt = seq.next_element()?
                            .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                        Ok(HeaderExt::PoW(x))
                    }
                    d => Err(A::Error::invalid_value(
                            serde::de::Unexpected::Unsigned(d as u64),
                            &"0x0")),
                }
            }
        }
        deserializer.deserialize_tuple(2, HeaderExtVisitor)
    }
}
