//! `--tree`: compares two world directories file by file.
//!
//! The file set must match (empty directories count). Equal bytes are equal. Otherwise `.dat`/`.dat_old`/`.nbt` files
//! (gzip or raw NBT) are decoded and compared exactly, `.mca` files chunk by chunk with the same exact comparison
//! (region headers carry timestamps and a sector layout, so their bytes are not compared), `.json` files structurally
//! (object key order is not significant), every other file by size and sha256. Left out: whatever `--ignore-field` names.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use mc_nbt::{CompoundTag, NbtAccounter};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::compare::{Category, ChunkDiff};
use crate::exact::{Slot, compare_region, count, diff_decoded, diff_json};
use crate::fields::Ignores;
use crate::world::{Store, js, region_coords};

pub struct TreeOptions {
    pub ignores: Ignores,
    /// Inclusive chunk rectangle `(x1, z1, x2, z2)` applied to every `.mca` file.
    pub chunks: Option<(i32, i32, i32, i32)>,
    /// Samples listed per differing file.
    pub max_listed: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    OnlyA,
    OnlyB,
    Differs,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::OnlyA => "only in A",
            Kind::OnlyB => "only in B",
            Kind::Differs => "differs",
        }
    }
}

#[derive(Debug)]
pub struct TreeEntry {
    /// Path relative to the world directory; a directory ends in `/`.
    pub path: String,
    pub kind: Kind,
    pub count: u64,
    pub samples: Vec<String>,
}

#[derive(Default, Debug)]
pub struct TreeReport {
    pub files_a: u64,
    pub files_b: u64,
    pub identical: u64,
    pub entries: Vec<TreeEntry>,
    pub errors: Vec<String>,
}

impl TreeReport {
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty() && self.errors.is_empty()
    }

    fn differing(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == Kind::Differs)
            .count()
    }
}

/// Files and empty directories below `dir`, keyed by their relative path; a directory with nothing left after the
/// ignores counts as empty. Returns the number of entries recorded below (and including) `dir`.
fn collect(
    dir: &Path,
    rel: &str,
    ign: &Ignores,
    out: &mut BTreeMap<String, PathBuf>,
    errors: &mut Vec<String>,
) -> usize {
    let entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(Result::ok).collect(),
        Err(e) => {
            errors.push(format!("{}: {e}", dir.display()));
            return 0;
        }
    };
    let mut added = 0;
    for e in entries {
        let path = e.path();
        let name = format!("{rel}{}", e.file_name().to_string_lossy());
        if path.is_dir() {
            let name = name + "/";
            if !ign.whole(&name) {
                added += collect(&path, &name, ign, out, errors);
            }
        } else if !ign.whole(&name) {
            out.insert(name, path);
            added += 1;
        }
    }
    if added == 0 && !rel.is_empty() {
        out.insert(rel.to_owned(), dir.to_path_buf());
        added = 1;
    }
    added
}

fn digest(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))[..16].to_owned()
}

fn bytes_note(d: &mut ChunkDiff, a: &[u8], b: &[u8]) {
    d.note(Category::Other, || {
        format!(
            "content differs: A {} bytes sha256 {}, B {} bytes sha256 {}",
            a.len(),
            digest(a),
            b.len(),
            digest(b)
        )
    });
}

/// Gzip or raw NBT file: the root must be a compound with an empty name and nothing may follow it. Also returns the
/// decompressed bytes.
fn decode_nbt(bytes: &[u8]) -> Result<(CompoundTag, Vec<u8>), String> {
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        mc_nbt::compress::gzip_decompress(bytes, None).map_err(|e| e.to_string())?
    } else {
        bytes.to_vec()
    };
    if raw.get(1..3) != Some(&[0, 0]) {
        return Err("root name is not empty".into());
    }
    let mut input = raw.as_slice();
    let tag = mc_nbt::read_named(&mut input, &mut NbtAccounter::unlimited_heap())
        .map_err(|e| e.to_string())?;
    match input.len() {
        0 => Ok((tag, raw)),
        n => Err(format!("{n} trailing bytes after the root compound")),
    }
}

/// Decodes both sides and diffs them; if exactly one side decodes, or neither, that is the difference.
fn decoded<T>(
    d: &mut ChunkDiff,
    (ba, bb): (&[u8], &[u8]),
    decode: fn(&[u8]) -> Result<T, String>,
    mut diff: impl FnMut(T, T, &mut ChunkDiff),
) {
    match (decode(ba), decode(bb)) {
        (Ok(x), Ok(y)) => diff(x, y, d),
        (Ok(_), Err(e)) => d.note(Category::Other, || format!("B does not decode: {e}")),
        (Err(e), Ok(_)) => d.note(Category::Other, || format!("A does not decode: {e}")),
        (Err(x), Err(y)) => {
            d.note(Category::Other, || {
                format!("neither side decodes (A: {x}; B: {y})")
            });
            bytes_note(d, ba, bb);
        }
    }
}

fn decode_json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

fn compare_file(
    rel: &str,
    pa: &Path,
    pb: &Path,
    o: &TreeOptions,
    errors: &mut Vec<String>,
) -> ChunkDiff {
    let mut d = ChunkDiff::new(o.max_listed);
    let (ba, bb) = match (std::fs::read(pa), std::fs::read(pb)) {
        (Ok(a), Ok(b)) => (a, b),
        (a, b) => {
            errors.extend(
                [a.err(), b.err()]
                    .into_iter()
                    .flatten()
                    .map(|e| format!("{rel}: {e}")),
            );
            return d;
        }
    };
    if ba == bb {
        return d;
    }
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "dat" | "dat_old" | "nbt" => {
            decoded(
                &mut d,
                (&ba, &bb),
                decode_nbt,
                |(mut x, px), (mut y, py), d| {
                    o.ignores.strip_nbt(rel, &mut x);
                    o.ignores.strip_nbt(rel, &mut y);
                    diff_decoded((&x, &px), (&y, &py), !o.ignores.touches(rel), d);
                },
            );
            let gz = |b: &[u8]| b.starts_with(&[0x1f, 0x8b]);
            if gz(&ba) != gz(&bb) {
                d.note(Category::Other, || "gzip on one side only".into());
            }
        }
        "json" => decoded(&mut d, (&ba, &bb), decode_json, |mut x, mut y, d| {
            o.ignores.strip_json(rel, &mut x);
            o.ignores.strip_json(rel, &mut y);
            diff_json(&x, &y, d);
        }),
        "mca" if region_coords(name).is_some() => {
            region_file(rel, name, (pa, pb), o, &mut d, errors);
        }
        _ => bytes_note(&mut d, &ba, &bb),
    }
    d
}

fn region_file(
    rel: &str,
    name: &str,
    (pa, pb): (&Path, &Path),
    o: &TreeOptions,
    d: &mut ChunkDiff,
    errors: &mut Vec<String>,
) {
    let Some(coords) = region_coords(name) else {
        return;
    };
    let store = |p: &Path| {
        Store::new(
            p.parent().unwrap_or(p).to_path_buf(),
            "minecraft:overworld",
            "chunk",
        )
    };
    let (mut sa, mut sb) = (store(pa), store(pb));
    let strip = |t: &mut CompoundTag| o.ignores.strip_nbt(rel, t);
    let payload = !o.ignores.touches(rel);
    compare_region(
        &mut sa,
        &mut sb,
        coords,
        o.chunks,
        o.max_listed,
        &strip,
        payload,
        |pos, slot| {
            let at = format!("chunk [{},{}]", pos.x, pos.z);
            match slot {
                Slot::Same => {}
                Slot::OnlyA => d.note(Category::Other, || format!("{at}: only in A")),
                Slot::OnlyB => d.note(Category::Other, || format!("{at}: only in B")),
                Slot::Error(e) => errors.push(format!("{rel} {at}: {e}")),
                Slot::Differs(c) => {
                    let n = count(&c);
                    let first = c
                        .cats
                        .get(&Category::Other)
                        .and_then(|c| c.samples.first().cloned())
                        .unwrap_or_default();
                    d.note(Category::Other, || {
                        format!("{at}: {n} differences, first {first}")
                    });
                }
            }
        },
    );
}

pub fn compare_trees(a: &Path, b: &Path, o: &TreeOptions) -> TreeReport {
    let mut r = TreeReport::default();
    let (mut fa, mut fb) = (BTreeMap::new(), BTreeMap::new());
    collect(a, "", &o.ignores, &mut fa, &mut r.errors);
    collect(b, "", &o.ignores, &mut fb, &mut r.errors);
    r.files_a = fa.len() as u64;
    r.files_b = fb.len() as u64;
    let paths: std::collections::BTreeSet<&String> = fa.keys().chain(fb.keys()).collect();
    for rel in paths {
        let only = |kind| TreeEntry {
            path: rel.clone(),
            kind,
            count: 1,
            samples: Vec::new(),
        };
        match (fa.get(rel), fb.get(rel)) {
            (Some(_), None) => r.entries.push(only(Kind::OnlyA)),
            (None, Some(_)) => r.entries.push(only(Kind::OnlyB)),
            (Some(_), Some(_)) if rel.ends_with('/') => r.identical += 1,
            (Some(pa), Some(pb)) => {
                let errors = r.errors.len();
                let d = compare_file(rel, pa, pb, o, &mut r.errors);
                match d.cats.into_iter().next() {
                    None => r.identical += (r.errors.len() == errors) as u64,
                    Some((_, c)) => r.entries.push(TreeEntry {
                        path: rel.clone(),
                        kind: Kind::Differs,
                        count: c.count,
                        samples: c.samples,
                    }),
                }
            }
            (None, None) => {}
        }
    }
    r
}

pub fn render_text(a: &str, b: &str, r: &TreeReport, max_listed: usize) -> String {
    let mut s = String::new();
    let only = |k| r.entries.iter().filter(|e| e.kind == k).count();
    let _ = writeln!(s, "worldcmp --tree A={a} B={b}");
    let _ = writeln!(
        s,
        "entries: A={} B={}; identical {}, differing {}, only in A {}, only in B {}",
        r.files_a,
        r.files_b,
        r.identical,
        r.differing(),
        only(Kind::OnlyA),
        only(Kind::OnlyB)
    );
    for e in &r.errors {
        let _ = writeln!(s, "ERROR {e}");
    }
    for e in r.entries.iter().take(max_listed) {
        match e.kind {
            Kind::Differs => {
                let _ = writeln!(s, "  differs: {} ({} differences)", e.path, e.count);
            }
            k => {
                let _ = writeln!(s, "  {}: {}", k.name(), e.path);
            }
        }
        for m in &e.samples {
            let _ = writeln!(s, "      {m}");
        }
    }
    if r.entries.len() > max_listed {
        let _ = writeln!(s, "  ... {} more", r.entries.len() - max_listed);
    }
    let _ = writeln!(
        s,
        "RESULT: {}",
        if r.is_clean() {
            "IDENTICAL"
        } else {
            "DIFFERENT"
        }
    );
    s
}

pub fn render_json(a: &str, b: &str, r: &TreeReport) -> String {
    let entries: Vec<String> = r
        .entries
        .iter()
        .map(|e| {
            format!(
                "{{\"path\":{},\"kind\":{},\"count\":{},\"samples\":[{}]}}",
                js(&e.path),
                js(e.kind.name()),
                e.count,
                e.samples
                    .iter()
                    .map(|m| js(m))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect();
    format!(
        "{{\"a\":{},\"b\":{},\"identical\":{},\"clean\":{},\"errors\":[{}],\"entries\":[{}]}}\n",
        js(a),
        js(b),
        r.identical,
        r.is_clean(),
        r.errors.iter().map(|e| js(e)).collect::<Vec<_>>().join(","),
        entries.join(",")
    )
}
