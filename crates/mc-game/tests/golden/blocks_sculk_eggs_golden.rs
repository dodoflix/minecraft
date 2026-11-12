//! Block scenarios of sculk (catalyst cursors, veins, shrieker) and the hatching blocks (turtle eggs, frogspawn, sniffer
//! eggs, infested blocks) (`tests/oracle/scenarios/blk_{sculk_spread,sculk_shrieker,eggs_hatch}.txt`) replayed on the real
//! vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

use crate::common;
use crate::common::block_scenario::run_scenario;
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
    sculk_spread => "blk_sculk_spread",
    sculk_shrieker => "blk_sculk_shrieker",
);

/// Only the hatched sniffer is a placeholder entity (it neither falls on its first tick nor makes its sounds nor has health): its row
/// is reduced to name, type, position and rotation on both sides and its sounds are dropped on both sides. Every other hatched mob
/// (turtles, tadpoles, silverfish) is compared with all its fields.
/// Not implemented: sniffer behaviour (motion, health, sounds).
fn normalize(line: &str) -> Option<String> {
    if line.contains("event=minecraft:entity.sniffer.") {
        return None;
    }
    let mut tokens = line.split(' ');
    match (tokens.next(), tokens.next()) {
        (Some("E"), Some(name)) if name.starts_with("sniffer#") => Some(
            line.split(' ')
                .filter(|t| !t.contains('=') || t.starts_with("pos=") || t.starts_with("rot="))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => Some(line.to_string()),
    }
}

#[test]
fn eggs_hatch() {
    let name = "blk_eggs_hatch";
    let normalize_all =
        |lines: &[String]| -> Vec<String> { lines.iter().filter_map(|l| normalize(l)).collect() };
    let expected = normalize_all(&Fixture::load(name).lines);
    let actual = normalize_all(&run_scenario(name));
    // `C` lines are the command feedback of the driver's `time` verb, which the replay does without a command
    let opts = Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    };
    if let Err(report) = compare(&expected, &actual, &opts) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}
