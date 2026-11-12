//! Flint and steel and TNT dispensing against vanilla (`tests/oracle/scenarios/blk_disp_fire.txt`; fixture regenerated with
//! `scripts/block-oracle.sh blk_disp_fire`).

use crate::blocks_spawners_golden::with_sorted_attributes;
use crate::common::block_scenario::run_scenario;
use crate::common::entity_oracle::{Fixture, Options, compare};

/// Compares with the `attributes` lists of `nbt:` dumps sorted (vanilla saves them in identity hash order, which changes
/// between JVM runs).
pub(crate) fn check(name: &str) {
    let normalize = |lines: &[String]| -> Vec<String> {
        lines.iter().map(|l| with_sorted_attributes(l)).collect()
    };
    let options = Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    };
    if let Err(report) = compare(
        &normalize(&Fixture::load(name).lines),
        &normalize(&run_scenario(name)),
        &options,
    ) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}

#[test]
fn flint_and_steel_and_tnt() {
    check("blk_disp_fire");
}
