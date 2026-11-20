//! Unpaired UTF-16 surrogates in strings and compound keys through every fix. Result tree, `NbtIo` bytes, exception text
//! and the logged errors (count and text, a lone surrogate encodes as `?`) must equal vanilla's, so a rename or rewrite keeps
//! a lone surrogate exactly where Java does.

use crate::step_samples::assert_replays;

/// One schema-driven sample per (fix version, type) with half of the strings and keys poisoned
/// (`fixtures/samples/step_lone_surrogate.tsv.gz`, recorded by `fixtures/samples/lone_surrogate.sh`).
#[test]
fn lone_surrogates_pass_every_fix_like_vanilla() {
    assert_replays(
        &mc_testkit::include_gz!("../fixtures/samples/step_lone_surrogate.tsv.gz"),
        12664,
    );
}

/// The strings the rewriting fixes write back (renames, `replace`, `toLowerCase`, flat generator strings, stat keys,
/// item predicates; `fixtures/samples/lone_surrogate_hand_written.snbt`, recorded by `record.sh lone_surrogate_hand_written 1`).
#[test]
fn hand_written_lone_surrogates_pass_the_rewriting_fixes_like_vanilla() {
    assert_replays(
        &mc_testkit::include_gz!("../fixtures/samples/lone_surrogate_hand_written.tsv.gz"),
        40,
    );
}
