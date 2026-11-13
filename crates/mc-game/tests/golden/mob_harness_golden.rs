//! The mob verbs and dumps of the block replay (`common/mob_verbs.rs`, shared with the entity replay), replayed against the real
//! vanilla level (scenarios recorded with `scripts/block-oracle.sh`).

use crate::common;

/// A biome change, spawned mobs, love and breeding, a ticking block entity, a mounted horse jump and the `mob:`/`mobs`/`goals:`/`brain:`/
/// `attrs:`/`nbt:` dumps in one script on the ticking level.
#[test]
fn mobs_on_the_ticking_level_match_vanilla() {
    common::block_scenario::check("mob_harness_smoke");
}

/// `START_RIDING_JUMP`, `STOP_RIDING_JUMP` and `OPEN_INVENTORY` of the player command packet for the rider of a saddled horse, an
/// unsaddled one, a pig, an untamed horse and a player on foot.
#[test]
fn ride_actions_of_the_player_command_match_vanilla() {
    common::block_scenario::check("ent_pcommand_ride_verbs");
}
