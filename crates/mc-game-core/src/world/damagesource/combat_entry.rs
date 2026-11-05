//! `net.minecraft.world.damagesource.CombatEntry`.

use super::damage_source::DamageSource;
use super::fall_location::FallLocation;

/// `CombatEntry(source, damage, fallLocation, fallDistance)`.
#[derive(Clone, Debug)]
pub struct CombatEntry {
    pub source: DamageSource,
    pub damage: f32,
    pub fall_location: Option<FallLocation>,
    pub fall_distance: f32,
}
