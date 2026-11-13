//! Armor stands, item frames, paintings, cushions and mannequins in the block replay (`tests/oracle/scenarios/ent_stand_*.txt`,
//! `ent_frame_*.txt`, `ent_hanging_*.txt`, `ent_painting_*.txt`, `ent_cushion_*.txt`, `ent_mannequin_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`): equipment swaps, damage and breaking, drops, the observer's packets.

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
    decoration_projectiles => "ent_decoration_projectiles",
    decoration_riding => "ent_decoration_riding",
    frame_use => "ent_frame_use",
    hanging_survive => "ent_hanging_survive",
    mannequin => "ent_mannequin",
    painting_and_cushion => "ent_painting_cushion",
    stand_equip => "ent_stand_equip",
    stand_hurt => "ent_stand_hurt",
    stand_poses => "ent_stand_poses",
);

/// The console feedback of the commands (`C` lines, which `check` leaves out): the box probes of `ent_stand_poses` and the
/// profile a mannequin reports.
#[test]
fn command_feedback() {
    let feedback = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with("C "))
            .cloned()
            .collect()
    };
    for scenario in ["ent_stand_poses", "ent_mannequin"] {
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
