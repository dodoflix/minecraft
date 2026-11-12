//! Using a bundle (`use`, `tick`, `release`, the dumps `entities`, `inventory:`, `stats:`, `use:`, `packets:`) replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn bundle_use_drops_one_stack_at_a_time_and_awards_the_stat() {
    common::block_scenario::check("itm_bundle_use");
}
