//! Interim call sites of other slices' APIs that are not on `EntitiesCore` yet; each function is replaced by the
//! owner's real call on rebase and this file is deleted.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::RemovalReason;

/// The damage sources `LivingEntity` movement/tick code hurts with (`DamageSources` factory names).
#[derive(Clone, Copy, Debug)]
pub enum LivingDamage {
    InWall,
    OutOfBorder,
    Drown,
    Cramming,
    FlyIntoWall,
    Freeze,
}

/// `this.hurtServer(level, this.damageSources().<kind>(), amount)`.
pub fn hurt(level: &mut ServerLevel, entity: &mut dyn Entity, kind: LivingDamage, amount: f32) {
    let sources = level.damage_sources();
    let source = match kind {
        LivingDamage::InWall => sources.in_wall(),
        LivingDamage::OutOfBorder => sources.out_of_border(),
        LivingDamage::Drown => sources.drown(),
        LivingDamage::Cramming => sources.cramming(),
        LivingDamage::FlyIntoWall => sources.fly_into_wall(),
        LivingDamage::Freeze => sources.freeze(),
    };
    entity.hurt_server(level, &source, amount);
}

/// SEAM(EcEnchant): `EnchantmentHelper.tickEffects(level, entity)`.
pub fn enchantment_tick_effects(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcEnchant): `EnchantmentHelper.runLocationChangedEffects(level, entity)`.
pub fn run_location_changed_effects(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcEffects): `tickEffects()`.
pub fn tick_effects(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcEffects): `triggerOnDeathMobEffects(level, reason)`.
pub fn trigger_on_death_mob_effects(_level: &mut ServerLevel, _entity: &mut dyn Entity, _reason: RemovalReason) {}

/// SEAM(EcEffects): `detectEquipmentUpdates()`.
pub fn detect_equipment_updates(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcEffects): `refreshDirtyAttributes()`.
pub fn refresh_dirty_attributes(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcDamage): `getCombatTracker().recheckStatus()`.
pub fn combat_tracker_recheck_status(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}

/// SEAM(EcEntityBase): `applyEffectsFromBlocks()`.
pub fn apply_effects_from_blocks(_level: &mut ServerLevel, _entity: &mut dyn Entity) {}
