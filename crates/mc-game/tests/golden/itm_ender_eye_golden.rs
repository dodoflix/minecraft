//! Golden of the eye of ender on end portal frames (`EnderEyeItem.useOn` and the frame check of `use`): scenario
//! `tests/oracle/scenarios/itm_ender_eye_frame.txt` replayed on the real vanilla level (`scripts/block-oracle.sh itm_ender_eye_frame`)
//! vs our ticking level.

use crate::common;

#[test]
fn eyes_fill_end_portal_frames_and_the_last_eye_opens_the_portal() {
    common::block_scenario::check("itm_ender_eye_frame");
}
