//! Sign goldens: scenarios `tests/oracle/scenarios/blk_signs_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_signs_*.txt`).
//! `blk_signs_click` runs the click events of waxed signs with `allow_op_features` (commands, dialogs); the other scenarios keep it off.

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

#[test]
fn click_events_match_vanilla() {
    common::block_scenario::check("blk_signs_click");
}

#[test]
fn line_resolution_on_data_merge_matches_vanilla() {
    common::block_scenario::check("blk_signs_resolve");
}

#[test]
fn line_resolution_on_setblock_fill_and_clone_matches_vanilla() {
    common::block_scenario::check("blk_signs_resolve_commands");
}

#[test]
fn sign_line_with_unparsable_nbt_path_keeps_loaded_fields_like_vanilla() {
    common::block_scenario::check("blk_signs_bad_nbt_path");
}
