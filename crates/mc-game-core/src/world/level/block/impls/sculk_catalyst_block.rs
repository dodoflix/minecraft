//! `net.minecraft.world.level.block.SculkCatalystBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;

pub struct SculkCatalystBlock;

impl BlockBehaviour for SculkCatalystBlock {
    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &crate::world::item::ItemStack, drop_experience: bool) {
        if drop_experience {
            helpers::try_drop_experience(level, pos, tool, &crate::registry::typed::int_provider::IntProvider::Constant(5));
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::BLOOM) {
            level.set_block_and_update(pos, state.set(props::BLOOM, false));
        }
    }
}

register_block_class!("SculkCatalystBlock", |_| Box::new(SculkCatalystBlock));
