//! Item frames and armor stand slots in the block replay: the map data of framed maps (markers, the updates sent to every
//! player, the tracked marker limit), the frame's single slot for `/item`, and the slots an armor stand can use for
//! `/item replace entity` and dispensers (`tests/oracle/scenarios/ent_frame_*.txt`, `ent_stand_*.txt`; fixtures regenerated
//! with `scripts/block-oracle.sh NAME`).

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
    frame_break_order => "ent_frame_break_order",
    frame_map => "ent_frame_map",
    frame_map_limit => "ent_frame_map_limit",
    frame_slot => "ent_frame_slot",
    stand_disabled_slots => "ent_stand_disabled_slots_cmd",
);

/// The console feedback of the commands (`C` lines, which `check` leaves out) names the frame and the stand.
#[test]
fn slot_commands_feedback() {
    let feedback = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with("C "))
            .cloned()
            .collect()
    };
    for scenario in ["ent_frame_slot", "ent_stand_disabled_slots_cmd"] {
        let expected = Fixture::load(scenario);
        let actual = common::block_scenario::run_scenario(scenario);
        if let Err(report) = compare(
            &feedback(&expected.lines),
            &feedback(&actual),
            &Options::default(),
        ) {
            panic!("{scenario}: the command feedback differs from the fixture:\n{report}");
        }
    }
}
