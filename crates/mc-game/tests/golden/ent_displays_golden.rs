//! The block, item and text display entities replayed against the real vanilla level (`scripts/block-oracle.sh NAME`):
//! the data they are paired with, the NBT codec of every field, `/data merge`, the text resolution of `/summon` and
//! `/data merge`, riding, the `contents` slot of an item display and what displays do not do to blocks.

use crate::common;

#[test]
fn display_pairing_data_nbt_codec_and_merge() {
    common::block_scenario::check("ent_displays");
}

#[test]
fn text_display_resolves_selector_score_and_nbt_text() {
    common::block_scenario::check("ent_display_text");
}

#[test]
fn displays_ride_hold_an_item_slot_and_ignore_blocks() {
    common::block_scenario::check("ent_display_ride");
}
