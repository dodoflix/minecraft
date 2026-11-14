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

#[test]
fn eggs_hatch() {
    let name = "blk_eggs_hatch";
    let expected = Fixture::load(name).lines;
    let actual = run_scenario(name);
    // `C` lines are the command feedback of the driver's `time` verb, which the replay does without a command
    let opts = Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    };
    if let Err(report) = compare(&expected, &actual, &opts) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}
