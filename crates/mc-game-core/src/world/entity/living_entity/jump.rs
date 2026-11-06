//! `LivingEntity.jumpFromGround` and the liquid jump helpers.

use super::util::{effect_amplifier, is_sprinting, l};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, movement};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::{Attribute, MobEffect};

/// `getJumpBoostPower()`.
pub fn get_jump_boost_power(entity: &dyn Entity) -> f32 {
    match effect_amplifier(entity, MobEffect::JUMP_BOOST) {
        Some(amplifier) => 0.1 * (amplifier as f32 + 1.0),
        None => 0.0,
    }
}

/// `getJumpPower(multiplier)`.
pub fn get_jump_power_with(entity: &dyn Entity, level: &ServerLevel, multiplier: f32) -> f32 {
    l(entity).get_attribute_value(Attribute::JUMP_STRENGTH) as f32
        * multiplier
        * movement::get_block_jump_factor(entity, level)
        + get_jump_boost_power(entity)
}

/// `getJumpPower()`.
pub fn get_jump_power(entity: &dyn Entity, level: &ServerLevel) -> f32 {
    get_jump_power_with(entity, level, 1.0)
}

/// `jumpFromGround()`.
pub fn jump_from_ground(entity: &mut dyn Entity, level: &ServerLevel) {
    let jump_power = entity.get_jump_power(level);
    if !(jump_power <= 1.0E-5) {
        let movement = entity.base().delta_movement();
        entity.base_mut().set_delta_movement_xyz(
            movement.x,
            f64::from(jump_power).max(movement.y),
            movement.z,
        );
        if is_sprinting(entity) {
            let angle = entity.base().y_rot() * (std::f64::consts::PI / 180.0) as f32;
            entity.base_mut().add_delta_movement(Vec3::new(
                -f64::from(mth::sin(f64::from(angle))) * 0.2,
                0.0,
                f64::from(mth::cos(f64::from(angle))) * 0.2,
            ));
        }
        entity.base_mut().needs_sync = true;
    }
}

/// `goDownInWater()`.
pub fn go_down_in_water(entity: &mut dyn Entity) {
    let dm = entity
        .base()
        .delta_movement()
        .add_xyz(0.0, -f64::from(0.04f32), 0.0);
    entity.base_mut().set_delta_movement(dm);
}

/// `jumpInLiquid(fluid)`.
pub fn jump_in_liquid(entity: &mut dyn Entity) {
    let dm = entity
        .base()
        .delta_movement()
        .add_xyz(0.0, f64::from(0.04f32), 0.0);
    entity.base_mut().set_delta_movement(dm);
}
