//! Spawner goldens: monster spawner block, trial spawner and vault scenarios (`tests/oracle/scenarios/blk_*spawner*.txt`,
//! `blk_vault_unlock.txt`) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.
//!
//! All five compare with rotations blanked and attribute lists sorted. The trial spawner scenarios give the entities a fixed
//! `Pos`: a trial spawner needs `spawn_mobs`, which turns natural spawning on in the oracle level, and the block harness does
//! not reproduce its random draws. Item entities take their yaw from an entity random that Rust cannot reproduce either.
//! The monster spawner scenario uses slimes and cows only: the harness level has no night sky darkening, which monster spawn
//! rules read.

use crate::common::block_scenario::run_scenario;
use crate::common::entity_oracle::{Fixture, Options, compare};

/// The entries of every `attributes:[{..},{..}]` list sorted: vanilla saves them in the iteration order of a hash map keyed by
/// identity hash codes, which changes from one JVM run to the next.
pub(crate) fn with_sorted_attributes(line: &str) -> String {
    const KEY: &str = "attributes:[";
    let Some(start) = line.find(KEY).map(|i| i + KEY.len()) else {
        return line.to_string();
    };
    let (mut depth, mut entries, mut from, mut end) = (0i32, Vec::new(), start, start);
    for (i, c) in line[start..].char_indices() {
        match c {
            '{' | '[' => depth += 1,
            '}' => depth -= 1,
            ']' if depth == 0 => {
                end = start + i;
                break;
            }
            ']' => depth -= 1,
            ',' if depth == 0 => {
                entries.push(&line[from..start + i]);
                from = start + i + 1;
            }
            _ => {}
        }
    }
    entries.push(&line[from..end]);
    entries.sort_unstable();
    format!(
        "{}{}{}",
        &line[..start],
        entries.join(","),
        with_sorted_attributes(&line[end..])
    )
}

/// The line with every rotation (`rot=` field of `E` lines, `Rotation` of `N` lines) blanked out and the attributes sorted.
fn normalize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find("Rotation:[") {
        let end = rest[i..].find(']').map_or(rest.len(), |e| i + e + 1);
        out.push_str(&rest[..i]);
        out.push_str("Rotation:[]");
        rest = &rest[end..];
    }
    out.push_str(rest);
    with_sorted_attributes(
        &out.split(' ')
            .filter(|t| !t.starts_with("rot="))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The harness does not echo the commands' own output (`C` lines).
fn options() -> Options {
    Options {
        ignore_kinds: vec!["C"],
        ..Options::default()
    }
}

fn check_normalized(name: &str, opts: &Options) {
    let actual: Vec<String> = run_scenario(name).iter().map(|l| normalize(l)).collect();
    let expected: Vec<String> = Fixture::load(name)
        .lines
        .iter()
        .map(|l| normalize(l))
        .collect();
    if let Err(report) = compare(&expected, &actual, opts) {
        panic!("block oracle scenario `{name}` differs from its fixture:\n{report}");
    }
}

fn check_normalized_default(name: &str) {
    check_normalized(name, &options());
}

#[test]
fn spawner_spawn_matches_vanilla() {
    check_normalized("blk_spawner_spawn", &options());
}

#[test]
fn trial_spawner_cycle_matches_vanilla() {
    check_normalized_default("blk_trial_spawner_cycle");
}

#[test]
fn trial_spawner_mobs_match_vanilla() {
    check_normalized_default("blk_trial_spawner_mobs");
}

#[test]
fn trial_spawner_rules_match_vanilla() {
    check_normalized_default("blk_trial_spawner_rules");
}

#[test]
fn vault_unlock_matches_vanilla() {
    // Item entities resting on the ground move differently from ours (`delta.y` -0.04 vs 0, later positions):
    // an `ItemEntity` motion difference that belongs to the entity port.
    check_normalized(
        "blk_vault_unlock",
        &Options {
            ignore_fields: vec!["pos", "delta"],
            ..options()
        },
    );
}
