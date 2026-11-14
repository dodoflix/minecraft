//! The creaking against the vanilla oracle (scenarios in `oracle/scenarios`): freezing while looked at, the link to its heart and
//! the ways a heart's creaking goes away.
use crate::common::block_scenario;
use crate::common::entity_oracle::Options;

/// A pumpkin given to a player plays the equip sound in vanilla and stays silent in the replay.
#[test]
fn creaking_freezes_while_looked_at() {
    block_scenario::check_with(
        "mob_creaking_freeze",
        &Options {
            ignore_containing: vec!["event=minecraft:item.armor.equip_"],
            ..Default::default()
        },
    );
}

#[test]
fn creaking_heart_spawns_and_protects_its_creaking() {
    block_scenario::check("mob_creaking_heart_link");
}

#[test]
fn creaking_tears_down_when_its_heart_goes() {
    block_scenario::check("mob_creaking_transform");
}
