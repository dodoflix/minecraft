//! Fishing rod and bobber against vanilla: `FishingRodItem` casting and reeling in, the bite timeline and loot, the open water rule, hooked
//! entities, the stop conditions, the world border bounce and Lure / Luck of the Sea (`tests/oracle/scenarios/{itm,ent}_fishing_*` replayed on the
//! real vanilla level; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    bite_timeline_in_clear_weather_and_rain => "ent_fishing_bite",
    bobber_bounces_off_the_world_border => "ent_fishing_border",
    lure_and_luck_of_the_sea => "ent_fishing_enchants",
    bobber_hooks_entities_and_reeling_in_pulls_them => "ent_fishing_hooked_entity",
    open_water_rule => "ent_fishing_open_water",
    bobber_stops_when_the_owner_cannot_fish => "ent_fishing_stop",
    rod_casts_and_reels_in => "itm_fishing_rod",
);
