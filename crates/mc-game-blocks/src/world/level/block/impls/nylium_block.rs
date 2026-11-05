//! `net.minecraft.world.level.block.NyliumBlock`: turns into netherrack under cover; bone meal spreads nether vegetation.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::{BonemealSource, BonemealType};

pub struct NyliumBlock;

/// `NyliumBlock.canBeNylium`.
fn can_be_nylium(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let above_state = level.get_block_state(pos.above());
    mc_light::get_light_dampening_into(state, above_state, Direction::Up, i32::from(above_state.get_light_block())) < 15
}

impl BlockBehaviour for NyliumBlock {
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !can_be_nylium(state, &*level, pos) {
            level.set_block_and_update(pos, Block::NETHERRACK.default_state());
        }
    }

    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.above()).is_air() && !level.is_outside_build_height(pos.above().y())
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        let above_pos = pos.above();
        if !level.is_outside_build_height(above_pos.y()) {
            level.place_feature("minecraft:nylium_bonemeal", above_pos);
        }
    }
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::NeighborSpreader
    }
}

register_block_class!("NyliumBlock", |_| Box::new(NyliumBlock));
