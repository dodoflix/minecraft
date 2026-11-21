//! `java.util.HashMap` iteration order of vanilla's `CompoundTag` for the NBT-kind map operations of [`crate::Ops`].
//!
//! A `Dyn::Map` built by NBT ops lists its entries in the order a `CompoundTag` would iterate them. Every
//! `NbtOps` mutation starts from `CompoundTag.shallowCopy()` (`new HashMap<>(map)`: table sized from the entry count,
//! entries re-inserted in iteration order) and then `put`s/`remove`s; a fresh `createMap`/record builder is a
//! `new CompoundTag()` that grows organically.
//!
//! Without a bin of 9 or more keys only the *final* table capacity matters: resizes keep chain order, and `HashMap`
//! iterates buckets ascending with insertion order inside a bucket, so the result is the stable sort, by final bucket, of
//! "source iteration order + new keys appended" (replaced keys keep their slot, removed keys vanish). A bin of 9 keys
//! makes `HashMap` double a table under 64 buckets and turn the bin into a red-black tree in a bigger one, so then the
//! history matters: [`mc_core::hash_bins`] replays the `put`s and the `remove` on a model of the table, tree bins
//! included.

use crate::dynv::Dyn;
use mc_core::hash_bins::Bins;
use mc_core::java_collections::hash_map_copy_capacity;

type Entries = Vec<(Dyn, Dyn)>;

fn bucket_of_hash(h: i32, cap: usize) -> usize {
    ((h ^ ((h as u32) >> 16) as i32) as u32 as usize) & (cap - 1)
}

fn bucket(key: &Dyn, cap: usize) -> usize {
    bucket_of_hash(key_hash(key), cap)
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

/// The entries of `src` in the order `order` (indices of `src`) lists them; entries it leaves out are dropped.
fn reorder(src: Entries, order: &[usize]) -> Entries {
    let mut slots: Vec<Option<(Dyn, Dyn)>> = src.into_iter().map(Some).collect();
    order
        .iter()
        .map(|&i| slots[i].take().expect("each key is listed once"))
        .collect()
}

/// `String.hashCode()` of a key.
fn key_hash(key: &Dyn) -> i32 {
    match key {
        Dyn::Str(s) => s.java_hash_code(),
        _ => 0,
    }
}

/// [`replay`] of the keys of `entries` (the first `copied` of them a copied map's entries) and the removal of `removed`.
fn tree_order(entries: &Entries, copied: usize, removed: Option<&str>) -> Option<Vec<usize>> {
    if entries.len() < 9 {
        return None;
    }
    let keys: Vec<&Dyn> = entries.iter().map(|e| &e.0).collect();
    let removed = removed.and_then(|key| keys.iter().position(|k| k.is_str(key)));
    replay(&keys, copied, removed)
}

/// The keys (distinct, the first `copied` of them the entries of a map that `new HashMap<>(map)` copied, in its iteration
/// order; the others put one after the other) left in iteration order after removing key `removed`, replayed on the
/// table with its tree bins. `None` when no bin can reach 9 keys, where [`finish`] is exact.
fn replay(keys: &[&Dyn], copied: usize, removed: Option<usize>) -> Option<Vec<usize>> {
    // a bin of 9 keys in a later, larger table has 9 keys in the same bin of the first table
    let first_cap = if copied == 0 { 16 } else { copy_cap(copied) };
    let hashes: Vec<i32> = keys.iter().map(|k| key_hash(k)).collect();
    let mut bins = vec![0u32; first_cap];
    for &h in &hashes {
        bins[bucket_of_hash(h, first_cap)] += 1;
    }
    if bins.iter().all(|&n| n < 9) {
        return None;
    }
    let compare = |a: usize, b: usize| match (keys[a], keys[b]) {
        (Dyn::Str(a), Dyn::Str(b)) => a.java_cmp(b),
        _ => std::cmp::Ordering::Equal,
    };
    let mut map = Bins::new(hashes, &compare, if copied == 0 { 0 } else { first_cap });
    for key in 0..keys.len() {
        map.put(key);
    }
    if let Some(key) = removed {
        map.remove(key);
    }
    Some(map.order())
}

/// `new CompoundTag()` filled with `put`s of `entries` in order (duplicate keys replace).
pub(crate) fn organic(entries: impl IntoIterator<Item = (Dyn, Dyn)>) -> Entries {
    let mut out = Entries::new();
    let mut cap = 0;
    for (k, v) in entries {
        put(&mut out, &mut cap, k, v);
    }
    match tree_order(&out, 0, None) {
        Some(order) => reorder(out, &order),
        None => finish(out, cap),
    }
}

/// `shallowCopy()` of `src`, then `put` of every item, in iteration order.
pub(crate) fn copy_put(src: &[(Dyn, Dyn)], items: impl IntoIterator<Item = (Dyn, Dyn)>) -> Entries {
    let mut out = src.to_vec();
    let mut cap = copy_cap(out.len());
    for (k, v) in items {
        put(&mut out, &mut cap, k, v);
    }
    match tree_order(&out, src.len(), None) {
        Some(order) => reorder(out, &order),
        None => finish(out, cap),
    }
}

/// `shallowCopy()` of `src` then `remove(key)`.
pub(crate) fn copy_remove(src: &[(Dyn, Dyn)], key: &str) -> Entries {
    let mut out = src.to_vec();
    let cap = copy_cap(out.len());
    if let Some(order) = tree_order(&out, src.len(), Some(key)) {
        return reorder(out, &order);
    }
    out.retain(|(k, _)| !k.is_str(key));
    finish(out, cap)
}
