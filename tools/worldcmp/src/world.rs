//! Reading two world directories' region files and aggregating per-chunk differences into a report.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::region::{RegionFileStorage, RegionStorageInfo};

use crate::compare::{
    Category, ChunkDiff, StartMismatch, compare_chunk, compare_entity_chunks, status_rank,
};

/// Folder holding `region/` and `entities/` of a dimension: `dimensions/<ns>/<path>` (26.x layout), else the legacy
/// layout (`.` / `DIM-1` / `DIM1`). `dim` is `overworld|nether|end|the_nether|the_end|ns:path`.
pub fn dimension_dir(world: &Path, dim: &str) -> (String, PathBuf) {
    let (ns, path) = dim.split_once(':').unwrap_or(("minecraft", dim));
    let path = match path {
        "nether" => "the_nether",
        "end" => "the_end",
        p => p,
    };
    let modern = world.join("dimensions").join(ns).join(path);
    let legacy = match (ns, path) {
        ("minecraft", "overworld") => Some(world.to_path_buf()),
        ("minecraft", "the_nether") => Some(world.join("DIM-1")),
        ("minecraft", "the_end") => Some(world.join("DIM1")),
        _ => None,
    };
    let dir = match legacy {
        Some(l) if !modern.is_dir() && l.join("region").is_dir() => l,
        _ => modern,
    };
    (format!("{ns}:{path}"), dir)
}

struct Store {
    storage: RegionFileStorage,
    folder: PathBuf,
}

impl Store {
    fn new(folder: PathBuf, dim: &str, kind: &str) -> Self {
        let id = Identifier::parse(dim).expect("dimension id validated by caller");
        Self {
            storage: RegionFileStorage::new(
                RegionStorageInfo::new("worldcmp", id, kind),
                &folder,
                false,
            ),
            folder,
        }
    }

    fn regions(&self) -> BTreeSet<(i32, i32)> {
        let Ok(rd) = std::fs::read_dir(&self.folder) else {
            return BTreeSet::new();
        };
        rd.filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let mut it = name.strip_prefix("r.")?.strip_suffix(".mca")?.split('.');
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
    }

    fn read(&mut self, pos: ChunkPos) -> Result<Option<CompoundTag>, String> {
        self.storage.read(&pos).map_err(|e| {
            format!(
                "chunk [{},{}] in {}: {e}",
                pos.x,
                pos.z,
                self.folder.display()
            )
        })
    }
}

#[derive(Clone, Debug)]
pub struct Filter {
    /// Categories whose differences are dropped (documented known residuals).
    pub ignore: Vec<Category>,
    /// Inclusive chunk rectangle `(x1, z1, x2, z2)`.
    pub chunks: Option<(i32, i32, i32, i32)>,
    /// Chunks whose `Status` ranks below this (in either world) are skipped.
    pub min_status: usize,
    pub max_listed: usize,
}

#[derive(Default, Debug)]
pub struct DimReport {
    pub dim: String,
    pub ignored: Vec<&'static str>,
    pub present_a: u64,
    pub present_b: u64,
    pub compared: u64,
    pub identical: u64,
    pub only_a: u64,
    pub only_b: u64,
    pub skipped_status: u64,
    pub errors: Vec<String>,
    pub differing: Vec<(ChunkPos, ChunkDiff)>,
    /// category → (chunks affected, differences)
    pub totals: BTreeMap<Category, (u64, u64)>,
    pub starts: BTreeMap<String, StartMismatch>,
    pub block_hist: BTreeMap<String, u64>,
}

impl DimReport {
    pub fn is_clean(&self) -> bool {
        self.differing.is_empty() && self.errors.is_empty() && self.only_a == 0 && self.only_b == 0
    }
}

fn rank(c: &CompoundTag) -> usize {
    status_rank(c.get_string_or("Status", "")).unwrap_or(0)
}

/// Compares one dimension of two worlds.
pub fn compare_dimension(a_world: &Path, b_world: &Path, dim: &str, f: &Filter) -> DimReport {
    let (name, da) = dimension_dir(a_world, dim);
    let (_, db) = dimension_dir(b_world, dim);
    let mut r = DimReport {
        dim: name.clone(),
        ignored: f.ignore.iter().map(|c| c.name()).collect(),
        ..DimReport::default()
    };
    let (mut ca, mut cb) = (
        Store::new(da.join("region"), &name, "chunk"),
        Store::new(db.join("region"), &name, "chunk"),
    );
    let (mut ea, mut eb) = (
        Store::new(da.join("entities"), &name, "entities"),
        Store::new(db.join("entities"), &name, "entities"),
    );
    let regions: BTreeSet<(i32, i32)> = ca.regions().union(&cb.regions()).copied().collect();
    for (rx, rz) in regions {
        for lz in 0..32 {
            for lx in 0..32 {
                let pos = ChunkPos::new(rx * 32 + lx, rz * 32 + lz);
                if let Some((x1, z1, x2, z2)) = f.chunks {
                    if pos.x < x1.min(x2)
                        || pos.x > x1.max(x2)
                        || pos.z < z1.min(z2)
                        || pos.z > z1.max(z2)
                    {
                        continue;
                    }
                }
                let (a, b) = match (ca.read(pos), cb.read(pos)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (a, b) => {
                        r.errors.extend([a.err(), b.err()].into_iter().flatten());
                        continue;
                    }
                };
                r.present_a += a.is_some() as u64;
                r.present_b += b.is_some() as u64;
                match (a, b) {
                    (None, None) => {}
                    (Some(a), None) => r.only_a += (rank(&a) >= f.min_status) as u64,
                    (None, Some(b)) => r.only_b += (rank(&b) >= f.min_status) as u64,
                    (Some(a), Some(b)) => {
                        if rank(&a) < f.min_status || rank(&b) < f.min_status {
                            r.skipped_status += 1;
                            continue;
                        }
                        r.compared += 1;
                        let mut d = compare_chunk(&a, &b, f.max_listed);
                        match (ea.read(pos), eb.read(pos)) {
                            (Ok(x), Ok(y)) => compare_entity_chunks(x.as_ref(), y.as_ref(), &mut d),
                            (x, y) => r.errors.extend([x.err(), y.err()].into_iter().flatten()),
                        }
                        d.cats.retain(|c, _| !f.ignore.contains(c));
                        if f.ignore.contains(&Category::Blocks) {
                            d.block_hist.clear();
                        }
                        if f.ignore.contains(&Category::StructureStarts) {
                            d.starts.clear();
                        }
                        if d.is_empty() {
                            r.identical += 1;
                        } else {
                            for (c, v) in &d.cats {
                                let t = r.totals.entry(*c).or_default();
                                t.0 += 1;
                                t.1 += v.count;
                            }
                            for (k, v) in &d.starts {
                                let s = r.starts.entry(k.clone()).or_default();
                                s.only_a += v.only_a;
                                s.only_b += v.only_b;
                                s.differs += v.differs;
                            }
                            for (k, v) in &d.block_hist {
                                *r.block_hist.entry(k.clone()).or_default() += v;
                            }
                            r.differing.push((pos, d));
                        }
                    }
                }
            }
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------------
// rendering

pub fn render_text(a: &str, b: &str, dims: &[DimReport], max_chunks_listed: usize) -> String {
    let mut s = String::new();
    writeln!(s, "worldcmp A={a} B={b}").unwrap();
    for r in dims {
        writeln!(s, "\n== {} ==", r.dim).unwrap();
        writeln!(
            s,
            "chunks: present A={} B={}; compared {} (identical {}, differing {}); only in A {}, only in B {}, skipped by status {}",
            r.present_a, r.present_b, r.compared, r.identical, r.differing.len(), r.only_a, r.only_b, r.skipped_status
        )
        .unwrap();
        if !r.ignored.is_empty() {
            writeln!(s, "ignored categories: {}", r.ignored.join(", ")).unwrap();
        }
        for e in &r.errors {
            writeln!(s, "ERROR {e}").unwrap();
        }
        if !r.totals.is_empty() {
            writeln!(s, "per-category (chunks affected / differences):").unwrap();
            for c in Category::ALL {
                if let Some((n, v)) = r.totals.get(&c) {
                    writeln!(s, "  {:<22} {n} / {v}", c.name()).unwrap();
                }
            }
        }
        if !r.starts.is_empty() {
            writeln!(s, "structure start mismatches:").unwrap();
            for (id, m) in &r.starts {
                writeln!(
                    s,
                    "  {id}: only in A {}, only in B {}, differing {}",
                    m.only_a, m.only_b, m.differs
                )
                .unwrap();
            }
        }
        if !r.block_hist.is_empty() {
            let mut h: Vec<_> = r.block_hist.iter().collect();
            h.sort_by(|x, y| y.1.cmp(x.1).then(x.0.cmp(y.0)));
            writeln!(s, "block difference histogram (positions, top 25):").unwrap();
            for (k, v) in h.into_iter().take(25) {
                writeln!(s, "  {v:>9}  {k}").unwrap();
            }
        }
        if !r.differing.is_empty() {
            writeln!(s, "differing chunks ({}):", r.differing.len()).unwrap();
            for (pos, d) in r.differing.iter().take(max_chunks_listed) {
                let sum: Vec<String> = d
                    .cats
                    .iter()
                    .map(|(c, v)| format!("{}={}", c.name(), v.count))
                    .collect();
                writeln!(s, "  [{},{}] {}", pos.x, pos.z, sum.join(" ")).unwrap();
                for (c, v) in &d.cats {
                    for m in &v.samples {
                        writeln!(s, "      {}: {m}", c.name()).unwrap();
                    }
                }
            }
            if r.differing.len() > max_chunks_listed {
                writeln!(s, "  ... {} more", r.differing.len() - max_chunks_listed).unwrap();
            }
        }
        writeln!(
            s,
            "{}",
            if r.is_clean() {
                "RESULT: IDENTICAL"
            } else {
                "RESULT: DIFFERENT"
            }
        )
        .unwrap();
    }
    s
}

fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => write!(o, "\\u{:04x}", c as u32).unwrap(),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub fn render_json(a: &str, b: &str, dims: &[DimReport]) -> String {
    let obj = |kv: Vec<(String, String)>| {
        format!(
            "{{{}}}",
            kv.into_iter()
                .map(|(k, v)| format!("{}:{v}", js(&k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let arr = |v: Vec<String>| format!("[{}]", v.join(","));
    let dims_json: Vec<String> = dims
        .iter()
        .map(|r| {
            let totals = obj(r.totals.iter().map(|(c, (n, v))| (c.name().into(), format!("{{\"chunks\":{n},\"differences\":{v}}}"))).collect());
            let starts = obj(
                r.starts
                    .iter()
                    .map(|(k, m)| (k.clone(), format!("{{\"only_a\":{},\"only_b\":{},\"differs\":{}}}", m.only_a, m.only_b, m.differs)))
                    .collect(),
            );
            let hist = obj(r.block_hist.iter().map(|(k, v)| (k.clone(), v.to_string())).collect());
            let chunks = arr(
                r.differing
                    .iter()
                    .map(|(p, d)| {
                        let cats = obj(
                            d.cats
                                .iter()
                                .map(|(c, v)| {
                                    (c.name().into(), format!("{{\"count\":{},\"samples\":{}}}", v.count, arr(v.samples.iter().map(|m| js(m)).collect())))
                                })
                                .collect(),
                        );
                        format!("{{\"x\":{},\"z\":{},\"categories\":{cats}}}", p.x, p.z)
                    })
                    .collect(),
            );
            format!(
                "{{\"dim\":{},\"present_a\":{},\"present_b\":{},\"compared\":{},\"identical\":{},\"differing\":{},\"only_a\":{},\"only_b\":{},\"skipped_status\":{},\"clean\":{},\"ignored\":{},\"errors\":{},\"category_totals\":{totals},\"structure_start_mismatches\":{starts},\"block_histogram\":{hist},\"differing_chunks\":{chunks}}}",
                js(&r.dim), r.present_a, r.present_b, r.compared, r.identical, r.differing.len(), r.only_a, r.only_b, r.skipped_status, r.is_clean(),
                arr(r.ignored.iter().map(|e| js(e)).collect()),
                arr(r.errors.iter().map(|e| js(e)).collect())
            )
        })
        .collect();
    format!(
        "{{\"a\":{},\"b\":{},\"dimensions\":{}}}\n",
        js(a),
        js(b),
        arr(dims_json)
    )
}
