//! STEP samples of the fixes of data versions 4996..5016 (`fixtures/samples/tail.tsv.gz`, recorded from vanilla's
//! `DataFixers` by `fixtures/samples/record.sh tail 8 0.1`): result tree, `NbtIo` bytes, error class and logged
//! error count must equal vanilla's.

use crate::step_samples::assert_replays;

#[test]
fn tail_fixes_equal_vanilla() {
    assert_replays(
        &mc_testkit::include_gz!("../fixtures/samples/tail.tsv.gz"),
        396,
    );
}
