//! `Monster` (`Enemy`): hostile sounds, light-based `noActionTime`, walk target value, drop rules.

use super::MobKind;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

/// Whether `e` is a vanilla `Monster`.
pub fn is_monster(e: &dyn Entity) -> bool {
    e.mob().is_some_and(|m| m.kind == MobKind::Monster)
}

/// `Monster.getSoundSource()`.
pub const SOUND_SOURCE: SoundSource = SoundSource::Hostile;
pub const SWIM_SOUND: SoundEvent = SoundEvent::ENTITY_HOSTILE_SWIM;
pub const SWIM_SPLASH_SOUND: SoundEvent = SoundEvent::ENTITY_HOSTILE_SPLASH;
pub const HURT_SOUND: SoundEvent = SoundEvent::ENTITY_HOSTILE_HURT;
pub const DEATH_SOUND: SoundEvent = SoundEvent::ENTITY_HOSTILE_DEATH;
pub const FALL_SOUNDS: (SoundEvent, SoundEvent) = (SoundEvent::ENTITY_HOSTILE_SMALL_FALL, SoundEvent::ENTITY_HOSTILE_BIG_FALL);

/// `Monster.shouldDropLoot(level)`.
pub fn should_drop_loot(level: &ServerLevel) -> bool {
    level.mob_drops()
}
