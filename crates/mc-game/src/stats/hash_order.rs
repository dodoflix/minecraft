//! Iteration orders vanilla's statistics code exposes: `Object2IntOpenHashMap` (the counter itself and the
//! `ClientboundAwardStatsPacket` map) and `java.util.HashMap` (`Util.toMap()` groups, dirty set) keyed by
//! `Stat.hashCode()` = `getName().hashCode()`.
use mc_core::math::java::java_string_hash;

/// `HashCommon.mix(int)`.
fn mix(x: i32) -> i32 {
    let h = x.wrapping_mul(0x9E37_79B9_u32 as i32);
    h ^ ((h as u32) >> 16) as i32
}

/// fastutil `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    ((expected as f64 / 0.75).ceil() as usize).next_power_of_two().max(2)
}

/// fastutil `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    ((n as f64 * 0.75).ceil() as usize).min(n - 1)
}

/// Iteration order (indices into `hashes`) of an `Object2IntOpenHashMap` built by `put`ting keys with these hash
/// codes in this order (distinct keys). `expected` is the constructor argument (`16` for `new Object2IntOpenHashMap()`).
pub fn object_open_hash_order(hashes: &[i32], expected: usize) -> Vec<usize> {
    let mut n = array_size(expected);
    let mut max = max_fill(n);
    let mut table: Vec<Option<usize>> = vec![None; n];
    let place = |table: &mut Vec<Option<usize>>, mask: usize, idx: usize| {
        let mut pos = (mix(hashes[idx]) as u32 as usize) & mask;
        while table[pos].is_some() {
            pos = (pos + 1) & mask;
        }
        table[pos] = Some(idx);
    };
    let mut size = 0;
    for idx in 0..hashes.len() {
        place(&mut table, n - 1, idx);
        let grow = size >= max;
        size += 1;
        if grow {
            let new_n = array_size(size + 1);
            let mut new_table = vec![None; new_n];
            // rehash walks the old table from the top
            for slot in table.iter().rev().flatten() {
                place(&mut new_table, new_n - 1, *slot);
            }
            table = new_table;
            n = new_n;
            max = max_fill(n);
        }
    }
    table.iter().rev().flatten().copied().collect()
}

/// Iteration order (indices into `names`) of a `java.util.HashMap` with these keys inserted in this order
/// (`initial_capacity`: table size of the map before the first insert, 16 for `new HashMap<>()`).
pub fn java_hash_map_order(names: &[String], initial_capacity: usize) -> Vec<usize> {
    let mut cap = initial_capacity.max(1).next_power_of_two();
    let mut bins: Vec<Vec<(usize, i32)>> = vec![Vec::new(); cap];
    let spread = |s: &str| {
        let h = java_string_hash(s);
        h ^ ((h as u32) >> 16) as i32
    };
    let mut size = 0;
    for (idx, name) in names.iter().enumerate() {
        let h = spread(name);
        bins[(h as u32 as usize) & (cap - 1)].push((idx, h));
        size += 1;
        if size > cap / 4 * 3 {
            cap *= 2;
            let old = std::mem::take(&mut bins);
            bins = vec![Vec::new(); cap];
            for (i, h) in old.into_iter().flatten() {
                bins[(h as u32 as usize) & (cap - 1)].push((i, h));
            }
        }
    }
    bins.into_iter().flatten().map(|(i, _)| i).collect()
}
