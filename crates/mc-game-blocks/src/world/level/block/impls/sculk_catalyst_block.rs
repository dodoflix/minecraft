//! `net.minecraft.world.level.block.SculkCatalystBlock`.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct SculkCatalystBlock;

impl BlockBehaviour for SculkCatalystBlock {
    fn spawn_after_break(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        tool: &mc_game_core::world::item::ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            helpers::try_drop_experience(
                level,
                pos,
                tool,
                &mc_game_core::registry::typed::int_provider::IntProvider::Constant(5),
            );
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::BLOOM) {
            level.set_block_and_update(pos, state.set(props::BLOOM, false));
        }
    }
}

register_block_class!("SculkCatalystBlock", |_| Box::new(SculkCatalystBlock));
