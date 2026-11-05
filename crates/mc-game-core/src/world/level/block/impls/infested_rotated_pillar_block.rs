//! `net.minecraft.world.level.block.InfestedRotatedPillarBlock`.
use super::prelude::*;
use super::rotated_pillar_block::{self, rotate_pillar};
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

pub struct InfestedRotatedPillarBlock;

impl BlockBehaviour for InfestedRotatedPillarBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &crate::world::item::ItemStack, _drop_experience: bool) {
        super::infested_block::spawn_after_break(level, pos, tool);
    }
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(rotated_pillar_block::get_state_for_placement(block, context))
    }
}

register_block_class!("InfestedRotatedPillarBlock", |_| Box::new(InfestedRotatedPillarBlock));
