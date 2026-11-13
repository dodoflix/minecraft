//! Minecarts against vanilla: rail movement of `OldMinecartBehavior` (slopes, curves, powered and detector rails, derailing, water),
//! cart-cart pushing, damage, riding and saving (`tests/oracle/scenarios/mc_old_*.txt`, `ent_minecart_*.txt`; fixtures regenerated
//! with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn flat_rails_boost_brake_and_derail() {
    common::block_scenario::check("mc_old_flat");
}

#[test]
fn curved_rails() {
    common::block_scenario::check("mc_old_curve");
}

#[test]
fn slopes() {
    common::block_scenario::check("mc_old_slope");
}

#[test]
fn junction_and_detector_rail() {
    common::block_scenario::check("mc_old_junction");
}

#[test]
fn derailed_carts() {
    common::block_scenario::check("mc_old_derail");
}

#[test]
fn carts_in_water() {
    common::block_scenario::check("mc_old_water");
}

#[test]
fn carts_pushing_each_other() {
    common::block_scenario::check("mc_old_collide");
}

#[test]
fn powered_rail_starts_a_standing_cart() {
    common::block_scenario::check("mc_old_push_start");
}

#[test]
fn damage_wobble_and_break() {
    common::block_scenario::check("mc_old_hurt");
}

#[test]
fn activator_rail_ejects_the_rider() {
    common::block_scenario::check("mc_old_activator");
}

#[test]
fn fast_cart_picks_up_mobs() {
    common::block_scenario::check("mc_old_rider_pickup");
}

#[test]
fn save_data_round_trip() {
    common::block_scenario::check("ent_minecart_nbt");
}

#[test]
fn drops_and_the_entity_drops_rule() {
    common::block_scenario::check("ent_minecart_drops");
}

#[test]
fn riding_and_dismounting() {
    common::block_scenario::check("ent_minecart_ride");
}

#[test]
fn dismounting_where_the_sides_are_blocked() {
    common::block_scenario::check("ent_minecart_dismount_blocked");
}

#[test]
fn pick_block() {
    common::block_scenario::check("ent_minecart_pick");
}

#[test]
fn riders_of_different_kinds() {
    common::block_scenario::check("mc_old_rider_kinds");
}

#[test]
fn lava_burns_the_cart() {
    common::block_scenario::check("mc_old_lava");
}

#[test]
fn rails_ignore_the_block_below() {
    common::block_scenario::check("mc_old_soul_sand");
}
