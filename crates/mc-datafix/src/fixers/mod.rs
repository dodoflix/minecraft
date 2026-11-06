//! `DataFixers.addFixers`, split by version range. Each `range_*` module mirrors the Java statements of its range in
//! order (schema registrations and `addFixer` calls).

use crate::fixer::DataFixerBuilder;

pub mod range_000_165;

/// `DataFixers.addFixers(fixerUpper, ..)`.
pub fn add_fixers(b: &mut DataFixerBuilder) {
    range_000_165::add(b);
}
