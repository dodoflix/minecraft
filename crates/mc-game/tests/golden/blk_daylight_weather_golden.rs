//! Daylight detector under rain and thunder: the weather layer of the environment attributes drives the sky light level
//! (scenarios `tests/oracle/scenarios/blk_daylight_rain_{ramp,times}.txt`, `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn rain_lowers_the_sky_light_level_as_the_rain_level_ramps() {
    common::block_scenario::check("blk_daylight_rain_ramp");
}

#[test]
fn daylight_detector_follows_rain_time_and_dimension() {
    common::block_scenario::check("blk_daylight_rain_times");
}
