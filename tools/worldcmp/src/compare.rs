//! Chunk-level comparison of two chunk NBT compounds (as stored in `region/*.mca`).
//!
//! Everything is compared semantically (palette-independent blocks/biomes, order-insensitive lists where vanilla's order
//! is not observable) and reported as short human-readable messages. Ignored: `LastUpdate`, `InhabitedTime`, light data
//! (`sections[].BlockLight/SkyLight`), entity `UUID`s (random in vanilla).

use std::collections::{BTreeMap, BTreeSet};

use mc_nbt::{CompoundTag, ListTag, Tag};

pub const STATUSES: [&str; 12] = [
    "empty",
    "structure_starts",
    "structure_references",
    "biomes",
    "noise",
    "surface",
    "carvers",
    "features",
    "initialize_light",
    "light",
    "spawn",
    "full",
];

/// Index of a chunk status (`minecraft:` prefix optional) in generation order.
pub fn status_rank(name: &str) -> Option<usize> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    STATUSES.iter().position(|s| *s == n)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Category {
    Status,
    IsLightOn,
    Blocks,
    Biomes,
    Heightmaps,
    BlockEntities,
    StructureStarts,
    StructureReferences,
    PostProcessing,
    Ticks,
    Entities,
    Other,
}

impl Category {
    pub const ALL: [Category; 12] = [
        Category::Status,
        Category::IsLightOn,
        Category::Blocks,
        Category::Biomes,
        Category::Heightmaps,
        Category::BlockEntities,
        Category::StructureStarts,
        Category::StructureReferences,
        Category::PostProcessing,
        Category::Ticks,
        Category::Entities,
        Category::Other,
    ];

    pub fn from_name(n: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.name() == n)
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Status => "status",
            Category::IsLightOn => "isLightOn",
            Category::Blocks => "blocks",
            Category::Biomes => "biomes",
            Category::Heightmaps => "heightmaps",
            Category::BlockEntities => "block_entities",
            Category::StructureStarts => "structure_starts",
            Category::StructureReferences => "structure_references",
            Category::PostProcessing => "post_processing",
            Category::Ticks => "ticks",
            Category::Entities => "entities",
            Category::Other => "other",
        }
    }
}

/// Number of differences in a category plus the first few as text.
#[derive(Default, Debug, Clone)]
pub struct CatDiff {
    pub count: u64,
    pub samples: Vec<String>,
}

/// Per structure id: starts only in A / only in B / present in both but different.
#[derive(Default, Debug, Clone)]
pub struct StartMismatch {
    pub only_a: u64,
    pub only_b: u64,
    pub differs: u64,
}

#[derive(Default, Debug)]
pub struct ChunkDiff {
    pub cats: BTreeMap<Category, CatDiff>,
    /// `"a_block -> b_block"` (or `"block [state differs]"`) → differing positions.
    pub block_hist: BTreeMap<String, u64>,
    pub starts: BTreeMap<String, StartMismatch>,
    max: usize,
}

impl ChunkDiff {
    pub fn new(max_listed: usize) -> Self {
        Self { max: max_listed, ..Self::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.cats.is_empty()
    }

    pub fn note(&mut self, cat: Category, msg: impl FnOnce() -> String) {
        let max = self.max;
        let e = self.cats.entry(cat).or_default();
        e.count += 1;
        if e.samples.len() < max {
            e.samples.push(msg());
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// canonical text + deep diff

fn short(mut s: String) -> String {
    if s.chars().count() > 160 {
        s = s.chars().take(157).collect::<String>() + "...";
    }
    s
}

/// Canonical (compound keys sorted, floats via `{:?}` so `-0.0`/NaN stay distinguishable) SNBT-like text.
pub fn canon(tag: &Tag) -> String {
    let mut s = String::new();
    canon_into(tag, &mut s);
    s
}

fn canon_into(tag: &Tag, s: &mut String) {
    use std::fmt::Write;
    match tag {
        Tag::End => s.push_str("<end>"),
        Tag::Byte(v) => write!(s, "{v}b").unwrap(),
        Tag::Short(v) => write!(s, "{v}s").unwrap(),
        Tag::Int(v) => write!(s, "{v}").unwrap(),
        Tag::Long(v) => write!(s, "{v}L").unwrap(),
        Tag::Float(v) => write!(s, "{v:?}f").unwrap(),
        Tag::Double(v) => write!(s, "{v:?}d").unwrap(),
        Tag::String(v) => write!(s, "{v:?}").unwrap(),
        Tag::ByteArray(v) => write!(s, "[B;{v:?}]").unwrap(),
        Tag::IntArray(v) => write!(s, "[I;{v:?}]").unwrap(),
        Tag::LongArray(v) => write!(s, "[L;{v:?}]").unwrap(),
        Tag::List(l) => {
            s.push('[');
            for (i, e) in l.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                canon_into(e, s);
            }
            s.push(']');
        }
        Tag::Compound(c) => {
            s.push('{');
            let sorted: BTreeMap<&String, &Tag> = c.iter().collect();
            for (i, (k, v)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                write!(s, "{k}:").unwrap();
                canon_into(v, s);
            }
            s.push('}');
        }
    }
}

/// Deep structural diff (compounds by key, lists element-wise in order); one message per differing leaf.
pub fn diff_tag(path: &str, a: &Tag, b: &Tag, out: &mut Vec<String>) {
    match (a, b) {
        (Tag::Compound(x), Tag::Compound(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                let p = if path.is_empty() { k.to_string() } else { format!("{path}.{k}") };
                match (x.get(k), y.get(k)) {
                    (Some(l), Some(r)) => diff_tag(&p, l, r, out),
                    (Some(l), None) => out.push(short(format!("{p}: only in A = {}", canon(l)))),
                    (None, Some(r)) => out.push(short(format!("{p}: only in B = {}", canon(r)))),
                    (None, None) => {}
                }
            }
        }
        (Tag::List(x), Tag::List(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: list length {} (A) != {} (B)", x.len(), y.len()));
            }
            for (i, (l, r)) in x.iter().zip(y.iter()).enumerate() {
                diff_tag(&format!("{path}[{i}]"), l, r, out);
            }
        }
        _ => {
            let (l, r) = (canon(a), canon(b));
            if l != r {
                out.push(short(format!("{path}: {l} (A) != {r} (B)")));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// paletted containers

struct Container {
    palette: Vec<String>,
    idx: Vec<u32>,
}

impl Container {
    fn single(name: &str, n: usize) -> Self {
        Self { palette: vec![name.to_owned()], idx: vec![0; n] }
    }
    fn at(&self, i: usize) -> &str {
        &self.palette[self.idx[i] as usize]
    }
}

fn ceil_log2(n: usize) -> usize {
    if n <= 1 { 0 } else { (usize::BITS - (n - 1).leading_zeros()) as usize }
}

/// A palette entry in the 26.x layout: a bare string for property-less states, else `{id, properties:{..}}`.
fn state_string(tag: &Tag) -> String {
    if let Some(name) = tag.as_string() {
        return name.to_owned();
    }
    let Some(c) = tag.as_compound() else { return canon(tag) };
    let name = c.get_string_or("id", "?");
    match c.get_compound("properties") {
        Some(p) if !p.is_empty() => {
            let props: BTreeMap<&String, String> =
                p.iter().map(|(k, v)| (k, v.as_string().map(str::to_owned).unwrap_or_else(|| canon(v)))).collect();
            let inner: Vec<String> = props.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{name}[{}]", inner.join(","))
        }
        _ => name.to_owned(),
    }
}

/// Decodes a `block_states` / `biomes` section compound with vanilla's persisted layout: palette list + optional long
/// array packed without crossing longs, `max(min_bits, ceillog2(palette))` bits per entry.
fn decode_container(c: &CompoundTag, n: usize, min_bits: usize, blocks: bool) -> Result<Container, String> {
    let Some(pal) = c.get_list("palette") else { return Err("no palette".into()) };
    if pal.is_empty() {
        return Err("empty palette".into());
    }
    let palette: Vec<String> =
        pal.iter().map(|t| if blocks { state_string(t) } else { t.as_string().map(str::to_owned).unwrap_or_else(|| canon(t)) }).collect();
    let bits = min_bits.max(ceil_log2(palette.len()));
    let idx = match c.get_long_array("data") {
        None if palette.len() == 1 => vec![0; n],
        None => return Err(format!("palette of {} without data", palette.len())),
        Some(data) => {
            let bits = bits.max(1);
            let per = 64 / bits;
            if data.len() != n.div_ceil(per) {
                return Err(format!("data length {} != {} expected for {bits} bits", data.len(), n.div_ceil(per)));
            }
            let mask = (1u64 << bits) - 1;
            (0..n).map(|i| ((data[i / per] as u64 >> ((i % per) * bits)) & mask) as u32).collect()
        }
    };
    if let Some(&bad) = idx.iter().find(|&&i| i as usize >= palette.len()) {
        return Err(format!("palette index {bad} out of range {}", palette.len()));
    }
    Ok(Container { palette, idx })
}

fn block_type(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

fn sections(c: &CompoundTag) -> BTreeMap<i32, &CompoundTag> {
    c.get_list_or_empty("sections")
        .iter()
        .filter_map(|t| t.as_compound())
        .map(|s| (i32::from(s.get_byte_or("Y", 0)), s))
        .collect()
}

fn compare_sections(a: &CompoundTag, b: &CompoundTag, d: &mut ChunkDiff) {
    let (sa, sb) = (sections(a), sections(b));
    let ys: BTreeSet<i32> = sa.keys().chain(sb.keys()).copied().collect();
    for y in ys {
        let mut blocks = Vec::new();
        for (side, s) in [("A", sa.get(&y)), ("B", sb.get(&y))] {
            blocks.push(match s.and_then(|s| s.get_compound("block_states")) {
                None => Some(Container::single("minecraft:air", 4096)), // vanilla omits all-air sections
                Some(c) => match decode_container(c, 4096, 4, true) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        d.note(Category::Blocks, || format!("section {y} ({side}): undecodable block_states: {e}"));
                        None
                    }
                },
            });
        }
        if let [Some(x), Some(z)] = blocks.as_slice() {
            for i in 0..4096 {
                let (l, r) = (x.at(i), z.at(i));
                if l == r {
                    continue;
                }
                let (lt, rt) = (block_type(l), block_type(r));
                let key = if lt == rt { format!("{lt} [state differs]") } else { format!("{lt} -> {rt}") };
                *d.block_hist.entry(key).or_default() += 1;
                d.note(Category::Blocks, || {
                    let (bx, by, bz) = (i & 15, (i >> 8) & 15, (i >> 4) & 15);
                    format!("section {y} local ({bx},{by},{bz}) y={}: {l} (A) != {r} (B)", y * 16 + by as i32)
                });
            }
        }
        // biomes: a section only present on one side has no stored biomes on the other (vanilla drops empty sections)
        if let (Some(ca), Some(cb)) = (
            sa.get(&y).and_then(|s| s.get_compound("biomes")),
            sb.get(&y).and_then(|s| s.get_compound("biomes")),
        ) {
            match (decode_container(ca, 64, 0, false), decode_container(cb, 64, 0, false)) {
                (Ok(x), Ok(z)) => {
                    for i in 0..64 {
                        if x.at(i) != z.at(i) {
                            d.note(Category::Biomes, || {
                                format!("section {y} cell ({},{},{}): {} (A) != {} (B)", i & 3, (i >> 4) & 3, (i >> 2) & 3, x.at(i), z.at(i))
                            });
                        }
                    }
                }
                (x, z) => {
                    for e in [x.err(), z.err()].into_iter().flatten() {
                        d.note(Category::Biomes, || format!("section {y}: undecodable biomes: {e}"));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// heightmaps

fn unpack_heights(data: &[i64]) -> Option<Vec<u32>> {
    let bits = (1..=32usize).find(|&b| 256usize.div_ceil(64 / b) == data.len())?;
    let per = 64 / bits;
    let mask = (1u64 << bits) - 1;
    Some((0..256).map(|i| ((data[i / per] as u64 >> ((i % per) * bits)) & mask) as u32).collect())
}

fn compare_heightmaps(a: &CompoundTag, b: &CompoundTag, d: &mut ChunkDiff) {
    let (ha, hb) = (a.get_compound_or_empty("Heightmaps"), b.get_compound_or_empty("Heightmaps"));
    let names: BTreeSet<&String> = ha.keys().chain(hb.keys()).collect();
    for n in names {
        match (ha.get_long_array(n), hb.get_long_array(n)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => match (unpack_heights(x), unpack_heights(y)) {
                (Some(vx), Some(vy)) => {
                    for i in 0..256 {
                        if vx[i] != vy[i] {
                            d.note(Category::Heightmaps, || format!("{n} column ({},{}): {} (A) != {} (B)", i & 15, i >> 4, vx[i], vy[i]));
                        }
                    }
                }
                _ => d.note(Category::Heightmaps, || format!("{n}: raw arrays differ (lengths {} / {})", x.len(), y.len())),
            },
            (Some(_), None) => d.note(Category::Heightmaps, || format!("{n}: only in A")),
            (None, Some(_)) => d.note(Category::Heightmaps, || format!("{n}: only in B")),
            (None, None) => d.note(Category::Heightmaps, || format!("{n}: not a long array")),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// order-insensitive keyed lists

/// Compares two lists of compounds as multisets keyed by `key` (after `prep`); equal keys with different contents get a
/// deep diff, keys on one side only are reported as such.
pub fn diff_keyed(
    d: &mut ChunkDiff,
    cat: Category,
    a: &ListTag,
    b: &ListTag,
    key: impl Fn(&Tag) -> String,
    prep: impl Fn(&Tag) -> Tag,
) {
    let group = |l: &ListTag| {
        let mut m: BTreeMap<String, Vec<(String, Tag)>> = BTreeMap::new();
        for t in l.iter() {
            let t = prep(t);
            m.entry(key(&t)).or_default().push((canon(&t), t));
        }
        for v in m.values_mut() {
            v.sort_by(|x, y| x.0.cmp(&y.0));
        }
        m
    };
    let (ga, gb) = (group(a), group(b));
    let keys: BTreeSet<&String> = ga.keys().chain(gb.keys()).collect();
    for k in keys {
        let (va, vb) = (ga.get(k).map_or(&[][..], Vec::as_slice), gb.get(k).map_or(&[][..], Vec::as_slice));
        for i in 0..va.len().max(vb.len()) {
            match (va.get(i), vb.get(i)) {
                (Some(x), Some(y)) if x.0 == y.0 => {}
                (Some(x), Some(y)) => {
                    let mut out = Vec::new();
                    diff_tag(k, &x.1, &y.1, &mut out);
                    for m in out {
                        d.note(cat, || m);
                    }
                }
                (Some(x), None) => d.note(cat, || short(format!("{k}: only in A = {}", x.0))),
                (None, Some(y)) => d.note(cat, || short(format!("{k}: only in B = {}", y.0))),
                (None, None) => {}
            }
        }
    }
}

fn int_of(t: &Tag, k: &str) -> i32 {
    t.as_compound().and_then(|c| c.get_int(k)).unwrap_or(0)
}

fn block_entity_key(t: &Tag) -> String {
    let id = t.as_compound().map_or("?", |c| c.get_string_or("id", "?"));
    format!("{id} @ ({},{},{})", int_of(t, "x"), int_of(t, "y"), int_of(t, "z"))
}

/// Entities are keyed by id + position (3 decimals); `UUID` is random in vanilla and ignored.
pub fn entity_key(t: &Tag) -> String {
    let c = t.as_compound();
    let id = c.map_or("?", |c| c.get_string_or("id", "?"));
    let pos = c.and_then(|c| c.get_list("Pos")).map(|l| l.iter().filter_map(Tag::as_double).map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(","));
    format!("{id} @ ({})", pos.unwrap_or_default())
}

pub fn strip_uuid(t: &Tag) -> Tag {
    let mut t = t.clone();
    if let Tag::Compound(c) = &mut t {
        c.remove("UUID");
    }
    t
}

fn unpack_chunk_key(k: i64) -> String {
    format!("[{},{}]", k as i32, (k >> 32) as i32)
}

fn compare_structures(a: &CompoundTag, b: &CompoundTag, d: &mut ChunkDiff) {
    let (sa, sb) = (a.get_compound_or_empty("structures"), b.get_compound_or_empty("structures"));
    let (xa, xb) = (sa.get_compound_or_empty("starts"), sb.get_compound_or_empty("starts"));
    let ids: BTreeSet<&String> = xa.keys().chain(xb.keys()).collect();
    for id in ids {
        match (xa.get(id), xb.get(id)) {
            (Some(x), Some(y)) => {
                let mut out = Vec::new();
                diff_tag(id, x, y, &mut out);
                if !out.is_empty() {
                    d.starts.entry(id.clone()).or_default().differs += 1;
                }
                for m in out {
                    d.note(Category::StructureStarts, || m);
                }
            }
            (Some(_), None) => {
                d.starts.entry(id.clone()).or_default().only_a += 1;
                d.note(Category::StructureStarts, || format!("{id}: start only in A"));
            }
            (None, Some(_)) => {
                d.starts.entry(id.clone()).or_default().only_b += 1;
                d.note(Category::StructureStarts, || format!("{id}: start only in B"));
            }
            (None, None) => {}
        }
    }
    let (ra, rb) = (sa.get_compound_or_empty("References"), sb.get_compound_or_empty("References"));
    let ids: BTreeSet<&String> = ra.keys().chain(rb.keys()).collect();
    for id in ids {
        let set = |c: &CompoundTag| c.get_long_array(id).map(|v| v.iter().copied().collect::<BTreeSet<i64>>()).unwrap_or_default();
        let (x, y) = (set(ra), set(rb));
        for (side, only) in [("A", x.difference(&y)), ("B", y.difference(&x))] {
            let only: Vec<String> = only.map(|&k| unpack_chunk_key(k)).collect();
            if !only.is_empty() {
                d.note(Category::StructureReferences, || format!("{id}: references only in {side}: {}", only.join(" ")));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------

const HANDLED: [&str; 12] = [
    "LastUpdate",
    "InhabitedTime",
    "sections",
    "Heightmaps",
    "structures",
    "block_entities",
    "block_ticks",
    "fluid_ticks",
    "PostProcessing",
    "Status",
    "isLightOn",
    "entities",
];

/// Compares two chunk compounds (the root of a chunk NBT payload).
pub fn compare_chunk(a: &CompoundTag, b: &CompoundTag, max_listed: usize) -> ChunkDiff {
    let mut d = ChunkDiff::new(max_listed);
    let (sa, sb) = (a.get_string_or("Status", ""), b.get_string_or("Status", ""));
    if sa != sb {
        d.note(Category::Status, || format!("{sa:?} (A) != {sb:?} (B)"));
    }
    let (la, lb) = (a.get_boolean_or("isLightOn", false), b.get_boolean_or("isLightOn", false));
    if la != lb {
        d.note(Category::IsLightOn, || format!("{la} (A) != {lb} (B)"));
    }
    compare_sections(a, b, &mut d);
    compare_heightmaps(a, b, &mut d);
    diff_keyed(&mut d, Category::BlockEntities, a.get_list_or_empty("block_entities"), b.get_list_or_empty("block_entities"), block_entity_key, Tag::clone);
    for k in ["block_ticks", "fluid_ticks"] {
        diff_keyed(&mut d, Category::Ticks, a.get_list_or_empty(k), b.get_list_or_empty(k), |t| format!("{k} {}", canon(t)), Tag::clone);
    }
    // entities still embedded in the chunk NBT (proto chunks); full chunks keep them in `entities/` regions
    diff_keyed(&mut d, Category::Entities, a.get_list_or_empty("entities"), b.get_list_or_empty("entities"), entity_key, strip_uuid);
    compare_structures(a, b, &mut d);
    let mut out = Vec::new();
    let pp = |c: &CompoundTag| Tag::List(c.get_list_or_empty("PostProcessing").clone());
    diff_tag("PostProcessing", &pp(a), &pp(b), &mut out);
    for m in out {
        d.note(Category::PostProcessing, || m);
    }
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).filter(|k| !HANDLED.contains(&k.as_str())).collect();
    for k in keys {
        let mut out = Vec::new();
        match (a.get(k), b.get(k)) {
            (Some(x), Some(y)) => diff_tag(k, x, y, &mut out),
            (Some(x), None) => out.push(short(format!("{k}: only in A = {}", canon(x)))),
            (None, Some(y)) => out.push(short(format!("{k}: only in B = {}", canon(y)))),
            (None, None) => {}
        }
        for m in out {
            d.note(Category::Other, || m);
        }
    }
    d
}

/// Entity-region payloads (`entities/r.*.mca`): the `Entities` lists, UUIDs ignored. Absent == empty.
pub fn compare_entity_chunks(a: Option<&CompoundTag>, b: Option<&CompoundTag>, d: &mut ChunkDiff) {
    let empty = ListTag::new();
    let l = |c: Option<&CompoundTag>| c.and_then(|c| c.get_list("Entities")).cloned().unwrap_or_else(|| empty.clone());
    diff_keyed(d, Category::Entities, &l(a), &l(b), entity_key, strip_uuid);
}

#[cfg(test)]
mod tests;
