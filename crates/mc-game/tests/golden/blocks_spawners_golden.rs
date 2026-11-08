//! Spawner goldens: monster spawner block, trial spawner and vault scenarios (`tests/oracle/scenarios/blk_*spawner*.txt`,
//! `blk_vault_unlock.txt`) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.
//!
//! The trial spawner and vault scenarios give the entities a fixed `Pos` and compare without rotations: a trial spawner needs
//! `spawn_mobs`, which turns natural spawning on in the oracle level, and the block harness does not reproduce its random draws
//! (the level random is out of step between the two servers from then on). Item entities take their yaw from an entity random
//! that Rust cannot reproduce either.

use crate::common::block_scenario::{check_with, run_scenario};
use crate::common::entity_oracle::{Fixture, Options, compare};

/// The line with every rotation (`rot=` field of `E` lines, `Rotation` of `N` lines) blanked out.
fn without_rotation(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find("Rotation:[") {
        let end = rest[i..].find(']').map_or(rest.len(), |e| i + e + 1);
        out.push_str(&rest[..i]);
        out.push_str("Rotation:[]");
        rest = &rest[end..];
    }
    out.push_str(rest);
    out.split(' ')
        .filter(|t| !t.starts_with("rot="))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The harness does not echo the commands' own output (`C` lines).
fn options() -> Options {
    Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    }
}

fn check_without_rotation(name: &str) {
    let actual: Vec<String> = run_scenario(name)
        .iter()
        .map(|l| without_rotation(l))
        .collect();
    let expected: Vec<String> = Fixture::load(name)
        .lines
        .iter()
        .map(|l| without_rotation(l))
        .collect();
    if let Err(report) = compare(&expected, &actual, &options()) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}

#[test]
fn spawner_spawn_matches_vanilla() {
    check_with("blk_spawner_spawn", &options());
}

#[test]
fn trial_spawner_cycle_matches_vanilla() {
    check_without_rotation("blk_trial_spawner_cycle");
}

#[test]
fn trial_spawner_mobs_match_vanilla() {
    check_without_rotation("blk_trial_spawner_mobs");
}

#[test]
fn trial_spawner_rules_match_vanilla() {
    check_without_rotation("blk_trial_spawner_rules");
}

#[test]
fn vault_unlock_matches_vanilla() {
    check_without_rotation("blk_vault_unlock");
}
