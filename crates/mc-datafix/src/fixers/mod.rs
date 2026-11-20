//! `DataFixers.addFixers`, split by version range; the ranges run in ascending order, so the registration order is the
//! Java one. Each range registers the schemas and fixes of its versions; a range not ported yet registers nothing.

use crate::fixer::DataFixerBuilder;

pub mod file_fixes;
pub mod range_000_165;
pub mod range_1344_1459;
pub mod range_1460_2199;
pub mod range_166_1343;
pub mod range_2200_2699;
pub mod range_2700_3299;
pub mod range_3300_3999;
pub mod range_4000_4649;
pub mod range_4650_5023;

/// `DataFixers.addFixers(fixerUpper, ..)`.
pub fn add_fixers(b: &mut DataFixerBuilder) {
    range_000_165::add(b);
    range_166_1343::add(b);
    range_1344_1459::add(b);
    range_1460_2199::add(b);
    range_2200_2699::add(b);
    range_2700_3299::add(b);
    range_3300_3999::add(b);
    range_4000_4649::add(b);
    range_4650_5023::add(b);
    file_fixes::add(b);
}
