//! STEP samples of the saved data types from data version 4899 to the current one, recorded from vanilla's `DataFixers`
//! (`fixtures/samples/record.sh saved_data 8`): schema-driven inputs of every `SAVED_DATA_*` type plus the saved data
//! files of the worlds saved by the 26.2 server (`saved_data.snbt`: the whole file compound, as `readTagFromDisk` hands
//! it to the fixer). Result tree, `NbtIo` bytes, error class and logged error count must equal vanilla's.

use crate::step_samples::assert_replays;

#[test]
fn saved_data_files_update_like_vanilla() {
    assert_replays(
        &mc_testkit::include_gz!("../fixtures/samples/saved_data.tsv.gz"),
        182,
    );
}
