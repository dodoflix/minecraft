//! Block scenarios of the explosion, door-destruction, bed and brushable-block rules (`tests/oracle/scenarios/blk_fbc_{tnt,dragon_egg,respawn_anchor,beds}.txt`,
//! `blk_infra_explode_drops.txt`, `blk_dc_door_destroy.txt`, `blk_brushable_{fall,brush}.txt`) and of the cross-cutting rules
//! `blk_{comparator_item_frame,piston_block_entities,tripwire_entities,double_chest_loot_first_half,double_chest_loot_second_half,
//! workstation_stats,spawner_break,lectern_not_container,daylight_nether}` replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

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
    tnt => "blk_fbc_tnt",
    dragon_egg => "blk_fbc_dragon_egg",
    respawn_anchor => "blk_fbc_respawn_anchor",
    explode_drops => "blk_infra_explode_drops",
    door_destroy => "blk_dc_door_destroy",
    beds => "blk_fbc_beds",
    brushable_fall => "blk_brushable_fall",
    brushable_brush => "blk_brushable_brush",
    comparator_frame => "blk_comparator_item_frame",
    piston_block_entities => "blk_piston_block_entities",
    tripwire => "blk_tripwire_entities",
    double_chest_loot => "blk_double_chest_loot_first_half",
    double_chest_loot_second_half => "blk_double_chest_loot_second_half",
    workstation_stats => "blk_workstation_stats",
    spawner_break => "blk_spawner_break",
    lectern_hopper => "blk_lectern_not_container",
    daylight_nether => "blk_daylight_nether",
    entity_block_state_nbt => "entity_block_state_nbt",
);
