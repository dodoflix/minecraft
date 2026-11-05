//! Collision pushing between entities: `LivingEntity.pushEntities` (incl. cramming damage), `doPush`, `isPushable`.
//!
//! `EntitySelector.pushableBy`'s team collision rules are not applied (no scoreboard access from the level yet).

use super::seams::{self, LivingDamage};
use super::util::is_position_entity_ticking;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::math::mth;
use mc_core::random::RandomSource;

/// `LivingEntity.isPushable()`.
pub fn living_is_pushable(entity: &dyn Entity, level: &ServerLevel) -> bool {
    if !entity.is_always_ticking() && !is_position_entity_ticking(level, entity.base().block_position()) {
        false
    } else {
        entity.is_alive() && !entity.is_spectator() && !entity.on_climbable(level)
    }
}

/// `Level.getPushableEntities(entity, box)` = `getEntities(entity, box, EntitySelector.pushableBy(entity))`
/// (`NO_SPECTATORS` and `isPushable()`).
fn get_pushable_entities(level: &ServerLevel, entity: &dyn Entity) -> Vec<crate::world::entity::EntityId> {
    level.get_entities(Some(entity.id()), &entity.base().bounding_box(), |other| !other.is_spectator() && other.is_pushable(level))
}

/// `LivingEntity.pushEntities()`.
pub fn push_entities(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let pushable = get_pushable_entities(level, entity);
    if pushable.is_empty() {
        return;
    }
    let max_cramming = level.max_entity_cramming();
    if max_cramming > 0 && pushable.len() as i32 > max_cramming - 1 && entity.base_mut().random.next_int_bounded(4) == 0 {
        let count = pushable.iter().filter(|&&id| level.get_entity(id).is_some_and(|e| !e.base().is_passenger())).count() as i32;
        if count > max_cramming - 1 {
            seams::hurt(level, entity, LivingDamage::Cramming, 6.0);
        }
    }
    for id in pushable {
        level.with_entity(id, |level, other| entity.do_push(level, other));
    }
}

/// `LivingEntity.doPush(entity)` = `entity.push(this)`.
pub fn do_push(level: &mut ServerLevel, this: &mut dyn Entity, other: &mut dyn Entity) {
    push(level, other, this);
}

/// `LivingEntity.push(entity)` / `Entity.push(entity)` with `this` the entity being pushed into and `entity` the
/// pusher: both move apart.
pub fn push(level: &ServerLevel, this: &mut dyn Entity, entity: &mut dyn Entity) {
    // `LivingEntity.push`: sleeping entities do not take part
    if this.living().is_some_and(|l| l.is_sleeping()) {
        return;
    }
    if level.is_passenger_of_same_vehicle(this.as_dyn(), entity.as_dyn()) {
        return;
    }
    if !entity.base().no_physics && !this.base().no_physics {
        let mut xa = entity.base().x() - this.base().x();
        let mut za = entity.base().z() - this.base().z();
        let mut dd = mth::abs_max_f64(xa, za);
        if dd >= f64::from(0.01f32) {
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let mut pow = 1.0 / dd;
            if pow > 1.0 {
                pow = 1.0;
            }
            xa *= pow;
            za *= pow;
            xa *= f64::from(0.05f32);
            za *= f64::from(0.05f32);
            if !this.base().is_vehicle() && this.is_pushable(level) {
                push_xyz(this, -xa, 0.0, -za);
            }
            if !entity.base().is_vehicle() && entity.is_pushable(level) {
                push_xyz(entity, xa, 0.0, za);
            }
        }
    }
}

/// `Entity.push(xa, ya, za)`.
fn push_xyz(entity: &mut dyn Entity, xa: f64, ya: f64, za: f64) {
    if xa.is_finite() && ya.is_finite() && za.is_finite() {
        let dm = entity.base().delta_movement().add_xyz(xa, ya, za);
        entity.base_mut().set_delta_movement(dm);
        entity.base_mut().needs_sync = true;
    }
}
