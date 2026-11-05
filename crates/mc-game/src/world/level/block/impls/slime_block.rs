//! `net.minecraft.world.level.block.SlimeBlock`.
use super::prelude::*;

pub struct SlimeBlock;

impl BlockBehaviour for SlimeBlock {
    fn fall_on(&self, level: &mut crate::server::level::ServerLevel, _state: BlockState, _pos: BlockPos, entity: &mut dyn crate::world::entity::Entity, fall_distance: f64) {
        if !entity.is_suppressing_bounce() {
            let source = level.damage_sources().fall();
            entity.cause_fall_damage(level, fall_distance, 0.0, &source);
        }
    }

    fn step_on(&self, _level: &mut crate::server::level::ServerLevel, _pos: BlockPos, _on_state: BlockState, entity: &mut dyn crate::world::entity::Entity) {
        let abs_delta_y = entity.base().delta_movement().y.abs();
        if abs_delta_y < 0.1 && !entity.is_shift_key_down() {
            let scale = 0.4 + abs_delta_y * 0.2;
            let dm = entity.base().delta_movement().multiply_xyz(scale, 1.0, scale);
            entity.base_mut().set_delta_movement(dm);
        }
    }
}

register_block_class!("SlimeBlock", |_| Box::new(SlimeBlock));
