//! `net.minecraft.world.level.block.HayBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

pub struct HayBlock;

impl BlockBehaviour for HayBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        super::rotated_pillar_block::rotate_pillar(state, rotation)
    }

    fn fall_on(&self, level: &mut ServerLevel, _state: BlockState, _pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        let source = level.damage_sources().fall();
        entity.cause_fall_damage(level, fall_distance, 0.2, &source);
    }
}

register_block_class!("HayBlock", |_| Box::new(HayBlock));
