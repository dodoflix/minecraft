//! `Character.codePointOf(name)` (used by SNBT `\N{...}` escapes) backed by a table dumped from
//! the JDK (`tests/fixtures/UnicodeNamesGen.java` -> `unicode_names.tsv.gz`).

use flate2::read::GzDecoder;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::LazyLock;

struct Tables {
    /// Explicit Unicode names -> code point.
    names: HashMap<String, u32>,
    named: HashSet<u32>,
    /// `(start, end, block name)` for every `Character.UnicodeBlock`, sorted by start.
    blocks: Vec<(u32, u32, String)>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut text = String::new();
    GzDecoder::new(&include_bytes!("unicode_names.tsv.gz")[..])
        .read_to_string(&mut text)
        .expect("embedded gzip table");
    let mut t = Tables {
        names: HashMap::new(),
        named: HashSet::new(),
        blocks: Vec::new(),
    };
    for line in text.lines() {
        let mut f = line.split('\t');
        match f.next() {
            Some("N") => {
                let cp = u32::from_str_radix(f.next().unwrap(), 16).unwrap();
                t.names.insert(f.next().unwrap().to_string(), cp);
                t.named.insert(cp);
            }
            Some("B") => {
                let start = u32::from_str_radix(f.next().unwrap(), 16).unwrap();
                let end = u32::from_str_radix(f.next().unwrap(), 16).unwrap();
                t.blocks.push((start, end, f.next().unwrap().to_string()));
            }
            _ => {}
        }
    }
    t
});

/// `Character.codePointOf`: the name is trimmed and upper-cased, looked up in the Unicode name
/// table, or given as `<BLOCK NAME> <HEX>` for code points without a name.
pub(crate) fn code_point_of(name: &str) -> Option<u32> {
    let t = &*TABLES;
    let name = name.trim_matches(|c: char| c <= ' ').to_ascii_uppercase();
    if let Some(&cp) = t.names.get(&name) {
        return Some(cp);
    }
    let (_, hex) = name.rsplit_once(' ')?;
    let cp = u32::from_str_radix(hex, 16)
        .ok()
        .filter(|&cp| cp <= 0x10FFFF)?;
    if t.named.contains(&cp) {
        return None;
    }
    let (_, _, block) = t.blocks.iter().find(|(s, e, _)| (*s..=*e).contains(&cp))?;
    (name == format!("{block} {cp:X}")).then_some(cp)
}
