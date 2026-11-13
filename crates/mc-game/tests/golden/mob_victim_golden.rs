//! A mob as the victim of damage: invulnerability frames, armor and enchantment reduction, knockback, shield blocking, death
//! animation, loot with kill credit and looting, experience and the last-hurt-by memory, replayed against the real vanilla level
//! (scenarios recorded with `scripts/block-oracle.sh`).

use crate::common;

/// A cow hit by a player's sword, a zombie, arrows, an explosion and fire, with hits inside and outside the invulnerability window.
#[test]
fn cow_hurt_by_every_source_matches_vanilla() {
    common::block_scenario::check("mob_hurt_cow");
}

/// Death loot with and without player credit, looting, equipment drop chances, a skeleton's and a charged creeper's victims, a
/// splitting slime, the experience orbs and the death animation.
#[test]
fn mob_deaths_drop_loot_and_experience_like_vanilla() {
    common::block_scenario::check("mob_death_loot_xp");
}

/// A blocking player hit by a hoglin, a baby hoglin, a zoglin and an axe-wielding zombie: the attacker's own reaction to the block.
#[test]
fn shield_blocks_with_hoglins_match_vanilla() {
    common::block_scenario::check("mob_block_shield_hoglin");
}

/// Zombies hit through armor points, armor durability, protection enchantments, resistance, absorption and fire resistance.
#[test]
fn armored_mob_takes_reduced_damage_like_vanilla() {
    common::block_scenario::check("mob_hurt_armored");
}
