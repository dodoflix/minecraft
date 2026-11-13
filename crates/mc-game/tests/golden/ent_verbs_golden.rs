//! The entity verbs of the block replay (`tests/oracle/scenarios/ent_verbs.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn entity_verbs_match_vanilla() {
    common::block_scenario::check("ent_verbs");
}

#[test]
fn respawn_matches_vanilla() {
    // the spawn search starts at a thread-local random offset in vanilla: the position of the respawned player is not comparable
    common::block_scenario::check_with(
        "ent_respawn",
        &crate::common::entity_oracle::Options {
            ignore_fields: vec!["pos"],
            ..Default::default()
        },
    );
}
