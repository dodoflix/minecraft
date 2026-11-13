//! Players tracking each other in the block replay (`tests/oracle/scenarios/ent_players_track.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn players_track_each_other_like_vanilla() {
    common::block_scenario::check("ent_players_track");
}
