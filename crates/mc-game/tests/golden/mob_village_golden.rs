//! Village behaviours against the vanilla oracle (ticking-level replay): the zombie siege spawner, villagers that call a golem, doors that mobs
//! open or break, and mobs that walk through a village. Scenarios: `tests/oracle/scenarios/mob_{siege,village,door}_*.txt`, fixtures:
//! `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// A night roll of one in ten starts a siege: twenty zombies, one every third spawner tick, on a ring around the village the player stands in.
#[test]
fn siege_sets_up_and_spawns_zombies() {
    check("mob_siege_setup");
}

/// Villagers walking to their beds open the wooden door of the room and close it behind them.
#[test]
fn villagers_open_and_close_doors() {
    check("mob_door_villager");
}

/// Zombies at night walk toward the points of interest of a village (MoveThroughVillageGoal), up to a door they may break.
#[test]
fn zombies_walk_through_a_village() {
    check("mob_village_zombie_walk");
}

/// A fox strolls through the village at night and not by day.
#[test]
fn fox_strolls_through_a_village() {
    check("mob_village_fox_stroll");
}

/// The siege ends after its twentieth zombie; a failed roll, daylight, no enemy spawning and a player outside the village stop or reset it,
/// and the setup of a night stays until daylight or a reset.
#[test]
fn siege_ends_and_resets() {
    check("mob_siege_done");
}

/// A raider on easy difficulty opens a wooden door instead of breaking it.
#[test]
fn raider_opens_a_door() {
    check("mob_door_raider_open");
}

/// Villagers that gossip call an iron golem once five of them slept recently; the gossip cooldown keeps one pair from calling twice.
#[test]
fn villagers_summon_a_golem() {
    check("mob_village_golem_summon");
}

/// Raiders far from a village walk to it (PathfindToRaidGoal), then to its beds (RaiderMoveThroughVillageGoal) while the player is out of range.
#[test]
fn raiders_walk_to_their_village() {
    check("mob_raid_pathfind");
}
