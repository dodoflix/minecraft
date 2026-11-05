//! `DataFixers.addFixers`, split by version range. Each `range_*` module mirrors the Java statements of its range in
//! order (schema registrations and `addFixer` calls); fixes not ported yet are marked with
//! `b.unported(&schema, "ClassName")` in place of the `addFixer` line so the differential harness skips the step.

use crate::fixer::DataFixerBuilder;

pub mod range_000_165;

/// `DataFixers.addFixers(fixerUpper, ..)`.
pub fn add_fixers(b: &mut DataFixerBuilder) {
    range_000_165::add(b);
}
