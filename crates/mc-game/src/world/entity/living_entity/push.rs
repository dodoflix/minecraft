//! Collision pushing between entities: `LivingEntity.pushEntities` (incl. cramming damage), `doPush`, `isPushable`.
//!
//! `EntitySelector.pushableBy`'s team collision rules are not applied (no scoreboard access from the level yet).

use super::seams::{self, LivingDamage};
use super::util::is_position_entity_ticking;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
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
pub fn do_push(level: &ServerLevel, this: &mut dyn Entity, other: &mut dyn Entity) {
    other.push_entity(level, this);
}
