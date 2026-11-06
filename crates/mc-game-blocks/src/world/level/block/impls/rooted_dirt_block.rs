//! `net.minecraft.world.level.block.RootedDirtBlock`: bonemeal grows hanging roots below.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct RootedDirtBlock;

impl BlockBehaviour for RootedDirtBlock {
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
        level.get_block_state(pos.below()).is_air()
            && !level.is_outside_build_height(pos.below().y())
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _source: BonemealSource,
    ) {
        level.set_block_and_update(pos.below(), Block::HANGING_ROOTS.default_state());
    }
    fn get_bonemeal_particle_pos(&self, pos: BlockPos) -> BlockPos {
        pos.below()
    }
}

register_block_class!("RootedDirtBlock", |_| Box::new(RootedDirtBlock));
