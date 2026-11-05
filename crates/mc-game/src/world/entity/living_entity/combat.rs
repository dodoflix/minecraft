//! `LivingEntity.getCombatTracker()` calls: `recordDamage` / `recheckStatus` and the combat enter/leave hooks.

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;

/// `getCombatTracker().recordDamage(source, damage)` (then `onLeaveCombat()` / `onEnterCombat()` as the tracker decided).
pub fn record_damage(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, damage: f32) {
    let mut tracker = std::mem::take(&mut entity.living_mut().expect("living").combat_tracker);
    let events = tracker.record_damage(level, entity, source, damage);
    entity.living_mut().expect("living").combat_tracker = tracker;
    if events.left_combat {
        entity.on_leave_combat(level);
    }
    if events.entered_combat {
        entity.on_enter_combat(level);
    }
}

/// `getCombatTracker().recheckStatus()` (`LivingEntity.remove` / every 20 ticks of `tick()` / `ServerPlayer.die`).
pub fn recheck_status(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let mut tracker = std::mem::take(&mut entity.living_mut().expect("living").combat_tracker);
    let left_combat = tracker.recheck_status(entity);
    entity.living_mut().expect("living").combat_tracker = tracker;
    if left_combat {
        entity.on_leave_combat(level);
    }
}
