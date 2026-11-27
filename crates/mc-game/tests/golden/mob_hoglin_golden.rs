//! Hoglin, zoglin and strider against vanilla: a hoglin attacking a player, the overworld zombification, a zoglin fighting, a strider on
//! lava and a ridden one (`tests/oracle/scenarios/mob_*.txt`; fixtures from `scripts/block-oracle.sh NAME`).

use crate::blocks_dispenser_spine_golden::check;

/// A hoglin walking up to a survival player and hitting it: the damage roll and the throw knockback.
#[test]
fn hoglin_vs_player() {
    check("mob_hoglin_vs_player");
}

/// The overworld timer converts an adult and a baby hoglin to zoglins; an immune hoglin stays.
#[test]
fn hoglin_zombify() {
    check("mob_hoglin_zombify");
}

/// Hoglins created with `finalizeSpawn`: a baby has the attack damage and experience of a baby.
#[test]
fn hoglin_baby_spawn() {
    check("mob_hoglin_baby_spawn");
}

/// Zoglins fighting a player and a cow, the baby's quicker attacks, and the retaliation target after a player hurts them.
#[test]
fn zoglin_attack() {
    check("mob_zoglin_attack");
}

/// A strider walking on lava, cold and slow on land, and the jockeys `finalizeSpawn` adds.
#[test]
fn strider_lava() {
    check("mob_strider_lava");
}

/// A saddled strider steered with a warped fungus on a stick over lava and land, saddling and dismounting.
#[test]
fn strider_ride() {
    check("mob_strider_ride");
}

/// A rider leaves a strider on a lava lake whose only shore lies beside it, and a stick in the saddle slot saddles nothing.
#[test]
fn strider_dismount_island() {
    check("mob_strider_dismount_island");
}
