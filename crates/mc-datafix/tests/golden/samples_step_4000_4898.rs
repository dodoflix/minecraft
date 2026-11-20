//! STEP samples of the fixes of data versions 4054..4892: schema-driven inputs (and hand written ones) recorded with
//! the real vanilla fixer (`fixtures/samples/record.sh step_4000_4898 12`), replayed through ours, and real chunks,
//! entity chunks, POI chunks, saved data and level files of the worlds saved by the 1.20.6 to 26.1 servers
//! (`fixtures/samples/step_real_4000_4898.tsv.gz`: the richest and a random chunk per world and dimension, every saved
//! data file, the level file whole and its `Data` compound; each updated from the stored version to 4898).

use crate::compare::report;
use crate::step_samples::{assert_replays, replay};

/// Samples whose outcome differs from vanilla's: vanilla fuses the fixes of one version and the recursion of a type
/// before it runs them (the optimising pass of `DataFixerUpper`, not ported). That changes the order of the effects of
/// one document (the order of the logged errors, and which of two failures throws) and, when a fix builds a nested
/// text component that a later fix of the same version would convert, whether the later fix reaches it: a legacy
/// `show_text` hover event whose `value` holds a component with string flags keeps them in vanilla.
const OPTIMISER_DIFFERENCES: [&str; 17] = [
    "g1-ref:player-4289-4290-3",
    "g1-ref:player-4289-4290-6",
    "g1-ref:player-4289-4290-7",
    "g1-ref:player-4289-4290-10",
    "g1-ref:player-4289-4290-11",
    "hand-text12-0073",
    "hand-text12-0074",
    "hand-text12-0075",
    "hand-text12-0076",
    "hand-text2-0048",
    "hand-text2-0081",
    "hand-text2-0191",
    "hand-text2-0225",
    "hand-text2-0269",
    "hand-text2-0284",
    "hand-text2-0325",
    "hand-text2-0399",
];

#[test]
fn every_step_of_the_1_21_2_to_1_21_11_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_4000_4898.tsv.gz");
    let (total, failures) = replay(&fixture);
    assert_eq!(total, 5675, "the fixture holds another number of samples");
    let ids: Vec<&str> = failures.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        OPTIMISER_DIFFERENCES,
        "{}",
        report(&failures, total, 4000)
    );
}

#[test]
fn real_worlds_of_the_1_20_6_to_26_1_servers_update_like_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_real_4000_4898.tsv.gz");
    assert_replays(&fixture, 264);
}
