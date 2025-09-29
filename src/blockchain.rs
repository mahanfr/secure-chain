use crate::{
    block::Block,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Blockchain {
    difficaulty: u8,
    blocks: Vec<Block>,
}

#[allow(dead_code)]
impl Blockchain {
    pub fn new() -> Self {
        Self {
            difficaulty: 3,
            blocks: vec![Block::genesis()],
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

    pub fn last_block(&self) -> Option<Block> {
        self.blocks.last().cloned()
    }
}

