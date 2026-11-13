//! Minecarts against vanilla with the `minecart_improvements` experiment enabled (`NewMinecartBehavior`): rail movement in steps,
//! slopes, curves, powered rails, collisions, riders, the packets of the steps and the `max_minecart_speed` rule
//! (`tests/oracle/scenarios/mc_new_*.txt`, `ent_new_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn flat_rails_boost_brake_and_derail() {
    common::block_scenario::check("mc_new_flat");
}

#[test]
fn curved_rails() {
    common::block_scenario::check("mc_new_curve");
}

#[test]
fn slopes() {
    common::block_scenario::check("mc_new_slope");
}

#[test]
fn junction_and_detector_rail() {
    common::block_scenario::check("mc_new_junction");
}

#[test]
fn derailed_carts() {
    common::block_scenario::check("mc_new_derail");
}

#[test]
fn carts_in_water() {
    common::block_scenario::check("mc_new_water");
}

#[test]
fn carts_pushing_each_other() {
    common::block_scenario::check("mc_new_collide");
}

#[test]
fn powered_rail_starts_a_standing_cart() {
    common::block_scenario::check("mc_new_push_start");
}

#[test]
fn damage_wobble_and_break() {
    common::block_scenario::check("mc_new_hurt");
}

#[test]
fn activator_rail_ejects_the_rider() {
    common::block_scenario::check("mc_new_activator");
}

#[test]
fn fast_cart_picks_up_mobs() {
    common::block_scenario::check("mc_new_rider_pickup");
}

#[test]
fn riders_of_different_kinds() {
    common::block_scenario::check("mc_new_rider_kinds");
}

#[test]
fn lava_burns_the_cart() {
    common::block_scenario::check("mc_new_lava");
}

#[test]
fn rails_ignore_the_block_below() {
    common::block_scenario::check("mc_new_soul_sand");
}

#[test]
fn furnace_cart_pushes_along_rails() {
    common::block_scenario::check("mc_new_furnace");
}

#[test]
fn hopper_cart_sucks_a_chest_and_item_entities_while_rolling() {
    common::block_scenario::check("mc_new_hopper");
}

#[test]
fn chest_carts_slow_down_by_their_fill() {
    common::block_scenario::check("mc_new_chest");
}

#[test]
fn tnt_cart_explodes_against_a_wall() {
    common::block_scenario::check("mc_new_tnt_collide");
}

#[test]
fn save_data_round_trip() {
    common::block_scenario::check("ent_new_minecart_nbt");
}

#[test]
fn riding_and_dismounting() {
    common::block_scenario::check_with("ent_new_minecart_ride", &ignore_on_ground());
}

#[test]
fn dismounting_where_the_sides_are_blocked() {
    common::block_scenario::check_with(
        "ent_new_minecart_dismount_blocked",
        &common::entity_oracle::Options {
            ignore_fields: vec!["og", "delta"],
            ..Default::default()
        },
    );
}

#[test]
fn tracking_players_receive_the_steps() {
    common::block_scenario::check("ent_new_minecart_packets");
}

#[test]
fn max_minecart_speed_rule() {
    common::block_scenario::check("ent_new_max_speed");
}

#[test]
fn pistons_push_carts() {
    common::block_scenario::check("mc_new_piston");
}

// same as the old behaviour's riding replay: the flag of a player that only sends packets is derived again in the replay
fn ignore_on_ground() -> common::entity_oracle::Options {
    common::entity_oracle::Options {
        ignore_fields: vec!["og"],
        ..Default::default()
    }
}
