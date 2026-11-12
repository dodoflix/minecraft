//! Brush item scenario `tests/oracle/scenarios/itm_brush_use.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh itm_brush_use`) vs our ticking level.

use crate::common;

#[test]
fn brushes_brush_suspicious_blocks_until_released_or_broken() {
    common::block_scenario::check("itm_brush_use");
}
