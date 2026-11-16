//! The end crystal entity and item replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): its data and
//! NBT, what removes it and the explosion order, and where the item places it.

use crate::common;

#[test]
fn end_crystal_data_removal_and_explosion() {
    common::block_scenario::check("ent_end_crystal");
}

/// In the End's fight a crystal in air lights its block, the fight counts the crystals of the pillars and tells the dragon of a destroyed one.
#[test]
fn end_crystal_in_the_fight() {
    common::block_scenario::check("ent_end_crystal_end");
}

/// `/kill` passes the command source's level to `kill`: a crystal the overworld console kills never tells the End's fight, `execute in` the End does.
#[test]
fn end_crystal_killed_from_another_level() {
    common::block_scenario::check("ent_end_crystal_kill_levels");
}

#[test]
fn end_crystal_item_placement() {
    common::block_scenario::check("itm_end_crystal");
}
