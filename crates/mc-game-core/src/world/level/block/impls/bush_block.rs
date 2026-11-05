//! `net.minecraft.world.level.block.BushBlock` (extends `VegetationBlock`; `bush`, `red_shrub`): bonemeal spreads it.
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, spreading_bonemeal_methods, vegetation_behaviour_methods};

pub struct BushBlock;
impl MayPlaceOn for BushBlock {}

impl BlockBehaviour for BushBlock {
    vegetation_behaviour_methods!();
    spreading_bonemeal_methods!();
}

register_block_class!("BushBlock", |_| Box::new(BushBlock));
