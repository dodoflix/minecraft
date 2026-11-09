//! Block scenarios of sculk (catalyst cursors, veins, shrieker) and the hatching blocks (turtle eggs, frogspawn, sniffer
//! eggs, infested blocks) (`tests/oracle/scenarios/blk_{sculk_spread,sculk_shrieker,eggs_hatch}.txt`) replayed on the real
//! vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).

use crate::common;
use crate::common::entity_oracle::Options;

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

/// The hatched sniffer is a placeholder entity (it neither falls on its first tick nor makes its sounds nor has health): only its spawn
/// position and rotation are compared.
#[test]
fn eggs_hatch() {
    common::block_scenario::check_with(
        "blk_eggs_hatch",
        &Options {
            ignore_fields: vec!["delta", "hp", "abs", "hurt", "death", "fx"],
            ignore_containing: vec!["event=minecraft:entity.sniffer."],
            ..Options::default()
        },
    );
}
