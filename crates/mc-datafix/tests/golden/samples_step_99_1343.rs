//! STEP samples of the fixes of data versions 99..1343 recorded with the real vanilla fixer and replayed through ours:
//! schema-driven and hand written inputs of every fix version (`fixtures/samples/record.sh step_99_1343 8`) and real
//! chunks and level files of worlds saved by the 1.7.10 to 1.12.2 servers (`fixtures/samples/step_real_99_1343.tsv.gz`: requests and records, chunks the richest in entities and block entities, a random chunk and level.dat per world; `-1` is the version of a world without a stored one).

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_legacy_band_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_99_1343.tsv.gz");
    assert_replays(&fixture, 3128);
}

#[test]
fn real_worlds_of_the_legacy_servers_update_like_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_real_99_1343.tsv.gz");
    assert_replays(&fixture, 47);
}
