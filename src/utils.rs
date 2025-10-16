use std::ops::{BitAnd, BitOr, Not, Shl};


pub trait Bitmask {

    type Repr: Copy
        + From<u8>
        + BitOr<Output = Self::Repr>
        + BitAnd<Output = Self::Repr>
        + Not<Output = Self::Repr>
        + Shl<u32, Output = Self::Repr>;
    
    type IndexKey: Copy;

    fn set(&mut self, key: Self::IndexKey, value:bool);
    fn get(&self, key: Self::IndexKey) -> bool;
    fn index(key: Self::IndexKey) -> Option<u8>;
    fn inner(&self) -> Self::Repr;
    fn inner_mut(&mut self) -> &mut Self::Repr;
}
