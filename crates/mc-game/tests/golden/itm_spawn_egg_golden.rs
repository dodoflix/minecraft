//! Spawn egg scenario `tests/oracle/scenarios/itm_spawn_egg.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh itm_spawn_egg`) vs our ticking level.

use crate::common;

#[test]
fn spawn_eggs_set_spawners_and_spawn_mobs_and_mob_buckets_spawn_their_mob() {
    common::block_scenario::check("itm_spawn_egg");
}
