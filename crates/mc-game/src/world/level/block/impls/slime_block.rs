//! `net.minecraft.world.level.block.SlimeBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

pub struct SlimeBlock;

impl BlockBehaviour for SlimeBlock {
    fn fall_on(&self, level: &mut ServerLevel, _state: BlockState, _pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        if !entity.is_suppressing_bounce() {
            let source = level.damage_sources().fall();
            entity.cause_fall_damage(level, fall_distance, 0.0, &source);
        }
    }

    fn step_on(&self, _level: &mut ServerLevel, _pos: BlockPos, _on_state: BlockState, entity: &mut dyn Entity) {
        let abs_delta_y = entity.base().delta_movement().y.abs();
        if abs_delta_y < 0.1 && !entity.is_stepping_carefully() {
            let scale = 0.4 + abs_delta_y * 0.2;
            let m = entity.base().delta_movement();
            entity.base_mut().set_delta_movement(m.multiply(Vec3::new(scale, 1.0, scale)));
        }
    }
}

register_block_class!("SlimeBlock", |_| Box::new(SlimeBlock));
