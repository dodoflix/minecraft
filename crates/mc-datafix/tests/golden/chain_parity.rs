//! The registered fixes against the recorded chain of vanilla (`fixtures/fixes.tsv`): every fix we register is the next
//! one of the recorded chain with the same version and class, and the rows we do not register yet are exactly the
//! data rows of `fixtures/chain_expected_missing.tsv` (delete a row there in the commit that ports the fix; a listed row
//! we now register fails the test, so the list only shrinks). The registered file fixes are exactly the recorded file
//! rows.

use crate::recorded_chain::{Row, parse};
use mc_datafix::{data_fixer, get_file_fixer};
use std::collections::BTreeSet;

const RECORDED: &str = include_str!("../fixtures/fixes.tsv");
const EXPECTED_MISSING: &str = include_str!("../fixtures/chain_expected_missing.tsv");

#[test]
fn recorded_chain_has_vanillas_424_data_and_8_file_fixes() {
    let rows = parse(RECORDED);
    let data: Vec<&Row> = rows.iter().filter(|r| r.kind == "data").collect();
    let file: Vec<&Row> = rows.iter().filter(|r| r.kind == "file").collect();
    assert_eq!((data.len(), file.len()), (424, 8));
    for (i, r) in data.iter().enumerate() {
        assert_eq!(r.index, i, "data rows are numbered in registration order");
    }
    for (i, r) in file.iter().enumerate() {
        assert_eq!(r.index, i, "file rows are numbered in registration order");
    }
    assert!(
        data.windows(2).all(|w| w[0].version <= w[1].version),
        "vanilla registers its fixes in ascending version order"
    );
}

#[test]
fn registered_fixes_follow_the_recorded_chain() {
    let rows = parse(RECORDED);
    let data: Vec<&Row> = rows.iter().filter(|r| r.kind == "data").collect();
    let mut matched = BTreeSet::new();
    let mut next = 0;
    for fix in data_fixer().fixes() {
        let key = fix.version_key();
        let found = data[next..]
            .iter()
            .position(|r| {
                (r.version, r.sub, r.class.as_str()) == (key / 10, key % 10, fix.fix_name())
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} at version {} is not in the recorded chain after row {next} (extra, misnamed or out of order)",
                    fix.fix_name(),
                    key / 10
                )
            });
        matched.insert(next + found);
        next += found + 1;
    }
    let missing: BTreeSet<Row> = rows
        .iter()
        .filter(|r| r.kind == "data" && !matched.contains(&r.index))
        .cloned()
        .collect();
    let expected: BTreeSet<Row> = parse(EXPECTED_MISSING).into_iter().collect();
    let now_registered: Vec<String> = expected.difference(&missing).map(Row::line).collect();
    let unlisted: Vec<String> = missing.difference(&expected).map(Row::line).collect();
    assert!(
        now_registered.is_empty(),
        "registered now, remove from chain_expected_missing.tsv:\n{}",
        now_registered.join("\n")
    );
    assert!(
        unlisted.is_empty(),
        "not registered and not listed in chain_expected_missing.tsv:\n{}",
        unlisted.join("\n")
    );
}

#[test]
fn registered_file_fixes_equal_the_recorded_file_chain() {
    let rows = parse(RECORDED);
    let recorded: Vec<(i32, &str)> = rows
        .iter()
        .filter(|r| r.kind == "file")
        .map(|r| (r.version, r.class.as_str()))
        .collect();
    let registry = get_file_fixer();
    let registered: Vec<(i32, &str)> = registry
        .fixes()
        .iter()
        .map(|f| (f.version, f.name))
        .collect();
    assert_eq!(registered, recorded);
    assert_eq!(
        Some(registry.latest_file_fixer_version()),
        recorded.iter().map(|(v, _)| *v).max()
    );
}

/// A file fix shares its version with a data schema; the schemas not registered yet are the keys of
/// `fixtures/schema_types_expected_missing.txt`.
#[test]
fn every_file_fix_version_is_a_registered_data_schema_or_a_known_missing_one() {
    const MISSING_SCHEMAS: &str = include_str!("../fixtures/schema_types_expected_missing.txt");
    let missing: BTreeSet<i32> = MISSING_SCHEMAS
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.parse().expect("schema key"))
        .collect();
    let registered: BTreeSet<i32> = data_fixer().schema_keys().collect();
    for fix in get_file_fixer().fixes() {
        let key = fix.version * 10;
        assert_eq!(
            registered.contains(&key),
            !missing.contains(&key),
            "{} (schema {key}) must be a registered schema unless listed as not registered yet",
            fix.name
        );
    }
}

/// The oldest version the facade upgrades is derived from the registered schemas and fixes (vanilla gives every
/// schema but the first a fix): it equals the version above the newest fix that is not registered yet.
#[test]
fn the_oldest_supported_version_follows_the_fixes_not_registered_yet() {
    let newest_missing = parse(EXPECTED_MISSING)
        .iter()
        .filter(|r| r.kind == "data")
        .map(|r| (r.version * 10 + r.sub + 9) / 10)
        .max()
        .unwrap_or(i32::MIN);
    assert_eq!(data_fixer().oldest_supported_version(), newest_missing);
}
