//! PalettedContainer vs the real vanilla class: replays the scripted sets of
//! `fixtures/palette/PaletteGolden.java` and compares network bytes, pack(), count() and the NBT codec.

use mc_data::block::BlockState;
use mc_nbt::{Tag, snbt};
use mc_world::palette::*;

struct TestBiomeCodec;
impl ElementCodec<BiomeId> for TestBiomeCodec {
    fn decode(&self, tag: &Tag) -> Result<BiomeId, String> {
        let s = tag.as_string().ok_or("not a string")?;
        s.strip_prefix('b').and_then(|n| n.parse().ok()).map(BiomeId).ok_or_else(|| "bad".into())
    }
    fn encode(&self, v: BiomeId) -> Result<Tag, String> {
        Ok(Tag::String(format!("b{}", v.0)))
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn longs(s: &str) -> Vec<i64> {
    s.split(',').filter(|p| !p.is_empty()).map(|p| p.parse().unwrap()).collect()
}

/// Checks one CHECK block against our container; `$id` maps a value to its global id.
macro_rules! check_block {
    ($c:expr, $lines:expr, $codec:expr, $id:expr, $ctx:expr) => {{
        let c = &$c;
        let l = &$lines;
        let mut net = Vec::new();
        c.write(&mut net).unwrap();
        assert_eq!(hex(&net), l[0].strip_prefix("NET ").unwrap(), "{} net", $ctx);
        assert_eq!(c.bits_per_entry().to_string(), l[1].strip_prefix("BITS ").unwrap(), "{} bits", $ctx);
        assert_eq!(c.serialized_size().unwrap().to_string(), l[2].strip_prefix("SIZE ").unwrap(), "{} size", $ctx);
        let packed = c.pack();
        let parts: Vec<&str> = l[3].strip_prefix("PACK ").unwrap().split(" | ").collect();
        let entries: Vec<u32> = parts[0].split(',').filter(|p| !p.is_empty()).map(|p| p.parse().unwrap()).collect();
        let ours: Vec<u32> = packed.palette_entries.iter().map(|v| $id(*v)).collect();
        assert_eq!(ours, entries, "{} pack entries", $ctx);
        let data = if parts[1] == "-" { None } else { Some(longs(parts[1])) };
        assert_eq!(packed.storage, data, "{} pack data", $ctx);
        assert_eq!(packed.bits_per_entry.to_string(), parts[2], "{} pack bits", $ctx);
        let mut counts = std::collections::BTreeMap::new();
        c.count(|v, n| *counts.entry($id(v)).or_insert(0usize) += n);
        let want: String = counts.iter().map(|(k, v)| format!("{k}:{v},")).collect();
        assert_eq!(want, l[4].strip_prefix("COUNT ").unwrap(), "{} count", $ctx);
        let want_nbt = snbt::parse(l[5].strip_prefix("NBT ").unwrap()).unwrap();
        assert_eq!(Tag::Compound(c.to_nbt(&$codec).unwrap()), want_nbt, "{} nbt", $ctx);
        // network round trip
        let mut back = c.recreate();
        back.read(&mut net.as_slice()).unwrap();
        let mut again = Vec::new();
        back.write(&mut again).unwrap();
        assert_eq!(again, net, "{} read/write", $ctx);
        for i in 0..c.strategy().entry_count() {
            assert_eq!(back.get_index(i), c.get_index(i), "{} read value {i}", $ctx);
        }
    }};
}

#[test]
fn palette_matches_vanilla() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/palette/golden.txt")).unwrap();
    let mut lines = text.lines().peekable();
    let mut checks = 0;
    while let Some(head) = lines.next() {
        let head: Vec<&str> = head.split(' ').collect();
        assert_eq!(head[0], "CASE");
        let kind = head[1];
        let size: usize = head[2].strip_prefix("mode").map_or_else(|| head[2].parse().unwrap(), |_| 0);
        let ctx = format!("{} {}", kind, head[2]);
        let (mut blocks, mut biomes);
        let block_codec = BlockStateCodec;
        if kind == "blocks" {
            blocks = Some(PalettedContainer::new(BlockState::from_id(0).unwrap(), Strategy::for_block_states()));
            biomes = None;
        } else {
            blocks = None;
            biomes = Some(PalettedContainer::new(BiomeId(0), Strategy::for_biomes(size)));
        }
        loop {
            let line = lines.next().unwrap();
            if line == "ENDCASE" {
                break;
            }
            if let Some(rest) = line.strip_prefix("SET ") {
                let v: Vec<i32> = rest.split(' ').map(|p| p.parse().unwrap()).collect();
                if let Some(c) = blocks.as_mut() {
                    c.set(v[0], v[1], v[2], BlockState::from_id(v[3] as u32).unwrap());
                } else {
                    biomes.as_mut().unwrap().set(v[0], v[1], v[2], BiomeId(v[3] as u32));
                }
            } else {
                assert_eq!(line, "CHECK");
                let l: Vec<&str> = (0..6).map(|_| lines.next().unwrap()).collect();
                checks += 1;
                if let Some(c) = &blocks {
                    check_block!(c, l, block_codec, |v: BlockState| v.id(), format!("{ctx} #{checks}"));
                } else {
                    check_block!(biomes.as_ref().unwrap(), l, TestBiomeCodec, |v: BiomeId| v.0, format!("{ctx} #{checks}"));
                }
            }
        }
    }
    assert_eq!(checks, 69);
}
