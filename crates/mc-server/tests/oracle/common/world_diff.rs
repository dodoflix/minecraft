//! NBT and world-directory diff shared by the save-file oracle tests: tag-by-tag comparison (paths, tag types, values,
//! key order, list order, with per-path tolerances and ignores), the list of files of a saved world split by file
//! kind, and readers for `.dat` files and the chunk NBT of `.mca` region files.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};

pub fn kind(t: &Tag) -> &'static str {
    match t {
        Tag::End => "end",
        Tag::Byte(_) => "byte",
        Tag::Short(_) => "short",
        Tag::Int(_) => "int",
        Tag::Long(_) => "long",
        Tag::Float(_) => "float",
        Tag::Double(_) => "double",
        Tag::ByteArray(_) => "byte[]",
        Tag::String(_) => "string",
        Tag::List(_) => "list",
        Tag::Compound(_) => "compound",
        Tag::IntArray(_) => "int[]",
        Tag::LongArray(_) => "long[]",
    }
}

/// What two saved NBT trees may differ in and still count as equal.
pub struct Rules<'a> {
    /// How far two numbers at `path` may be apart (clock-driven values).
    pub tolerance: &'a dyn Fn(&str) -> f64,
    /// A path whose whole subtree is not compared (a value that depends on wall time, a nondeterministic array).
    pub ignore: &'a dyn Fn(&str) -> bool,
    /// A compound at `path` whose key order is not compared (written in the order of an identity-hashed map).
    pub unordered: &'a dyn Fn(&str) -> bool,
}

pub fn diff_tag(path: &str, v: &Tag, o: &Tag, rules: &Rules, out: &mut Vec<String>) {
    if (rules.ignore)(path) {
        return;
    }
    match (v, o) {
        (Tag::Compound(a), Tag::Compound(b)) => diff_compound(path, a, b, rules, out),
        (Tag::List(a), Tag::List(b)) => {
            let (a, b): (Vec<&Tag>, Vec<&Tag>) = (a.iter().collect(), b.iter().collect());
            if a.len() != b.len() {
                out.push(format!(
                    "{path}: list length vanilla {} ours {}",
                    a.len(),
                    b.len()
                ));
            }
            for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                diff_tag(&format!("{path}[{i}]"), x, y, rules, out);
            }
        }
        _ if kind(v) != kind(o) => out.push(format!(
            "{path}: type vanilla {} ({v:?}) ours {} ({o:?})",
            kind(v),
            kind(o)
        )),
        _ if v != o => {
            let near = v.as_number().zip(o.as_number()).is_some_and(|(a, b)| {
                (a.double_value() - b.double_value()).abs() <= (rules.tolerance)(path)
            });
            if !near {
                out.push(format!("{path}: vanilla {v:?} ours {o:?}"));
            }
        }
        _ => {}
    }
}

pub fn diff_compound(
    path: &str,
    v: &CompoundTag,
    o: &CompoundTag,
    rules: &Rules,
    out: &mut Vec<String>,
) {
    for (k, vv) in v.iter() {
        let at = format!("{path}.{k}");
        if (rules.ignore)(&at) {
            continue;
        }
        match o.get(k) {
            None => out.push(format!("{at}: missing in ours (vanilla {vv:?})")),
            Some(ov) => diff_tag(&at, vv, ov, rules, out),
        }
    }
    for (k, ov) in o.iter() {
        let at = format!("{path}.{k}");
        if v.get(k).is_none() && !(rules.ignore)(&at) {
            out.push(format!("{at}: extra in ours ({ov:?})"));
        }
    }
    let common = |c: &CompoundTag, other: &CompoundTag| {
        c.keys()
            .filter(|k| other.get(k).is_some() && !(rules.ignore)(&format!("{path}.{k}")))
            .cloned()
            .collect::<Vec<_>>()
    };
    let (kv, ko) = (common(v, o), common(o, v));
    if kv != ko && !(rules.unordered)(path) {
        out.push(format!("{path}: key order vanilla {kv:?} ours {ko:?}"));
    }
}

/// `list` sorted by `key`: for lists written in the order of identity-hashed collections, where two vanilla runs
/// already disagree.
pub fn sorted_list(list: &ListTag, key: fn(&Tag) -> String) -> ListTag {
    let mut v: Vec<_> = list.iter().cloned().collect();
    v.sort_by_key(key);
    let mut l = ListTag::new();
    for t in v {
        l.push(t);
    }
    l
}

/// Sorts every `attributes` list (written in `Object2ObjectOpenHashMap<Holder<Attribute>, _>` order, which hashes by
/// identity) by attribute id, anywhere in the tree: the lists of a player, of passengers and of entities in chunks.
pub fn sort_attributes(c: &mut CompoundTag) {
    if let Some(list) = c.get_list("attributes") {
        let sorted = sorted_list(list, |t| {
            t.as_compound()
                .and_then(|c| c.get_string("id"))
                .unwrap_or("")
                .to_string()
        });
        c.put("attributes", sorted);
    }
    fn walk(tag: &mut Tag) {
        match tag {
            Tag::Compound(c) => sort_attributes(c),
            Tag::List(l) => l.iter_mut().for_each(walk),
            _ => {}
        }
    }
    c.values_mut().for_each(walk);
}

/// Normalises what vanilla writes in the order of identity-hashed collections (`Holder.Reference` hashes by identity,
/// so even two vanilla runs disagree): `attributes` sorted by id anywhere in the tree, the recipe book by name.
pub fn sort_unordered(c: &mut CompoundTag) {
    sort_attributes(c);
    if let Some(book) = c.get_compound("recipeBook") {
        let mut book = book.clone();
        for key in ["recipes", "toBeDisplayed"] {
            if let Some(list) = book.get_list(key) {
                let sorted = sorted_list(list, |t| t.as_string().unwrap_or("").to_string());
                book.put(key, sorted);
            }
        }
        c.put("recipeBook", book);
    }
}

/// The file kinds of a saved world, in the order a report lists them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FileKind {
    LevelDat,
    Region,
    Entities,
    Poi,
    SavedData,
    PlayerData,
    Stats,
    Advancements,
    Other,
}

impl FileKind {
    /// `rel` is the path inside the world directory with `/` separators.
    pub fn of(rel: &str) -> FileKind {
        let parts: Vec<&str> = rel.split('/').collect();
        match parts.as_slice() {
            ["level.dat"] => FileKind::LevelDat,
            ["players", "data", ..] => FileKind::PlayerData,
            ["players", "stats", ..] => FileKind::Stats,
            ["players", "advancements", ..] => FileKind::Advancements,
            ["data", ..] => FileKind::SavedData,
            [.., "region", _] if rel.ends_with(".mca") => FileKind::Region,
            [.., "entities", _] if rel.ends_with(".mca") => FileKind::Entities,
            [.., "poi", _] if rel.ends_with(".mca") => FileKind::Poi,
            ["dimensions", _, _, "data", ..] => FileKind::SavedData,
            _ => FileKind::Other,
        }
    }
}

/// Every file below `root`, as a path relative to it.
pub fn list_files(root: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

/// A gzip-compressed `.dat` file.
pub fn read_dat(path: &Path) -> CompoundTag {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The chunk NBT of every chunk stored in the region file at `path`, by chunk position.
pub fn read_region(path: &Path) -> BTreeMap<(i32, i32), CompoundTag> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .expect("file name");
    let mut it = name.split('.').skip(1);
    let (rx, rz): (i32, i32) = (
        it.next().and_then(|s| s.parse().ok()).expect("region x"),
        it.next().and_then(|s| s.parse().ok()).expect("region z"),
    );
    let info = RegionStorageInfo::new("world", Identifier::parse("overworld").unwrap(), "chunk");
    let mut storage = RegionFileStorage::new(info, path.parent().expect("folder"), false);
    let mut out = BTreeMap::new();
    for dz in 0..32 {
        for dx in 0..32 {
            let pos = ChunkPos::new(rx * 32 + dx, rz * 32 + dz);
            if let Some(tag) = storage
                .read(&pos)
                .unwrap_or_else(|e| panic!("{}: {pos:?}: {e}", path.display()))
            {
                out.insert((pos.x, pos.z), tag);
            }
        }
    }
    storage.close().expect("close the region storage");
    out
}

/// JSON compared by value: objects as sets of keys (Gson writes them in map order), arrays by index.
pub fn diff_json(
    path: &str,
    v: &serde_json::Value,
    o: &serde_json::Value,
    rules: &Rules,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    if (rules.ignore)(path) {
        return;
    }
    match (v, o) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, x) in a {
                let at = format!("{path}.{k}");
                match b.get(k) {
                    None if (rules.ignore)(&at) => {}
                    None => out.push(format!("{at}: missing in ours (vanilla {x})")),
                    Some(y) => diff_json(&at, x, y, rules, out),
                }
            }
            for (k, y) in b {
                if !a.contains_key(k) && !(rules.ignore)(&format!("{path}.{k}")) {
                    out.push(format!("{path}.{k}: extra in ours ({y})"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!(
                    "{path}: array length vanilla {} ours {}",
                    a.len(),
                    b.len()
                ));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff_json(&format!("{path}[{i}]"), x, y, rules, out);
            }
        }
        _ if v != o
            && v.as_str()
                .zip(o.as_str())
                .is_some_and(|(a, b)| is_time(a) && is_time(b)) => {}
        _ if v != o => {
            let near = v
                .as_f64()
                .zip(o.as_f64())
                .is_some_and(|(a, b)| (a - b).abs() <= (rules.tolerance)(path));
            if !near {
                out.push(format!("{path}: vanilla {v} ours {o}"));
            }
        }
        _ => {}
    }
}

/// An obtained-time string of an advancement file (`yyyy-MM-dd HH:mm:ss Z`), which is the time of the run.
fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 25
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 | 19 => *c == b' ',
            13 | 16 => *c == b':',
            20 => matches!(*c, b'+' | b'-'),
            _ => c.is_ascii_digit(),
        })
}
