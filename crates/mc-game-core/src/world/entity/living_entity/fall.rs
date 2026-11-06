//! The `LivingEntity.checkFallDamage` override around `Entity.checkFallDamage`.

use super::util::{self, l, lm};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::enchantment::actors::Actors;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::Attribute;
use mc_data::block::BlockState;
use mc_protocol::packets::play::ParticleOptions;

/// `calculateFallPower(fallDistance)`.
fn calculate_fall_power(entity: &dyn Entity, fall_distance: f64) -> f64 {
    fall_distance + 1.0E-6 - l(entity).get_attribute_value(Attribute::SAFE_FALL_DISTANCE)
}

/// The part of `LivingEntity.checkFallDamage` that runs before `super.checkFallDamage`: fluid refresh when not in water and
/// the landing particles.
pub fn before_check_fall_damage(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    on_ground: bool,
    on_state: BlockState,
    pos: BlockPos,
) {
    if !entity.is_in_water() {
        entity.update_fluid_interaction(level);
    }
    if on_ground && entity.base().fall_distance > 0.0 {
        {
            let id = entity.id();
            crate::world::item::enchantment_helper::run_location_changed_effects(
                level,
                &mut Actors::of(entity),
                id,
            );
        }
        let power = f64::from(0.max(mth::floor_f64(calculate_fall_power(
            entity,
            entity.base().fall_distance,
        ))));
        if power > 0.0 && !on_state.is_air() {
            let p = entity.base().position();
            let (mut x, y, mut z) = (p.x, p.y, p.z);
            let entity_pos = entity.base().block_position();
            if pos.x() != entity_pos.x() || pos.z() != entity_pos.z() {
                let x_diff = x - f64::from(pos.x()) - 0.5;
                let z_diff = z - f64::from(pos.z()) - 0.5;
                let max_diff = x_diff.abs().max(z_diff.abs());
                x = f64::from(pos.x()) + 0.5 + x_diff / max_diff * 0.5;
                z = f64::from(pos.z()) + 0.5 + z_diff / max_diff * 0.5;
            }
            let scale = (f64::from(0.2f32) + power / 15.0).min(2.5);
            let particles = (150.0 * scale) as i32;
            level.send_particles(
                ParticleOptions::Block {
                    particle_type: mc_data::ParticleType::BLOCK,
                    state: on_state,
                },
                x,
                y,
                z,
                particles,
                0.0,
                0.0,
                0.0,
                f64::from(0.15f32),
            );
        }
    }
}

/// The part after `super.checkFallDamage`.
pub fn after_check_fall_damage(entity: &mut dyn Entity, on_ground: bool) {
    if on_ground {
        lm(entity).last_climbable_pos = None;
    }
}
