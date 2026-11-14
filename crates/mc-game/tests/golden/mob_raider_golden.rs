//! Ravager and witch against the vanilla oracle (scenarios in `oracle/scenarios`): the ravager's claw, shield stun and roar,
//! the leaves it breaks, and the witch's potion drinking, throwing, healing of raiders and the villager that lightning turns
//! into one.
use crate::common::block_scenario;

#[test]
fn ravager_attacks_stuns_and_roars() {
    block_scenario::check("mob_ravager_attack_roar");
}

#[test]
fn ravager_destroys_leaves_with_mob_griefing() {
    block_scenario::check("mob_ravager_destroy_leaves");
}

#[test]
fn witch_drinks_potions() {
    block_scenario::check("mob_witch_drink");
}

#[test]
fn witch_heals_raiders() {
    block_scenario::check("mob_witch_heals_raider");
}

#[test]
fn witch_throws_splash_potions() {
    block_scenario::check("mob_witch_potion_throw");
}

#[test]
fn villager_struck_by_lightning_becomes_a_witch() {
    block_scenario::check("mob_witch_lightning");
}
