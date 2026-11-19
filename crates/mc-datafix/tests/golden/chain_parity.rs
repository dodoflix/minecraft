//! The registered fixes against the recorded chain of vanilla (`fixtures/fixes.tsv`): every fix we register is the next
//! one of the recorded chain with the same version and class, and the rows we do not register yet are exactly the
//! rows of `fixtures/chain_expected_missing.tsv` (delete a row there in the commit that ports the fix; a listed row we
//! now register fails the test, so the list only shrinks).

use crate::recorded_chain::{Row, parse};
use mc_datafix::data_fixer;
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
        .filter(|r| r.kind == "file" || !matched.contains(&r.index))
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
