//! Structural features of an NBT tree: every key path with its tag type, plus the short strings, the id-like numbers
//! and the byte values a fix may react to. Two samples with the same features are duplicates for the corpus, and a
//! set cover over the features picks the samples that exercise the most different shapes.

use mc_nbt::{CompoundTag, Tag};
use std::collections::BTreeSet;

/// FNV-1a, 64 bit: a hash that stays the same across toolchains (candidate files and selections must be reproducible).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Strings longer than this are free text (names, messages, JSON), not vocabulary.
const MAX_STRING: usize = 48;
/// Elements of a list that are not compounds are looked at up to this many.
const MAX_SCALARS: usize = 256;
const MAX_COMPOUNDS: usize = 4096;

/// Keys whose numbers are ids or variants (legacy numeric ids, data values, types).
fn id_like(key: &str) -> bool {
    key.eq_ignore_ascii_case("id")
        || key.ends_with("Id")
        || key.ends_with("ID")
        || matches!(
            key,
            "Data" | "Damage" | "Type" | "type" | "Variant" | "SkullType" | "Profession" | "Career"
        )
}

fn add(out: &mut BTreeSet<u64>, feature: &str) {
    out.insert(fnv1a(feature.as_bytes()));
}

/// The sorted feature hashes of `root`, the data version `from` included (so every version is covered).
pub fn features(root: &CompoundTag, from: i32) -> Vec<u64> {
    let mut out = BTreeSet::new();
    add(&mut out, &format!("from={from}"));
    compound(root, &mut String::new(), &mut out);
    out.into_iter().collect()
}

fn compound(c: &CompoundTag, path: &mut String, out: &mut BTreeSet<u64>) {
    for (key, tag) in c.iter() {
        let len = path.len();
        path.push('/');
        path.push_str(key);
        add(out, &format!("{path}:{}", tag.id()));
        value(key, tag, path, out);
        path.truncate(len);
    }
}

fn value(key: &str, tag: &Tag, path: &mut String, out: &mut BTreeSet<u64>) {
    match tag {
        Tag::String(s) if s.len() <= MAX_STRING => add(out, &format!("{path}={s}")),
        Tag::Byte(n) if id_like(key) => add(out, &format!("{path}={n}")),
        Tag::Short(n) if id_like(key) => add(out, &format!("{path}={n}")),
        Tag::Int(n) if id_like(key) => add(out, &format!("{path}={n}")),
        Tag::Long(n) if id_like(key) => add(out, &format!("{path}={n}")),
        Tag::ByteArray(b) if b.len() >= 16 => {
            let distinct: BTreeSet<i8> = b.iter().copied().collect();
            for v in distinct {
                add(out, &format!("{path}#{v}"));
            }
        }
        Tag::IntArray(a) if key == "Biomes" => {
            let distinct: BTreeSet<i32> = a.iter().copied().take(MAX_SCALARS).collect();
            for v in distinct {
                add(out, &format!("{path}#{v}"));
            }
        }
        Tag::Compound(c) => compound(c, path, out),
        Tag::List(list) => {
            let len = path.len();
            path.push_str("[]");
            add(out, &format!("{path}:{}", list.identify_raw_element_type()));
            let (mut compounds, mut scalars) = (0, 0);
            for element in list.iter() {
                let (seen, max) = if matches!(element, Tag::Compound(_)) {
                    (&mut compounds, MAX_COMPOUNDS)
                } else {
                    (&mut scalars, MAX_SCALARS)
                };
                if *seen >= max {
                    continue;
                }
                *seen += 1;
                value(key, element, path, out);
            }
            path.truncate(len);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CompoundTag {
        let mut entity = CompoundTag::new();
        entity.put_string("id", "minecraft:pig");
        entity.put_short("Damage", 5);
        entity.put_string("CustomName", "x".repeat(60));
        let mut list = mc_nbt::ListTag::new();
        list.push(Tag::Compound(entity));
        let mut root = CompoundTag::new();
        root.put("Entities", Tag::List(list));
        root.put_byte_array("Blocks", vec![1; 32]);
        root
    }

    #[test]
    fn features_name_types_ids_and_bytes_but_not_free_text() {
        let f = features(&sample(), 7);
        let has = |s: &str| f.contains(&fnv1a(s.as_bytes()));
        assert!(has("from=7"));
        assert!(has("/Entities:9"));
        assert!(has("/Entities[]:10"));
        assert!(has("/Entities[]/id=minecraft:pig"));
        assert!(has("/Entities[]/Damage=5"));
        assert!(has("/Blocks#1"));
        assert!(!f.contains(&fnv1a(
            format!("/Entities[]/CustomName={}", "x".repeat(60)).as_bytes()
        )));
    }

    #[test]
    fn the_features_do_not_depend_on_key_order() {
        let mut a = CompoundTag::new();
        a.put_int("a", 1);
        a.put_int("b", 2);
        let mut b = CompoundTag::new();
        b.put_int("b", 2);
        b.put_int("a", 1);
        assert_eq!(features(&a, 0), features(&b, 0));
    }
}
