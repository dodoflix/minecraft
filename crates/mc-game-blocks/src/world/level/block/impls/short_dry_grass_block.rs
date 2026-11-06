//! `net.minecraft.world.level.block.ShortDryGrassBlock` (extends `DryVegetationBlock`): bonemeal grows it to tall dry grass.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, vegetation_behaviour_methods,
};

pub struct ShortDryGrassBlock;

impl MayPlaceOn for ShortDryGrassBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_dry_vegetation")
    }
}

impl BlockBehaviour for ShortDryGrassBlock {
    vegetation_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        true
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) {
        level.set_block_and_update(pos, Block::TALL_DRY_GRASS.default_state());
    }
}

register_block_class!("ShortDryGrassBlock", |_| Box::new(ShortDryGrassBlock));
