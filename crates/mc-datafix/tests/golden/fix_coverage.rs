//! The coverage gate: every data fix of vanilla's chain changes the data of at least one committed sample, or is listed
//! with a reason in `fixtures/coverage_expected_unexercised.txt`.
//!
//! `fixtures/coverage.tsv` is written by `scripts/datafix-corpus.sh coverage` (vanilla's `DataFixers` with a watch around every
//! fix, run over every fixture of `fixtures/samples`; `tests/oracle/Coverage.java`): a row per fix, the number of real and
//! of synthetic samples the fix changed. The unexercised list only shrinks: a listed fix that a sample now exercises fails
//! here, and a fix nothing exercises that is not listed fails too. Every sample fixture is measured and has a replay test; the
//! live measurement against the committed file runs in the oracle target.

use crate::recorded_chain::{Row, parse};
use std::collections::BTreeMap;
use std::path::Path;

const COVERAGE: &str = include_str!("../fixtures/coverage.tsv");
const UNEXERCISED: &str = include_str!("../fixtures/coverage_expected_unexercised.txt");
const FIXES: &str = include_str!("../fixtures/fixes.tsv");

struct Measured {
    row: Row,
    real: u32,
    synth: u32,
}

impl Measured {
    fn exercised(&self) -> bool {
        self.real + self.synth > 0
    }
}

fn measured() -> Vec<Measured> {
    COVERAGE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 6, "malformed coverage row: {l}");
            Measured {
                row: Row {
                    index: f[0].parse().expect("index"),
                    version: f[1].parse().expect("version"),
                    sub: f[2].parse().expect("subversion"),
                    kind: "data".into(),
                    class: f[3].into(),
                },
                real: f[4].parse().expect("real count"),
                synth: f[5].parse().expect("synthetic count"),
            }
        })
        .collect()
}

/// The listed fixes: `INDEX VERSION SUBVERSION CLASS REASON`, tab separated, `#` comments.
fn listed() -> BTreeMap<usize, (Row, String)> {
    let mut out = BTreeMap::new();
    for l in UNEXERCISED
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = l.splitn(5, '\t').collect();
        assert_eq!(
            f.len(),
            5,
            "an entry is INDEX VERSION SUBVERSION CLASS REASON: {l}"
        );
        assert!(
            f[4].trim().len() >= 20,
            "the reason of {l} is missing or a stub"
        );
        let row = Row {
            index: f[0].parse().expect("index"),
            version: f[1].parse().expect("version"),
            sub: f[2].parse().expect("subversion"),
            kind: "data".into(),
            class: f[3].into(),
        };
        assert!(
            out.insert(row.index, (row, f[4].to_string())).is_none(),
            "fix {} is listed twice",
            f[0]
        );
    }
    out
}

#[test]
fn the_measured_chain_is_the_recorded_chain() {
    let recorded: Vec<Row> = parse(FIXES)
        .into_iter()
        .filter(|r| r.kind == "data")
        .collect();
    let rows: Vec<Row> = measured().into_iter().map(|m| m.row).collect();
    assert_eq!(
        rows, recorded,
        "coverage.tsv and fixes.tsv list other fixes: run scripts/datafix-corpus.sh coverage"
    );
}

#[test]
fn every_fix_is_exercised_or_listed_with_a_reason() {
    let listed = listed();
    let unlisted: Vec<String> = measured()
        .iter()
        .filter(|m| !m.exercised() && !listed.contains_key(&m.row.index))
        .map(|m| m.row.line())
        .collect();
    assert!(
        unlisted.is_empty(),
        "no committed sample exercises these fixes: add samples (scripts/datafix-corpus.sh, \
         fixtures/samples/record.sh), or list the fix with its reason in coverage_expected_unexercised.txt:\n{}",
        unlisted.join("\n")
    );
}

#[test]
fn a_listed_fix_that_a_sample_exercises_is_removed_from_the_list() {
    let listed = listed();
    let measured = measured();
    let by_index: BTreeMap<usize, &Measured> = measured.iter().map(|m| (m.row.index, m)).collect();
    let mut stale = Vec::new();
    for (index, (row, _)) in &listed {
        match by_index.get(index) {
            None => stale.push(format!("{} is not a fix of the chain", row.line())),
            Some(m) if m.row != *row => stale.push(format!(
                "{} is listed as another fix than the chain's {}",
                row.line(),
                m.row.line()
            )),
            Some(m) if m.exercised() => stale.push(format!(
                "{} is exercised ({} real, {} synthetic samples): delete it from the list",
                row.line(),
                m.real,
                m.synth
            )),
            Some(_) => {}
        }
    }
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}

/// Every fixture of `fixtures/samples` is measured (named in the header of `coverage.tsv`) and a golden test replays
/// it (its name appears in the sources of a crate's golden target).
#[test]
fn every_sample_fixture_is_measured_and_replayed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir.join("fixtures/samples"))
        .expect("the samples folder")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".tsv.gz"))
        .collect();
    on_disk.sort();
    let measured: Vec<(String, String)> = COVERAGE
        .lines()
        .filter_map(|l| l.strip_prefix("# fixture\t"))
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0].to_string(), f[1].to_string())
        })
        .collect();
    let names: Vec<String> = measured.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(
        on_disk, names,
        "the sample fixtures changed: run scripts/datafix-corpus.sh coverage"
    );
    for (name, sha) in &measured {
        let bytes = std::fs::read(dir.join("fixtures/samples").join(name)).expect("a fixture");
        assert_eq!(
            crate::compare::sha256_hex(&bytes),
            *sha,
            "{name} changed since it was measured: run scripts/datafix-corpus.sh coverage"
        );
    }
    // the fixtures are replayed by the golden targets of the crates that load them
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let sources: String = std::fs::read_dir(crates)
        .expect("the crates folder")
        .filter_map(|e| std::fs::read_dir(e.ok()?.path().join("tests/golden")).ok())
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.ok()?.path()).ok())
        .collect();
    let unreplayed: Vec<&String> = on_disk
        .iter()
        .filter(|n| !sources.contains(n.as_str()))
        .collect();
    assert!(
        unreplayed.is_empty(),
        "no golden test replays {unreplayed:?}"
    );
}
