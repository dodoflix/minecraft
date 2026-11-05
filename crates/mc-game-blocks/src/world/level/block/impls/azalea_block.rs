//! `net.minecraft.world.level.block.AzaleaBlock` (extends `VegetationBlock`): bonemeal grows an azalea tree.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::grower::TreeGrower;

pub struct AzaleaBlock;

impl MayPlaceOn for AzaleaBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_azalea")
    }
}

impl BlockBehaviour for AzaleaBlock {
    vegetation_behaviour_methods!();
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        false
    }
    fn is_valid_bonemeal_target_server(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        let min_height = TreeGrower::AZALEA.get_minimum_height(level).unwrap_or(0);
        !level.is_outside_build_height(pos.above_n(min_height + 2).y()) && level.get_fluid_state(pos.above()).is_empty()
    }
    fn is_bonemeal_success(&self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.random().next_float() < 0.45
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        TreeGrower::AZALEA.grow_tree(level, pos, state);
    }
}

register_block_class!("AzaleaBlock", |_| Box::new(AzaleaBlock));
