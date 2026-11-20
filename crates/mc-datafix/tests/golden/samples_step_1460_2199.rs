//! STEP samples of the fixes of data versions 1460..2199 (1.13 to 1.15): schema-driven inputs and hand written ones
//! (whole chunks with leaves, trapped chests, structure pieces, villages and POI sections, generator option strings,
//! advancements, villager and cat entities, lore and banner names, options) recorded with the real vanilla fixer
//! (`fixtures/samples/record.sh step_1460_2199 8`), replayed through ours, plus real chunks, level files and saved data of
//! worlds saved by the 1.12.2 to 1.14.4 servers (`fixtures/samples/step_real_1460_2199.tsv.gz`).

use crate::compare::{Mismatch, report};
use crate::step_samples::{assert_replays, replay};

/// Chunks with two sections of one `Y`: `LeavesFix` fails in `Collectors.toMap`, whose message names both
/// `LeavesSection` objects by identity hash code (`LeavesFix$LeavesSection@6c2e9c38`), a text no port can reproduce.
/// Both sides throw `IllegalStateException`; only the message hash differs.
const DUPLICATE_SECTION_SAMPLES: [&str; 5] = [
    "g1-ref:chunk-1495-1496-2",
    "g1-ref:chunk-1495-1496-7",
    "g1-ref:chunk-1459-2199-3",
    "g1-ref:chunk-1459-2199-4",
    "g1-ref:chunk-1459-2199-5",
];

#[test]
fn every_step_of_the_1_13_to_1_15_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_1460_2199.tsv.gz");
    let (total, failures) = replay(&fixture);
    assert_eq!(total, 3194, "the fixture holds another number of samples");
    let (identity, other): (Vec<_>, Vec<_>) = failures.into_iter().partition(|(id, m)| {
        matches!(m, Mismatch::ExceptionMessage { .. })
            && DUPLICATE_SECTION_SAMPLES.contains(&id.as_str())
    });
    assert!(other.is_empty(), "{}", report(&other, total, 4000));
    let ids: Vec<&str> = identity.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids, DUPLICATE_SECTION_SAMPLES,
        "the samples with an identity hash in the vanilla message"
    );
}

#[test]
fn real_worlds_of_the_1_12_to_1_14_servers_update_to_1_15_like_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_real_1460_2199.tsv.gz");
    assert_replays(&fixture, 76);
}
