//! `HoglinBase`: the shared attack of hoglins and zoglins (`hurtAndThrowTarget`, `throwTarget`).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment_helper;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::Attribute;

/// `ATTACK_ANIMATION_DURATION`.
pub const ATTACK_ANIMATION_DURATION: i32 = 10;
/// `PROBABILITY_OF_SPAWNING_AS_BABY`.
pub const PROBABILITY_OF_SPAWNING_AS_BABY: f32 = 0.2;

/// `HoglinBase.hurtAndThrowTarget(level, body, target)` (`target` is out of the level while the attack runs).
pub fn hurt_and_throw_target(level: &mut ServerLevel, body: &mut dyn Entity, target: &mut dyn Entity) -> bool {
    let attack_damage = body.living().expect("living").get_attribute_value(Attribute::ATTACK_DAMAGE) as f32;
    let actual_damage = if !body.is_baby() && (attack_damage as i32) > 0 {
        attack_damage / 2.0 + level.tick_state.random.next_int_bounded(attack_damage as i32) as f32
    } else {
        attack_damage
    };
    let damage_source = level.damage_sources().mob_attack(&*body);
    let was_hurt = target.hurt_server(level, &damage_source, actual_damage);
    if was_hurt {
        let target_id = target.id();
        enchantment_helper::do_post_attack_effects(level, &mut Actors::of(&mut *body).and(&mut *target), target_id, &damage_source);
        if !body.is_baby() {
            throw_target(level, &*body, target);
        }
    }
    was_hurt
}

/// `HoglinBase.throwTarget(body, target)`.
pub fn throw_target(level: &mut ServerLevel, body: &dyn Entity, target: &mut dyn Entity) {
    let knockback_power = body.living().expect("living").get_attribute_value(Attribute::ATTACK_KNOCKBACK);
    let knockback_resistance = target.living().expect("living").get_attribute_value(Attribute::KNOCKBACK_RESISTANCE);
    let effective_knockback_power = knockback_power - knockback_resistance;
    if !(effective_knockback_power <= 0.0) {
        let xd = target.base().x() - body.base().x();
        let zd = target.base().z() - body.base().z();
        let random = &mut level.tick_state.random;
        let horizontal_push_angle = (random.next_int_bounded(21) - 10) as f32;
        let horizontal_scale = effective_knockback_power * f64::from(random.next_float() * 0.5 + 0.2);
        let horizontal_push_vector = Vec3::new(xd, 0.0, zd).normalize().scale(horizontal_scale).y_rot(horizontal_push_angle);
        let vertical_scale = effective_knockback_power * f64::from(random.next_float()) * 0.5;
        target.push_xyz(horizontal_push_vector.x, vertical_scale, horizontal_push_vector.z);
        target.base_mut().sync_velocity = true;
    }
}
