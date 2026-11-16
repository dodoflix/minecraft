//! Nether portal travel in a replay that holds the overworld and the nether (`tests/oracle/scenarios/portal_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

/// A survival player waits out the default delay, arrives in a portal built for it and travels back.
#[test]
fn survival_player_travels_to_the_nether_and_back() {
    common::block_scenario::check("portal_nether_player");
}

/// The xp pickup delay is counted down once per tick across a portal trip (the part of the tick before the portal step runs in the old level).
#[test]
fn xp_pickup_delay_counts_down_across_the_portal_trip() {
    common::block_scenario::check("portal_nether_xp_delay");
}

/// A villager that is trading, a cow, an item, a primed tnt, a minecart, a zombified piglin and an arrow travel without a delay.
#[test]
fn other_entities_travel_without_a_delay() {
    common::block_scenario::check("portal_nether_entities");
}

/// A boat with two mobs, a boat with a player and a minecart with a cow travel as a whole.
#[test]
fn vehicles_take_their_passengers_through() {
    common::block_scenario::check("portal_nether_vehicles");
}

/// A cow that arrived stays put while it is in the arrival portal; it travels again after its 300 tick cooldown.
#[test]
fn mobs_wait_300_ticks_between_portals() {
    common::block_scenario::check("portal_nether_cooldown_300");
}

/// The portal search of the nether (16 blocks) picks an existing portal or builds a new one.
#[test]
fn nether_portal_search_reaches_16_blocks() {
    common::block_scenario::check("portal_nether_search");
}

/// The portal search of the overworld (128 blocks).
#[test]
fn overworld_portal_search_reaches_128_blocks() {
    common::block_scenario::check("portal_nether_search_far");
}

/// A random tick of an overworld portal block spawns a zombified piglin.
#[test]
fn portal_random_tick_spawns_a_zombified_piglin() {
    common::block_scenario::check("portal_piglin_spawn");
}

/// A pearl thrown into a portal uses it; when it lands in the nether its owner is teleported there.
#[test]
fn ender_pearl_travels_through_a_portal_and_takes_its_owner() {
    common::block_scenario::check("portal_nether_pearl");
}

/// Creative and default portal delays, a delay of 0 and a negative delay.
#[test]
fn player_portal_delays_follow_the_game_rules() {
    common::block_scenario::check("portal_nether_delays");
}

/// A spectator never builds a portal, it uses an existing one.
#[test]
fn spectators_use_existing_portals_only() {
    common::block_scenario::check("portal_nether_spectator");
}

/// The destination is clamped into the border of the destination level.
#[test]
fn destination_is_clamped_into_the_world_border() {
    common::block_scenario::check("portal_world_border");
}

/// Entering the nether, walking 900 blocks and returning 7200 blocks from the entry earns `changed_dimension` and `nether_travel`.
#[test]
fn a_long_trip_through_the_nether_awards_fast_travel() {
    common::block_scenario::check("portal_nether_travel");
}
