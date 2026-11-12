//! Lily pad, frogspawn, powder snow bucket, scaffolding while sneaking and `block_entity_data` on a barrel, command block
//! and spawner against vanilla (`tests/oracle/scenarios/itm_blockitem_kinds.txt`; fixture regenerated with
//! `scripts/block-oracle.sh itm_blockitem_kinds`).

use crate::common;

#[test]
fn block_item_kinds() {
    common::block_scenario::check("itm_blockitem_kinds");
}
