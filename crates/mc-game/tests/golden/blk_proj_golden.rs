//! Block reactions to projectile hits replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): bells, candles and
//! campfires, TNT, chorus flowers, amethyst and pointed dripstone, and the blocks arrows press (buttons, plates, tripwire).

use crate::common::block_scenario::check;

#[test]
fn bells_ring_only_when_hit_on_a_ringing_side() {
    check("blk_proj_bell");
}

#[test]
fn burning_projectiles_light_candles_cakes_and_campfires() {
    check("blk_proj_candles");
}

#[test]
fn burning_projectiles_prime_tnt_for_owners_who_may_break_it() {
    check("blk_proj_tnt");
}

#[test]
fn projectiles_break_chorus_flowers_under_the_break_rules() {
    check("blk_proj_chorus");
}

#[test]
fn projectiles_chime_amethyst_and_tridents_break_pointed_dripstone() {
    check("blk_proj_amethyst_dripstone");
}

#[test]
fn arrows_press_buttons_plates_and_tripwires() {
    check("blk_proj_buttons_plates");
}
