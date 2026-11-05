//! `AbstractGolem` (`PathfinderMob`): no ambient/hurt/death sound of its own, ambient interval 120, never despawns when
//! far away. The concrete golems embed a `MobData` of kind `PathfinderMob` and forward these two overrides.

/// `getAmbientSoundInterval()`.
pub const AMBIENT_SOUND_INTERVAL: i32 = 120;
