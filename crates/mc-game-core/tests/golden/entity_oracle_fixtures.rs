//! Sanity of the committed entity-oracle fixtures (vanilla output of `scripts/entity-oracle.sh`) and of the comparator in
//! `common/entity_oracle.rs`. Does not touch mc-game gameplay code.
use crate::common::entity_oracle::*;

#[test]
fn fixtures_are_well_formed() {
    for name in ["fall", "jump", "blk_swim_pool"] {
        let f = Fixture::load(name);
        let ticks = f.ticks();
        assert!(!ticks.is_empty(), "{name}: no ticks");
        for (i, w) in ticks.windows(2).enumerate() {
            assert_eq!(
                w[1].tick,
                w[0].tick + 1,
                "{name}: tick headers not consecutive at {i}"
            );
        }
        assert!(
            f.entity_lines("Steve")
                .iter()
                .all(|l| l.fields.contains_key("pos")),
            "{name}: P line without pos"
        );
    }
}

#[test]
fn fall_fixture_is_a_vanilla_fall_with_damage() {
    let steve = Fixture::load("fall").entity_lines("Steve");
    let first = &steve[0];
    assert_eq!(first.vec("pos"), vec![0.5, -40.0, 0.5]);
    let last = steve.last().unwrap();
    assert_eq!(last.vec("pos"), vec![0.5, -60.0, 0.5]);
    assert_eq!(last.str("og"), "1");
    assert!(last.f64("hp") < 20.0, "a 20 block fall hurts");
}

#[test]
fn comparator_is_token_exact_for_numbers_and_reports_diffs() {
    let o = Options::default();
    // Java and Rust spell the same double differently; the value is what counts
    assert!(lines_match(
        "P a pos=1.0E-5,2.0,-0.0",
        "P a pos=0.00001,2,-0",
        &o
    ));
    assert!(!lines_match(
        "P a pos=0.1",
        "P a pos=0.10000000000000002",
        &o
    ));
    assert!(lines_match(
        "P a pos=0.1",
        "P a pos=0.10000000000000002",
        &Options {
            float_tolerance: Some(1e-12),
            ..Default::default()
        }
    ));
    assert!(!lines_match("P a og=0", "P a og=1", &o));
    let err = compare(
        &["T 1".into(), "P a hp=20.0".into()],
        &["T 1".into(), "P a hp=19.0".into()],
        &o,
    )
    .unwrap_err();
    assert!(
        err.contains("exp     1: P a hp=20.0") && err.contains("act     1: P a hp=19.0"),
        "{err}"
    );
    let ign = Options {
        ignore_fields: vec!["fire"],
        ignore_kinds: vec!["C"],
        ..Default::default()
    };
    assert!(
        compare(
            &["P a fire=0 hp=1".into(), "C x".into()],
            &["P a fire=-20 hp=1".into()],
            &ign
        )
        .is_ok()
    );
}
