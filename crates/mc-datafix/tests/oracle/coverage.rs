//! The committed coverage (`fixtures/coverage.tsv`) against the live measurement: vanilla's `DataFixers` with a watch around
//! every fix, over every committed sample fixture. Nightly: one pass over every committed sample.

use crate::oracle_client::{repo_root, run_verb};

#[test]
#[ignore = "oracle: runs every committed sample through vanilla's DataFixers in a JVM"]
fn coverage_matches_the_committed_measurement() {
    let samples = repo_root().join("crates/mc-datafix/tests/fixtures/samples");
    let live = run_verb(&[
        "coverage",
        "--samples",
        samples.to_str().expect("a UTF-8 path"),
    ]);
    let committed = include_str!("../fixtures/coverage.tsv");
    assert!(
        live == committed,
        "regenerate with scripts/datafix-corpus.sh coverage"
    );
}
