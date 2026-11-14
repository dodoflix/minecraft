//! Goldens of `CarvedPumpkinBlock`, `PumpkinBlock`, `WitherSkullBlock`, `HugeMushroomBlock`, `ShelfMushroomBlock` and the
//! `SculkBlock` experience: scenarios `tests/oracle/scenarios/blk_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`).
//!
//! The wither has no behaviour yet (it is an inert placeholder), so for it only what the block code decides is compared:
//! the entity exists at the right position and rotation.
//! Not implemented: the wither entity behaviour (health, motion, sounds, effects); those fields are not compared.

use crate::common;
use crate::common::block_scenario::run_scenario;
use crate::common::entity_oracle::{Fixture, Options, compare};

/// `E wither#1 ..` reduced to name, type, `pos` and `rot`.
fn normalize(line: &str) -> Option<String> {
    let mut tokens = line.split(' ');
    match (tokens.next(), tokens.next()) {
        (Some("E"), Some(name)) if name.starts_with("wither#") => Some(
            line.split(' ')
                .filter(|t| !t.contains('=') || t.starts_with("pos=") || t.starts_with("rot="))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => Some(line.to_string()),
    }
}

fn check_placeholders(name: &str) {
    let normalize_all =
        |lines: &[String]| -> Vec<String> { lines.iter().filter_map(|l| normalize(l)).collect() };
    let expected = normalize_all(&Fixture::load(name).lines);
    let actual = normalize_all(&run_scenario(name));
    let opts = Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    };
    if let Err(report) = compare(&expected, &actual, &opts) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}

#[test]
fn pumpkin_golems_match_vanilla() {
    common::block_scenario::check("blk_pumpkin_golems");
}

#[test]
fn golem_copper_matches_vanilla() {
    common::block_scenario::check("blk_golem_copper");
}

#[test]
fn wither_pattern_matches_vanilla() {
    check_placeholders("blk_wither_pattern");
}

#[test]
fn mushroom_blocks_place_matches_vanilla() {
    common::block_scenario::check("blk_mushroom_blocks_place");
}

#[test]
fn sculk_break_xp_matches_vanilla() {
    common::block_scenario::check("blk_sculk_break_xp");
}
