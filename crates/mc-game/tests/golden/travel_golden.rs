//! Entity classes travelling through nether portals, end portals and end gateways in a replay that holds several levels
//! (`tests/oracle/scenarios/travel_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

/// Arrows, tridents, rockets, wind charges, thrown items, fireballs and potions cross without a delay and keep owner and flags.
#[test]
fn projectiles_cross_a_nether_portal() {
    common::block_scenario::check("travel_projectiles");
}

/// Every minecart class keeps its contents and data across a portal.
#[test]
fn minecarts_keep_their_contents() {
    common::block_scenario::check("travel_minecarts");
}

/// Chest boats, rafts and boats with nested passengers travel as a whole.
#[test]
fn boats_take_contents_and_passengers() {
    common::block_scenario::check("travel_boats");
}

/// Leashed cows and boats lose the link in the old level and keep the saved link in the new one.
#[test]
fn leashed_entities_leave_their_holder_behind() {
    common::block_scenario::check("travel_leashed");
}

/// Tamed pets keep their owner and data when they cross without their owner.
#[test]
fn tamed_pets_travel_without_their_owner() {
    common::block_scenario::check("travel_pets");
}

/// Villagers, a baby, a wandering trader and a curing zombie villager keep memories and timers.
#[test]
fn villagers_keep_their_brain_memories() {
    common::block_scenario::check("travel_villagers");
}

/// Equipment, effects, fire, fall distance and jockeys survive the trip.
#[test]
fn mobs_keep_equipment_effects_and_riders() {
    common::block_scenario::check("travel_mobs");
}

/// Armor stands travel; displays and markers never move into a portal; experience orbs merge on arrival.
#[test]
fn decorations_and_orbs_in_a_portal() {
    common::block_scenario::check("travel_decor");
}

/// Items merge into the stack at their arrival point and keep thrower, owner, age and health.
#[test]
fn items_merge_at_arrival() {
    common::block_scenario::check("travel_items");
}

/// A primed tnt that used a portal explodes with the portal damage calculator.
#[test]
fn primed_tnt_keeps_owner_and_uses_the_portal_calculator() {
    common::block_scenario::check("travel_tnt");
}

/// Falling blocks cross a nether portal with their block, time and block entity data.
#[test]
fn falling_blocks_cross_a_nether_portal() {
    common::block_scenario::check("travel_falling_blocks");
}

/// Entities fall into an end portal and arrive on the end platform; from the end they return to the world spawn.
#[test]
fn entities_cross_an_end_portal() {
    common::block_scenario::check("travel_end_portal");
}

/// An end gateway teleports every class of entity to its exact exit and cools down.
#[test]
fn entities_cross_an_end_gateway() {
    common::block_scenario::check("travel_end_gateway");
}

/// Piglins and hoglins carry their overworld counter across a portal and zombify in the overworld.
#[test]
fn piglins_and_hoglins_zombify_in_the_overworld() {
    common::block_scenario::check("travel_zombification");
}

/// The dragon and the wither refuse portals; a wither on a boat travels with it.
#[test]
fn dragon_and_wither_refuse_portals() {
    common::block_scenario::check("travel_refusals");
}

/// Chorus fruit, bubble columns and a landing pearl reset the fall distance.
#[test]
fn fall_distance_resets() {
    common::block_scenario::check("travel_fall_resets");
}

/// A tamed wolf in the overworld while its owner is in the nether, and after the owner returns.
#[test]
fn a_pet_waits_for_its_owner_in_another_level() {
    common::block_scenario::check("travel_pet_follow");
}

/// The watchers of both levels get the packets of a trip: removal, new entity with its UUID, equipment, passengers, no link update for the removed entity.
#[test]
fn watchers_of_both_levels_hear_of_a_trip() {
    common::block_scenario::check("travel_observer");
}

/// A primed tnt that used a portal credits the owner it has in another level with what its blast kills.
#[test]
fn primed_tnt_credits_an_owner_in_another_level() {
    common::block_scenario::check("travel_tnt_credit");
}

/// A fishing bobber standing in a nether portal stays where it is.
#[test]
fn fishing_bobber_stays_in_a_portal() {
    common::block_scenario::check("travel_bobber");
}

/// The end platform replaces blocks with drops: the items it drops match.
#[test]
fn end_platform_replaces_blocks_and_drops_items() {
    common::block_scenario::check("travel_end_platform_drop");
}

/// A chunk of the End that a block dump read (and so generated) before the platform was built: the items it drops match.
#[test]
fn end_platform_over_a_chunk_that_was_read_drops_items() {
    common::block_scenario::check("travel_end_platform_read_drop");
}

/// The end platform generates a chunk of its own while it is built: the items it drops match.
#[test]
fn end_platform_generates_a_chunk_and_drops_items() {
    common::block_scenario::check("travel_end_platform_build_drop");
}

/// A creaking heart a primed tnt destroys for an owner in another level drops experience when that owner is not in creative mode.
#[test]
fn primed_tnt_of_an_owner_in_another_level_pops_a_creaking_hearts_experience() {
    common::block_scenario::check("travel_tnt_creaking_heart");
}

/// A lead outlives a level change of its holder (home restriction kept), survives the holder's return and drops after its logout.
#[test]
fn leash_survives_its_holder_changing_level() {
    common::block_scenario::check("travel_leash_holder_levels");
}

/// A boat with a rider and a pig, and a horse with its rider, go through a nether portal and are teleported across levels while seated.
#[test]
fn mounted_riders_cross_levels() {
    common::block_scenario::check("travel_portal_mounted");
}
