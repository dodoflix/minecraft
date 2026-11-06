//! `net.minecraft.world.level.block.TallDryGrassBlock` (extends `DryVegetationBlock`): bonemeal spreads short dry grass.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::{
    BonemealSource, find_spreadable_neighbour_pos, has_spreadable_neighbour_pos,
};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, vegetation_behaviour_methods,
};

pub struct TallDryGrassBlock;

impl MayPlaceOn for TallDryGrassBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_dry_vegetation")
    }
}

impl BlockBehaviour for TallDryGrassBlock {
    vegetation_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        level: &dyn LevelReader,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        has_spreadable_neighbour_pos(level, pos, Block::SHORT_DRY_GRASS.default_state())
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) {
        let short = Block::SHORT_DRY_GRASS.default_state();
        if let Some(target) = find_spreadable_neighbour_pos(level, pos, short) {
            level.set_block_and_update(target, short);
        }
    }
}

register_block_class!("TallDryGrassBlock", |_| Box::new(TallDryGrassBlock));
