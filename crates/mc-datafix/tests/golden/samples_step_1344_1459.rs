//! STEP samples of the fixes of data versions 1344..1459, the flattening: schema-driven inputs (and hand written ones:
//! whole chunks of legacy sections with their block entities, entities, items, options, stats, flat generator
//! settings) recorded with the real vanilla fixer (`fixtures/samples/record.sh step_1344_1459 10`), replayed through ours,
//! plus real chunks and level files of worlds saved by the 1.7.10 to 1.12.2 servers
//! (`fixtures/samples/step_real_1344_1459.tsv.gz`; the 1.13.2 world's files are fed to the 1343 schema as hostile input).

use crate::step_samples::assert_replays;

#[test]
fn every_step_of_the_flattening_matches_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_1344_1459.tsv.gz");
    assert_replays(&fixture, 8355);
}

#[test]
fn real_worlds_of_the_legacy_servers_update_to_the_flattened_format_like_vanilla() {
    let fixture = mc_testkit::include_gz!("../fixtures/samples/step_real_1344_1459.tsv.gz");
    assert_replays(&fixture, 68);
}
