//! Exact comparison: decoded content must be equal, key order included, and re-serialise to the same bytes.
//!
//! Differences are reported as `path: message` samples in a [`ChunkDiff`] (category `other`; the path is the point).
//! Nothing is normalised here: fields to leave out are removed beforehand (see `fields.rs`).

use std::collections::BTreeSet;

use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

use crate::compare::{Category, ChunkDiff, canon, short};
use crate::world::Store;

const OTHER: Category = Category::Other;

fn at(path: &str) -> &str {
    if path.is_empty() { "<root>" } else { path }
}

fn key_list<'a>(keys: impl Iterator<Item = &'a String>) -> String {
    let v: Vec<&str> = keys.take(9).map(String::as_str).collect();
    format!(
        "[{}{}]",
        v[..v.len().min(8)].join(","),
        if v.len() > 8 { ",.." } else { "" }
    )
}

/// Compares two decoded NBT roots: tags and key order (the order stored in the file).
pub fn diff_nbt(a: &CompoundTag, b: &CompoundTag, d: &mut ChunkDiff) {
    walk_compound(&mut String::new(), a, b, d);
}

/// [`diff_nbt`], then (`payload`: only when no field was removed) the decompressed bytes. The tag model is lossy
/// (`-0.0` loads as `0.0`, NaN payloads, duplicate keys, bytes after the root), so equal tags are not yet equal
/// serialisations.
pub fn diff_decoded(
    (a, pa): (&CompoundTag, &[u8]),
    (b, pb): (&CompoundTag, &[u8]),
    payload: bool,
    d: &mut ChunkDiff,
) {
    diff_nbt(a, b, d);
    if payload && d.is_empty() && pa != pb {
        let i = pa.iter().zip(pb).position(|(x, y)| x != y);
        let i = i.unwrap_or(pa.len().min(pb.len()));
        d.note(OTHER, || {
            format!(
                "<root>: equal tags, different payload bytes (lengths {} vs {}, first difference at byte {i})",
                pa.len(),
                pb.len()
            )
        });
    }
}

pub fn count(d: &ChunkDiff) -> u64 {
    d.cats.get(&OTHER).map_or(0, |c| c.count)
}

fn walk_compound(path: &mut String, x: &CompoundTag, y: &CompoundTag, d: &mut ChunkDiff) {
    for k in x.keys().filter(|k| !y.contains(k)) {
        d.note(OTHER, || format!("{}: key {k:?} only in A", at(path)));
    }
    for k in y.keys().filter(|k| !x.contains(k)) {
        d.note(OTHER, || format!("{}: key {k:?} only in B", at(path)));
    }
    let (ka, kb): (Vec<&String>, Vec<&String>) = (
        x.keys().filter(|k| y.contains(k)).collect(),
        y.keys().filter(|k| x.contains(k)).collect(),
    );
    if ka != kb {
        d.note(OTHER, || {
            format!(
                "{}: key order differs: A {} B {}",
                at(path),
                key_list(ka.iter().copied()),
                key_list(kb.iter().copied())
            )
        });
    }
    for (k, va) in x {
        if let Some(vb) = y.get(k) {
            let len = path.len();
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(k);
            walk(path, va, vb, d);
            path.truncate(len);
        }
    }
}

fn walk(path: &mut String, a: &Tag, b: &Tag, d: &mut ChunkDiff) {
    match (a, b) {
        (Tag::Compound(x), Tag::Compound(y)) => walk_compound(path, x, y, d),
        (Tag::List(x), Tag::List(y)) => {
            if x.len() != y.len() {
                d.note(OTHER, || {
                    format!("{}: list length {} vs {}", at(path), x.len(), y.len())
                });
            }
            for (i, (va, vb)) in x.iter().zip(y.iter()).enumerate() {
                let len = path.len();
                path.push_str(&format!("[{i}]"));
                walk(path, va, vb, d);
                path.truncate(len);
            }
        }
        _ if a == b => {}
        _ => d.note(OTHER, || format!("{}: {}", at(path), leaf_message(a, b))),
    }
}

fn first_diff<T: PartialEq>(a: &[T], b: &[T]) -> String {
    let i = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    format!(
        "arrays differ (lengths {} vs {}, first difference at index {i})",
        a.len(),
        b.len()
    )
}

fn leaf_message(a: &Tag, b: &Tag) -> String {
    match (a, b) {
        (Tag::ByteArray(x), Tag::ByteArray(y)) => first_diff(x, y),
        (Tag::IntArray(x), Tag::IntArray(y)) => first_diff(x, y),
        (Tag::LongArray(x), Tag::LongArray(y)) => first_diff(x, y),
        _ => format!("A {} B {}", short(canon(a)), short(canon(b))),
    }
}

/// Structural JSON comparison: object key order is not significant, array order is.
pub fn diff_json(a: &Value, b: &Value, d: &mut ChunkDiff) {
    json_walk(&mut String::new(), a, b, d);
}

fn json_walk(path: &mut String, a: &Value, b: &Value, d: &mut ChunkDiff) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                match (x.get(k), y.get(k)) {
                    (Some(va), Some(vb)) => json_walk(path, va, vb, d),
                    (Some(_), None) => d.note(OTHER, || format!("{}: only in A", at(path))),
                    _ => d.note(OTHER, || format!("{}: only in B", at(path))),
                }
                path.truncate(len);
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                d.note(OTHER, || {
                    format!("{}: array length {} vs {}", at(path), x.len(), y.len())
                });
            }
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                let len = path.len();
                path.push_str(&format!("[{i}]"));
                json_walk(path, va, vb, d);
                path.truncate(len);
            }
        }
        _ if a == b => {}
        _ => d.note(OTHER, || {
            format!(
                "{}: A {} B {}",
                at(path),
                short(a.to_string()),
                short(b.to_string())
            )
        }),
    }
}

/// One chunk slot of a region file pair.
pub enum Slot {
    OnlyA,
    OnlyB,
    Same,
    Differs(ChunkDiff),
    Error(String),
}

/// Compares every chunk of region `(rx, rz)` that lies in `chunks` (inclusive rectangle), calling `strip` on each
/// decoded chunk first. Slots empty on both sides are not reported.
pub(crate) fn compare_region(
    a: &mut Store,
    b: &mut Store,
    (rx, rz): (i32, i32),
    chunks: Option<(i32, i32, i32, i32)>,
    max_listed: usize,
    strip: &dyn Fn(&mut CompoundTag),
    payload: bool,
    mut emit: impl FnMut(ChunkPos, Slot),
) {
    for lz in 0..32 {
        for lx in 0..32 {
            let pos = ChunkPos::new(rx * 32 + lx, rz * 32 + lz);
            if let Some((x1, z1, x2, z2)) = chunks
                && (pos.x < x1.min(x2)
                    || pos.x > x1.max(x2)
                    || pos.z < z1.min(z2)
                    || pos.z > z1.max(z2))
            {
                continue;
            }
            let (ra, rb) = (a.read_payload(pos), b.read_payload(pos));
            // `read` also answers `None` for a record it cannot use (bad length or compression, missing external file)
            let dead = |r: &Result<Option<(CompoundTag, Vec<u8>)>, String>, s: &Store| {
                matches!(r, Ok(None)) && s.allocated(pos)
            };
            let (da, db) = (dead(&ra, a), dead(&rb, b));
            let slot = match (ra, rb) {
                _ if da || db => Slot::Error(format!(
                    "chunk record present but unreadable in {}",
                    if da && db {
                        "A and B"
                    } else if da {
                        "A"
                    } else {
                        "B"
                    }
                )),
                (Ok(None), Ok(None)) => continue,
                (Ok(Some(_)), Ok(None)) => Slot::OnlyA,
                (Ok(None), Ok(Some(_))) => Slot::OnlyB,
                (Ok(Some((mut x, px))), Ok(Some((mut y, py)))) => {
                    strip(&mut x);
                    strip(&mut y);
                    let mut d = ChunkDiff::new(max_listed);
                    diff_decoded((&x, &px), (&y, &py), payload, &mut d);
                    if d.is_empty() {
                        Slot::Same
                    } else {
                        Slot::Differs(d)
                    }
                }
                (x, y) => Slot::Error(
                    [x.err(), y.err()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
            };
            emit(pos, slot);
        }
    }
}
