//! `net.minecraft.world.level.block.FireflyBushBlock` (extends `VegetationBlock`): bonemeal spreads it.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, spreading_bonemeal_methods, vegetation_behaviour_methods,
};

pub struct FireflyBushBlock;
impl MayPlaceOn for FireflyBushBlock {}

impl BlockBehaviour for FireflyBushBlock {
    vegetation_behaviour_methods!();
    spreading_bonemeal_methods!();
}

register_block_class!("FireflyBushBlock", |_| Box::new(FireflyBushBlock));
