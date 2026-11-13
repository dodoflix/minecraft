//! Item entities in the block replay: the NBT round trip, merging across chunk borders and against the stack cap, the packets of
//! dropping and picking up, lava, cactus, fire and despawn ages, who may pick an item up, the vibration an item dampens, and the item slot commands
//! (`tests/oracle/scenarios/ent_item_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;
use crate::common::entity_oracle::{Fixture, Options, compare};

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            common::block_scenario::check($scenario);
        }
    )+};
}

golden!(
    drop_packets => "ent_item_drop_packets",
    environment => "ent_item_environment",
    hopper => "ent_item_hopper",
    merge_chunk => "ent_item_merge_chunk",
    nbt => "ent_item_nbt",
    pickup_modes => "ent_item_pickup_modes",
    slot_cmd => "ent_item_slot_cmd",
    wool_vibration => "ent_item_wool_vibration",
);

/// The console feedback of the commands names an item entity by its item (`C` lines, which `check` leaves out).
#[test]
fn slot_cmd_feedback() {
    let feedback = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with("C "))
            .cloned()
            .collect()
    };
    let expected = Fixture::load("ent_item_slot_cmd");
    let actual = common::block_scenario::run_scenario("ent_item_slot_cmd");
    if let Err(report) = compare(
        &feedback(&expected.lines),
        &feedback(&actual),
        &Options::default(),
    ) {
        panic!("the command feedback differs from the fixture:\n{report}");
    }
}
