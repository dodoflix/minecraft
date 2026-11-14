//! `ChargeAttack`: the dash of a nautilus at its attack target (the damage, the knockback and the cooldown that ends it).

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::{Attribute, MobEffect, SoundEvent};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, CHARGE_COOLDOWN_TICKS,
};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::{combat, ext, tamable};
use mc_game_core::world::item::enchantment::actors::Actors;
use mc_game_core::world::item::enchantment_helper;

pub struct ChargeAttack {
    time_between_attacks: i32,
    charge_targeting: TargetingConditions,
    speed: f32,
    knockback_force: f32,
    max_target_detection_distance: f64,
    max_charge_distance: f64,
    charge_sound: SoundEvent,
    charge_velocity_vector: Vec3,
    start_position: Vec3,
}

impl ChargeAttack {
    /// `new ChargeAttack(timeBetweenAttacks, chargeTargeting, speed, knockbackForce, maxChargeDistance,
    /// maxTargetDetectionDistance, chargeSound)`.
    pub fn new(
        time_between_attacks: i32,
        charge_targeting: TargetingConditions,
        speed: f32,
        knockback_force: f32,
        max_charge_distance: f64,
        max_target_detection_distance: f64,
        charge_sound: SoundEvent,
    ) -> BehaviorBase<ChargeAttack> {
        BehaviorBase::new(
            entry_condition![CHARGE_COOLDOWN_TICKS => ValueAbsent, ATTACK_TARGET => ValuePresent],
            ChargeAttack {
                time_between_attacks,
                charge_targeting,
                speed,
                knockback_force,
                max_target_detection_distance,
                max_charge_distance,
                charge_sound,
                charge_velocity_vector: Vec3::ZERO,
                start_position: Vec3::ZERO,
            },
        )
    }

    fn attack_target(body: &dyn Entity) -> Option<EntityId> {
        body.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
    }

    /// `canStillUse(level, body, timestamp)`.
    fn can_charge(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        let Some(target) =
            Self::attack_target(body).and_then(|t| level.get_entity_or_recently_removed(t))
        else {
            return false;
        };
        let too_far_from_start = body.position().subtract(self.start_position).length_sqr()
            >= self.max_charge_distance * self.max_charge_distance;
        let target_out_of_range = target.position().subtract(body.position()).length_sqr()
            >= self.max_target_detection_distance * self.max_target_detection_distance;
        if (body.mob().is_some_and(|m| m.tamable.is_some()) && tamable::is_tame(body))
            || too_far_from_start
            || target_out_of_range
        {
            false
        } else {
            body.has_line_of_sight(level, target)
                && !body.brain().has_memory_value(CHARGE_COOLDOWN_TICKS)
        }
    }

    fn amplifier_level(body: &dyn Entity, effect: MobEffect) -> i32 {
        body.get_effect(effect).map_or(0, |e| e.amplifier + 1)
    }
}

impl Behavior for ChargeAttack {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.brain().has_memory_value(ATTACK_TARGET)
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.can_charge(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.start_position = body.position();
        let Some(target) = Self::attack_target(body).and_then(|t| level.get_entity(t)) else {
            return;
        };
        let direction = target.position().subtract(body.position()).normalize();
        self.charge_velocity_vector = direction.scale(f64::from(self.speed));
        if self.can_charge(level, body) {
            body.play_sound_default(level, self.charge_sound);
        }
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(attack_target) = Self::attack_target(body) else {
            return;
        };
        if let Some(target) = level.get_entity(attack_target) {
            combat::look_at(body, target, 360.0, 360.0);
        }
        body.base_mut()
            .set_delta_movement(self.charge_velocity_vector);
        let bb = body.get_bounding_box();
        let colliding =
            ext::get_nearby_entities(level, |_| true, &self.charge_targeting, &*body, &bb);
        if let Some(&closest) = colliding.first() {
            if body.base().passengers.contains(&closest) {
                return;
            }
            let source = level.damage_sources().mob_attack(&*body);
            let damage = body
                .living()
                .expect("living")
                .get_attribute_value(Attribute::ATTACK_DAMAGE) as f32;
            let movement_speed_level = Self::amplifier_level(&*body, MobEffect::SPEED);
            let movement_slowdown_level = Self::amplifier_level(&*body, MobEffect::SLOWNESS);
            let speed_boost_power = 0.25 * (movement_speed_level - movement_slowdown_level) as f32;
            let movement_speed =
                body.living()
                    .expect("living")
                    .get_attribute_value(Attribute::MOVEMENT_SPEED) as f32;
            let speed_factor =
                mth::clamp_f32(self.speed * movement_speed, 0.2, 2.0) + speed_boost_power;
            let knockback = speed_factor * self.knockback_force;
            level.with_entity(closest, |level, target| {
                if level.lend(&mut *body, |level| {
                    target.hurt_server(level, &source, damage)
                }) {
                    enchantment_helper::do_post_attack_effects(
                        level,
                        &mut Actors::of(body).and(target),
                        closest,
                        &source,
                    );
                }
                let movement = body.base().delta_movement();
                body.cause_extra_knockback(target, knockback, movement, &source, damage, false);
            });
            self.stop(level, body, timestamp);
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut()
            .set_memory(&CHARGE_COOLDOWN_TICKS, self.time_between_attacks);
        body.brain_mut().erase_memory(&ATTACK_TARGET);
    }
}
