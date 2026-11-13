//! Minecarts against vanilla: rail movement of `OldMinecartBehavior` (slopes, curves, powered and detector rails, derailing, water),
//! cart-cart pushing, damage, riding and saving (`tests/oracle/scenarios/mc_old_*.txt`, `ent_minecart_*.txt`; fixtures regenerated
//! with `scripts/block-oracle.sh NAME`).

use crate::common;

// `og` of a player that only sends packets is left out: vanilla keeps the flag the ride set, the replay derives it again on the first
// tick after the dismount (the position, pose and packets are compared)
fn ignore_on_ground() -> common::entity_oracle::Options {
    common::entity_oracle::Options {
        ignore_fields: vec!["og"],
        ..Default::default()
    }
}

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
    common::block_scenario::check_with("ent_minecart_ride", &ignore_on_ground());
}

// the replay applies a physics step to a rider in the tick it is dismounted, vanilla keeps the motion of the riding tick; position,
// pose and packets are compared
#[test]
fn dismounting_where_the_sides_are_blocked() {
    common::block_scenario::check_with(
        "ent_minecart_dismount_blocked",
        &common::entity_oracle::Options {
            ignore_fields: vec!["og", "delta"],
            ..Default::default()
        },
    );
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
