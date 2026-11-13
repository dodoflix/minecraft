//! The minecart item and its dispenser behaviour against the real vanilla level (`scripts/block-oracle.sh NAME`): where a cart is placed
//! on each rail shape, what the stack's components change and which dispenser cases drop the item instead.

use crate::common;

#[test]
fn minecart_item_placement() {
    common::block_scenario::check("itm_minecart");
}

#[test]
fn minecart_dispenser_behaviour() {
    common::block_scenario::check("blk_disp_minecart");
}
