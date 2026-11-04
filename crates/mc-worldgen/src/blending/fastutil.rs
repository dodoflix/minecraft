//! Iteration order of fastutil 8.5 `Long2ObjectOpenHashMap` (default constructor, load factor 0.75, insert-only).
//!
//! `Blender` sums doubles over the entries of such maps and breaks distance ties by "first seen", so the order in
//! which `forEach` visits the entries is observable; the keys are simulated through the same table layout
//! (`HashCommon.mix`, linear probing, growth by `arraySize(size + 1, f)`) to reproduce it.

const LONG_PHI: i64 = 0x9E37_79B9_7F4A_7C15u64 as i64;
const DEFAULT_INITIAL_SIZE: usize = 16;

/// `HashCommon.mix(long)` truncated to `int`.
fn mix(x: i64) -> i32 {
    let mut h = x.wrapping_mul(LONG_PHI);
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

/// `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    (((expected as f64) / 0.75).ceil() as u64).next_power_of_two().max(2) as usize
}

/// `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    (((n as f64) * 0.75).ceil() as usize).min(n - 1)
}

/// Keys of a map that received `keys` (distinct) via `put`, in `forEach` order: the zero key first, then the table
/// from its last slot down to the first.
pub fn iteration_order(keys: &[i64]) -> Vec<i64> {
    let mut n = array_size(DEFAULT_INITIAL_SIZE);
    let mut table = vec![0i64; n];
    let mut fill = max_fill(n);
    let mut contains_null_key = false;
    let mut size = 0usize;
    for &k in keys {
        if k == 0 {
            if contains_null_key {
                continue;
            }
            contains_null_key = true;
        } else {
            let mask = n - 1;
            let mut pos = mix(k) as usize & mask;
            let mut present = false;
            while table[pos] != 0 {
                if table[pos] == k {
                    present = true;
                    break;
                }
                pos = (pos + 1) & mask;
            }
            if present {
                continue;
            }
            table[pos] = k;
        }
        let old = size;
        size += 1;
        if old >= fill {
            let new_n = array_size(size + 1);
            let mask = new_n - 1;
            let mut new_table = vec![0i64; new_n];
            for i in (0..n).rev() {
                let key = table[i];
                if key != 0 {
                    let mut pos = mix(key) as usize & mask;
                    while new_table[pos] != 0 {
                        pos = (pos + 1) & mask;
                    }
                    new_table[pos] = key;
                }
            }
            table = new_table;
            n = new_n;
            fill = max_fill(n);
        }
    }
    let mut order = Vec::with_capacity(size);
    if contains_null_key {
        order.push(0);
    }
    order.extend(table.iter().rev().copied().filter(|&k| k != 0));
    order
}
