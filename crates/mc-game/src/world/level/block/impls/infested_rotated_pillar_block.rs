//! `net.minecraft.world.level.block.InfestedRotatedPillarBlock`.
use super::prelude::*;
use super::rotated_pillar_block::rotate_pillar;

pub struct InfestedRotatedPillarBlock;

impl BlockBehaviour for InfestedRotatedPillarBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
}

register_block_class!("InfestedRotatedPillarBlock", |_| Box::new(InfestedRotatedPillarBlock));
