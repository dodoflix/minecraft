//! Eating, drinking, early release and the equippable swap through the player action verbs: scenarios
//! `tests/oracle/scenarios/itm_{eat_basic,eat_remainder,drink_effects,release_early,equip_swap}.txt`, fixtures from `scripts/block-oracle.sh NAME`.
use crate::common::block_scenario::check;

#[test]
fn eating_food_matches_vanilla() {
    check("itm_eat_basic");
}

#[test]
fn remainders_match_vanilla() {
    check("itm_eat_remainder");
}

#[test]
fn drinks_and_effects_match_vanilla() {
    check("itm_drink_effects");
}

#[test]
fn early_release_matches_vanilla() {
    check("itm_release_early");
}

#[test]
fn equippable_swap_matches_vanilla() {
    check("itm_equip_swap");
}
