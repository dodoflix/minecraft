//! The end crystal entity and item replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): its data and
//! NBT, what removes it and the explosion order, and where the item places it.

use crate::common;

#[test]
fn end_crystal_data_removal_and_explosion() {
    common::block_scenario::check("ent_end_crystal");
}

#[test]
fn end_crystal_item_placement() {
    common::block_scenario::check("itm_end_crystal");
}
