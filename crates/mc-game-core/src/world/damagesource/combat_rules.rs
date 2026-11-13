//! `net.minecraft.world.damagesource.CombatRules`.

use mc_core::math::mth;

use super::damage_source::DamageSource;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment_helper;

pub const MAX_ARMOR: f32 = 20.0;
pub const ARMOR_PROTECTION_DIVIDER: f32 = 25.0;
pub const BASE_ARMOR_TOUGHNESS: f32 = 2.0;
pub const MIN_ARMOR_RATIO: f32 = 0.2;

/// `getDamageAfterAbsorb(victim, damage, source, totalArmor, armorToughness)`.
pub fn get_damage_after_absorb(
    level: &mut ServerLevel,
    victim: &mut dyn Entity,
    damage: f32,
    source: &DamageSource,
    total_armor: f32,
    armor_toughness: f32,
) -> f32 {
    let toughness = 2.0 + armor_toughness / 4.0;
    let real_armor = mth::clamp_f32(total_armor - damage / toughness, total_armor * 0.2, 20.0);
    let armor_fraction = real_armor / 25.0;
    let modified_armor_fraction = match source.get_weapon_item() {
        Some(weapon) => {
            let victim_id = victim.id();
            let mut actors = Actors::of(victim);
            mth::clamp_f32(
                enchantment_helper::modify_armor_effectiveness(
                    level,
                    &mut actors,
                    weapon,
                    victim_id,
                    source,
                    armor_fraction,
                ),
                0.0,
                1.0,
            )
        }
        None => armor_fraction,
    };
    let damage_multiplier = 1.0 - modified_armor_fraction;
    damage * damage_multiplier
}

/// `getDamageAfterMagicAbsorb(damage, totalMagicArmor)`.
pub fn get_damage_after_magic_absorb(damage: f32, total_magic_armor: f32) -> f32 {
    let real_armor = mth::clamp_f32(total_magic_armor, 0.0, 20.0);
    damage * (1.0 - real_armor / 25.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_absorb_is_linear_and_clamped() {
        assert_eq!(get_damage_after_magic_absorb(10.0, 0.0), 10.0);
        assert_eq!(
            get_damage_after_magic_absorb(10.0, 20.0),
            10.0 * (1.0 - 20.0f32 / 25.0)
        );
        assert_eq!(
            get_damage_after_magic_absorb(10.0, 99.0),
            10.0 * (1.0 - 20.0f32 / 25.0)
        );
    }
}
