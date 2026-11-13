//! Lightning bolts replayed against the real vanilla level.

use crate::common;

#[test]
fn lightning_matches_vanilla() {
    common::block_scenario::check("lightning");
}
