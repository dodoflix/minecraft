//! Sign goldens: scenarios `tests/oracle/scenarios/blk_signs_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_signs_*.txt`).
//! `run_command` click events are not run (`Not implemented:` in `SignBlockEntity`); the scenarios keep `allow_op_features` off.

use crate::common;

#[test]
fn sign_block_entities_match_vanilla() {
    common::block_scenario::check("blk_signs_nbt");
}

#[test]
fn sign_support_loss_matches_vanilla() {
    common::block_scenario::check("blk_signs_survive");
}

#[test]
fn player_placement_and_editing_matches_vanilla() {
    common::block_scenario::check("blk_signs_player");
}

#[test]
fn wall_and_hanging_variants_match_vanilla() {
    common::block_scenario::check("blk_signs_wall");
}

#[test]
fn applicators_match_vanilla() {
    common::block_scenario::check("blk_signs_apply");
}
