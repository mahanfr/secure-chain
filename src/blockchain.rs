use crate::{block::Block, types::{ByteSerialize, Bytes}};

#[derive(Debug, Clone)]
pub struct Blockchain {
    difficaulty: u8,
    blocks: Vec<Block>
}

impl Blockchain {
    pub fn new() -> Self {
        Self {
            difficaulty: 3,
            blocks: vec![Block::genesis()]
        }
    }

    pub fn mine_and_add_block(&mut self, block: &Block) {
        let mut blk = block.clone();
        blk.mine_block(self.difficaulty);
        self.blocks.push(blk);
    }

    pub fn last_hash(&self) -> String {
        self.blocks.last().unwrap().hash.clone()
    }
}

impl ByteSerialize for Blockchain {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }
    fn from_bytes(bytes: Bytes) -> Self {
       todo!() 
    }
}

