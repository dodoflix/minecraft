//! Pillager and vindicator against the vanilla oracle (scenarios in `oracle/scenarios`): the crossbow charge and shot, `finalizeSpawn`,
//! the inventory slots, the Johnny flag and a raid vindicator breaking a door.
use crate::common::block_scenario;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn pillager_crossbow_volleys() {
    run_scenario_test("mob_pillager_crossbow");
}

#[test]
fn pillager_finalize_spawn() {
    block_scenario::check("mob_pillager_finalize");
}

#[test]
fn pillager_inventory_slots() {
    block_scenario::check("mob_pillager_inventory");
}

#[test]
fn vindicator_johnny_attacks_cows() {
    block_scenario::check("mob_vindicator_johnny");
}

#[test]
fn vindicator_raid_breaks_door() {
    block_scenario::check("mob_vindicator_chase");
}
