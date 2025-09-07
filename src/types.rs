pub type Bytes = Vec<u8>;

pub trait ByteSerialize {
    fn to_bytes(&self) -> Bytes;
    fn from_bytes(bytes: Bytes) -> Self;
}
