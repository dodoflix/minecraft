//! Golden of the eye of ender: `EnderEyeItem.useOn` and the frame check of `use` (`itm_ender_eye_frame`), the launch towards the nearest
//! stronghold in a normal world (`itm_ender_eye_throw`) and where none is located (`itm_ender_eye_nostructure`), and the flight, drop and
//! break of the `EyeOfEnder` entity (`ent_eye_flight`, `ent_eye_drop_or_break`): scenarios `tests/oracle/scenarios/NAME.txt` replayed on the
//! real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn eyes_fill_end_portal_frames_and_the_last_eye_opens_the_portal() {
    common::block_scenario::check("itm_ender_eye_frame");
}

#[test]
fn eye_launches_towards_the_nearest_stronghold() {
    common::block_scenario::check("itm_ender_eye_throw");
}

#[test]
fn eye_is_not_thrown_without_a_stronghold() {
    common::block_scenario::check("itm_ender_eye_nostructure");
}

#[test]
fn eye_flies_towards_its_target() {
    common::block_scenario::check("ent_eye_flight");
}

#[test]
fn eye_drops_or_breaks_after_its_flight() {
    common::block_scenario::check("ent_eye_drop_or_break");
}
