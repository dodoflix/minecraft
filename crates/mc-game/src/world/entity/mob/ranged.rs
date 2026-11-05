//! `net.minecraft.world.entity.monster.RangedAttackMob` and `CrossbowAttackMob`: the interfaces the ranged-attack goals
//! drive. A mob implements the trait on its struct and exposes it through `Entity::as_ranged_attack_mob_mut` /
//! `Entity::as_crossbow_attack_mob_mut`.

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;

/// `RangedAttackMob`.
pub trait RangedAttackMob {
    /// `performRangedAttack(target, power)`.
    fn perform_ranged_attack(&mut self, level: &mut ServerLevel, target: EntityId, power: f32);

    /// `rangedAttackUncertainty(level)`.
    fn ranged_attack_uncertainty(&self, level: &ServerLevel) -> f32 {
        (14 - level.difficulty().get_id() * 4) as f32
    }
}

/// `CrossbowAttackMob` (`performCrossbowAttack` is `projectile::util`-side: it needs the crossbow item shooting).
pub trait CrossbowAttackMob: RangedAttackMob {
    /// `setChargingCrossbow(isCharging)`.
    fn set_charging_crossbow(&mut self, is_charging: bool);

    /// `onCrossbowAttackPerformed()`.
    fn on_crossbow_attack_performed(&mut self);
}
