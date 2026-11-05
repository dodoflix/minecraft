//! `java.util.HashMap` iteration order of vanilla's `CompoundTag` for the NBT-kind map operations of [`crate::Ops`].
//!
//! A `Dyn::Map` built by NBT ops lists its entries in the order a `CompoundTag` would iterate them. Every
//! `NbtOps` mutation starts from `CompoundTag.shallowCopy()` (`new HashMap<>(map)`: table sized from the entry count,
//! entries re-inserted in iteration order) and then `put`s/`remove`s; a fresh `createMap`/record builder is a
//! `new CompoundTag()` that grows organically.
//!
//! Only the *final* table capacity matters: resizes keep chain order, and `HashMap` iterates buckets ascending with
//! insertion order inside a bucket, so the result is the stable sort, by final bucket, of "source iteration order +
//! new keys appended" (replaced keys keep their slot, removed keys vanish).
//!
//! shortcut: bins with 9+ colliding keys that Java treeifies (table >= 64) iterate differently; not modelled.

use crate::dynv::Dyn;
use mc_core::java_collections::hash_map_copy_capacity;
use mc_core::math::java::java_string_hash;

type Entries = Vec<(Dyn, Dyn)>;

fn bucket(key: &Dyn, cap: usize) -> usize {
    let h = match key {
        Dyn::Str(s) => java_string_hash(s),
        _ => 0,
    };
    ((h ^ ((h as u32) >> 16) as i32) as u32 as usize) & (cap - 1)
}

/// Table capacity of `new HashMap<>(map)` for `n` entries (0 = table not allocated yet).
pub(crate) fn copy_cap(n: usize) -> usize {
    if n == 0 { 0 } else { hash_map_copy_capacity(n) }
}

/// `HashMap.put(key, value)`: replace in place or append, growing `cap` like `resize()`.
pub(crate) fn put(entries: &mut Entries, cap: &mut usize, key: Dyn, value: Dyn) {
    if let Some(e) = entries.iter_mut().find(|(k, _)| *k == key) {
        e.1 = value;
        return;
    }
    if *cap == 0 {
        *cap = 16;
    }
    entries.push((key, value));
    if entries.len() > *cap * 3 / 4 {
        *cap *= 2;
    }
}

/// The entries in iteration order of a table of capacity `cap`.
pub(crate) fn finish(mut entries: Entries, cap: usize) -> Entries {
    if cap != 0 && entries.len() > 1 {
        entries.sort_by_cached_key(|(k, _)| bucket(k, cap));
    }
    entries
}

/// `new CompoundTag()` filled with `put`s of `entries` in order (duplicate keys replace).
pub(crate) fn organic(entries: impl IntoIterator<Item = (Dyn, Dyn)>) -> Entries {
    let mut out = Entries::new();
    let mut cap = 0;
    for (k, v) in entries {
        put(&mut out, &mut cap, k, v);
    }
    finish(out, cap)
}

/// `shallowCopy()` of `src`, then `put` of every item, in iteration order.
pub(crate) fn copy_put(src: &[(Dyn, Dyn)], items: impl IntoIterator<Item = (Dyn, Dyn)>) -> Entries {
    let mut out = src.to_vec();
    let mut cap = copy_cap(out.len());
    for (k, v) in items {
        put(&mut out, &mut cap, k, v);
    }
    finish(out, cap)
}

/// `shallowCopy()` of `src` then `remove(key)`.
pub(crate) fn copy_remove(src: &[(Dyn, Dyn)], key: &str) -> Entries {
    let mut out = src.to_vec();
    let cap = copy_cap(out.len());
    out.retain(|(k, _)| k.as_str() != Some(key));
    finish(out, cap)
}
