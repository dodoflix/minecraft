//! `LivingEntity` damage pipeline: `isInvulnerableTo`, `hurtServer`, `die`, `causeFallDamage`.

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;

/// `LivingEntity.isInvulnerableTo(level, source)`.
pub fn is_invulnerable_to(entity: &dyn Entity, level: &ServerLevel, source: &DamageSource) -> bool {
    entity.is_invulnerable_to_base(level, source)
}

/// `LivingEntity.hurtServer(level, source, damage)`.
pub fn hurt_server(_entity: &mut dyn Entity, _level: &mut ServerLevel, _source: &DamageSource, _amount: f32) -> bool {
    false
}

/// `LivingEntity.die(source)`.
pub fn die(_entity: &mut dyn Entity, _level: &mut ServerLevel, _source: &DamageSource) {}

/// `Entity.causeFallDamage` / `LivingEntity.causeFallDamage`.
pub fn cause_fall_damage(_entity: &mut dyn Entity, _level: &mut ServerLevel, _fall_distance: f64, _damage_modifier: f32, _source: &DamageSource) -> bool {
    false
}
