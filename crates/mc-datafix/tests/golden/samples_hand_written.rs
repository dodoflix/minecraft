//! STEP samples written by hand for the fixes no generated or world sample changes data of (`fixtures/samples/hand_written.snbt`,
//! recorded from vanilla's `DataFixers` by `fixtures/samples/record.sh hand_written 8`): item, block, biome, recipe, advancement
//! and game event renames, one input per rename map, and the field fixes whose input needs a particular shape. Result tree,
//! `NbtIo` bytes, error class and logged error count must equal vanilla's.

use crate::step_samples::assert_replays;

#[test]
fn hand_written_samples_equal_vanilla() {
    assert_replays(
        &mc_testkit::include_gz!("../fixtures/samples/hand_written.tsv.gz"),
        54,
    );
}
