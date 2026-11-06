//! `DataFixers.addFixers`, split by version range. Not implemented: no range registers a schema or a fix yet, so the
//! chain is empty.

use crate::fixer::DataFixerBuilder;

pub mod range_000_165;

/// `DataFixers.addFixers(fixerUpper, ..)`.
pub fn add_fixers(b: &mut DataFixerBuilder) {
    range_000_165::add(b);
}
