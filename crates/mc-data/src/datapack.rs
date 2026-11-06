//! The vanilla built-in data pack (`data/**` of the server jar), the `assets/**` of the server jar (language files) and
//! `version.json`, embedded at build time as an archive of individually deflated files.
//!
//! Paths are relative to the jar root: `data/minecraft/loot_table/blocks/stone.json`, `assets/minecraft/lang/en_us.json`.

use std::borrow::Cow;
use std::io::Read;

static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/datapack.bin"));

/// `(path, deflated, offset, stored length, raw length)`, sorted by path.
mod index {
    include!(concat!(env!("OUT_DIR"), "/datapack_index.rs"));
}
use index::INDEX;

fn entry(path: &str) -> Option<&'static (&'static str, bool, u32, u32, u32)> {
    INDEX
        .binary_search_by(|e| e.0.cmp(path))
        .ok()
        .map(|i| &INDEX[i])
}

fn stored(e: &(&'static str, bool, u32, u32, u32)) -> &'static [u8] {
    &BLOB[e.2 as usize..(e.2 + e.3) as usize]
}

/// Whether a file exists in the embedded pack.
pub fn exists(path: &str) -> bool {
    entry(path).is_some()
}

/// File contents (inflated on demand; uncompressed files are borrowed from the binary).
pub fn read(path: &str) -> Option<Cow<'static, [u8]>> {
    let e = entry(path)?;
    let data = stored(e);
    if !e.1 {
        return Some(Cow::Borrowed(data));
    }
    let mut out = Vec::with_capacity(e.4 as usize);
    flate2::read::DeflateDecoder::new(data)
        .read_to_end(&mut out)
        .expect("embedded data pack is valid deflate");
    Some(Cow::Owned(out))
}

/// Text file contents (`None` if missing or not UTF-8).
pub fn read_string(path: &str) -> Option<String> {
    String::from_utf8(read(path)?.into_owned()).ok()
}

/// `data/<namespace>/<relative>` (e.g. `read_data("minecraft", "recipe/stick.json")`).
pub fn read_data(namespace: &str, relative: &str) -> Option<Cow<'static, [u8]>> {
    read(&format!("data/{namespace}/{relative}"))
}

/// Size of a file as stored uncompressed.
pub fn raw_len(path: &str) -> Option<usize> {
    entry(path).map(|e| e.4 as usize)
}

/// All file paths, sorted.
pub fn paths() -> impl ExactSizeIterator<Item = &'static str> + Clone {
    INDEX.iter().map(|e| e.0)
}

/// All file paths starting with `prefix` (e.g. `data/minecraft/tags/block/`), sorted.
pub fn list(prefix: &'static str) -> impl Iterator<Item = &'static str> {
    let start = INDEX.partition_point(|e| e.0 < prefix);
    INDEX[start..]
        .iter()
        .map(|e| e.0)
        .take_while(move |p| p.starts_with(prefix))
}
