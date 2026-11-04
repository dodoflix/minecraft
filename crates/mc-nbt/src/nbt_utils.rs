//! The registry-independent parts of `net.minecraft.nbt.NbtUtils`.

use crate::{CompoundTag, Tag};

/// `NbtUtils.compareNbt(expected, actual, partialListMatches)`: `expected` is a pattern. A missing
/// `expected` always matches; compounds match when every expected entry matches; lists match
/// element-wise equal, or (with `partial_list_matches`) when each expected element matches some
/// actual element.
pub fn compare_nbt(expected: Option<&Tag>, actual: Option<&Tag>, partial_list_matches: bool) -> bool {
    let Some(expected) = expected else { return true };
    let Some(actual) = actual else { return false };
    match (expected, actual) {
        (Tag::Compound(e), Tag::Compound(a)) => {
            a.len() >= e.len() && e.iter().all(|(k, v)| compare_nbt(Some(v), a.get(k), partial_list_matches))
        }
        (Tag::List(e), Tag::List(a)) if partial_list_matches => {
            if e.is_empty() {
                a.is_empty()
            } else {
                a.len() >= e.len() && e.iter().all(|et| a.iter().any(|at| compare_nbt(Some(et), Some(at), partial_list_matches)))
            }
        }
        (e, a) => std::mem::discriminant(e) == std::mem::discriminant(a) && e == a,
    }
}

/// `NbtUtils.addDataVersion`.
pub fn add_data_version(tag: &mut CompoundTag, version: i32) {
    tag.put_int("DataVersion", version);
}

/// `NbtUtils.getDataVersion(tag)` (`-1` when absent).
pub fn get_data_version(tag: &CompoundTag) -> i32 {
    get_data_version_or(tag, -1)
}

/// `NbtUtils.getDataVersion(tag, default)`.
pub fn get_data_version_or(tag: &CompoundTag, default: i32) -> i32 {
    tag.get_int_or("DataVersion", default)
}
