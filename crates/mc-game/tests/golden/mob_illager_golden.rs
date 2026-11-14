//! Illagers (pillager, vindicator, evoker, illusioner) and the evoker's vexes against the vanilla oracle (scenarios in `oracle/scenarios`): the crossbow charge and shot, `finalizeSpawn`,
//! the inventory slots, the Johnny flag, a raid vindicator breaking a door, the spells of the evoker and the illusioner.
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

#[test]
fn evoker_fangs_row() {
    block_scenario::check("mob_evoker_fangs_row");
}

#[test]
fn evoker_fangs_circle() {
    block_scenario::check("mob_evoker_fangs_circle");
}

#[test]
fn evoker_summons_vexes() {
    block_scenario::check("mob_evoker_summon_vex");
}

#[test]
fn evoker_wololo_recolours_blue_sheep() {
    block_scenario::check("mob_evoker_wololo");
}

#[test]
fn evoker_drops_loot() {
    block_scenario::check("mob_evoker_drops");
}

#[test]
fn illusioner_mirror_and_blindness() {
    block_scenario::check("mob_illusioner_blind_mirror");
}

#[test]
fn illusioner_shoots_bow() {
    block_scenario::check("mob_illusioner_bow");
}

#[test]
fn vex_charges_through_walls_and_starves() {
    block_scenario::check("mob_vex_charge");
}
