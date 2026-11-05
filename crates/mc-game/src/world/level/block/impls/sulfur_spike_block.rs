//! `net.minecraft.world.level.block.SulfurSpikeBlock` (extends `SpeleothemBlock`).
use super::prelude::*;
use super::speleothem_block::{Speleothem, SpeleothemClass};

const MAX_GROWING_LENGTH: i32 = 2;

pub struct SulfurSpikeBlock;

impl SpeleothemClass for SulfurSpikeBlock {
    fn block_to_grow_on(&self) -> Block {
        Block::SULFUR
    }
    fn stalactite_landing_sound(&self) -> i32 {
        1052
    }
    fn max_growth_length(&self) -> i32 {
        MAX_GROWING_LENGTH
    }
}

register_block_class!("SulfurSpikeBlock", |block| Box::new(Speleothem { block, class: SulfurSpikeBlock }));
