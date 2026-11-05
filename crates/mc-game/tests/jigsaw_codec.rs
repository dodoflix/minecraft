//! Codecs of the jigsaw system against the vanilla codecs (`tests/oracle/JigsawCodecOracle.java` over
//! `fixtures/jigsaw_codec_cases.in`): accept/reject of alias bindings, dimension padding, max distance, pool elements, template
//! pools and jigsaw structures, the NBT encoding of accepted pool elements, and a digest of every vanilla template pool.

mod common;

use common::feature_harness::Fnv;
use mc_game::world::level::levelgen::codec::decode_dispatch;
use mc_game::world::level::levelgen::registries::{Cells, DecodeCtx, load_vanilla_data};
use mc_game::world::level::levelgen::structure::pools::alias::PoolAliasBinding;
use mc_game::world::level::levelgen::structure::pools::dimension_padding::DimensionPadding;
use mc_game::world::level::levelgen::structure::pools::jigsaw_placement::MaxDistance;
use mc_game::world::level::levelgen::structure::pools::structure_pool_element::{ElementCodecCtx, decode_element, encode_element};
use mc_game::world::level::levelgen::structure::pools::structure_template_pool::{StructureTemplatePool, load_template_pools};
use mc_game::world::level::levelgen::structure::structure::STRUCTURE_TYPES;
use mc_game::world::level::levelgen::structure::templatesystem::structure_processor_list::load_processor_lists;
use mc_nbt::Tag;
use mc_worldgen::data::{FsDataSource, WorldgenDataSource};
use std::sync::Arc;

fn vanilla_source() -> Arc<dyn WorldgenDataSource> {
    Arc::new(FsDataSource::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data")))
}

/// Mirror of `JigsawCodecOracle.canon`.
pub fn canon(tag: &Tag) -> String {
    match tag {
        Tag::Compound(c) => {
            let mut keys: Vec<&String> = c.keys().collect();
            keys.sort();
            format!("{{{}}}", keys.iter().map(|k| format!("{k}:{}", canon(c.get(k).unwrap()))).collect::<Vec<_>>().join(","))
        }
        Tag::List(l) => format!("[{}]", l.iter().map(canon).collect::<Vec<_>>().join(",")),
        Tag::String(s) => format!("\"{s}\""),
        Tag::Byte(v) => format!("{v}b"),
        Tag::Short(v) => format!("{v}s"),
        Tag::Int(v) => format!("{v}i"),
        Tag::Long(v) => format!("{v}l"),
        other => panic!("unsupported tag {other:?}"),
    }
}

#[test]
fn codec_cases_match_vanilla() {
    let (worldgen, biomes, _) = load_vanilla_data();
    let cells = Cells::new(vanilla_source());
    let ctx = DecodeCtx { worldgen: &worldgen, biomes: &biomes, cells: &cells };
    let element_ctx = ElementCodecCtx::Json(&ctx);
    let golden = include_str!("fixtures/jigsaw_codec_cases.golden");
    let mut wrong = Vec::new();
    let mut total = 0;
    for line in golden.lines().filter(|l| !l.starts_with("POOL\t")) {
        let mut parts = line.split('\t');
        let (kind, json, expected) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let expected_canon = parts.next();
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let (ok, encoded) = match kind {
            "alias" => (PoolAliasBinding::from_json(&value).is_ok(), None),
            "padding" => (DimensionPadding::from_json(&value).is_ok(), None),
            "maxdist" => (MaxDistance::from_json(&value).is_ok(), None),
            "element" => match decode_element(&value, &element_ctx) {
                Ok(element) => (true, Some(canon(&Tag::Compound(encode_element(&*element))))),
                Err(_) => (false, None),
            },
            "pool" => (StructureTemplatePool::decode_direct(&value, &ctx).is_ok(), None),
            "structure" => (decode_dispatch(&value, &ctx, &STRUCTURE_TYPES).is_ok(), None),
            other => panic!("unknown kind {other}"),
        };
        total += 1;
        let actual = if ok { "ok" } else { "err" };
        if actual != expected || encoded.as_deref() != expected_canon {
            wrong.push(format!("{kind} {json}\n  vanilla: {expected} {expected_canon:?}\n  ours:    {actual} {encoded:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {total} cases differ:\n{}", wrong.len(), wrong.join("\n"));
}

/// All 245 vanilla template pools decode, with the elements, weights and fallbacks vanilla has (digest of the NBT encodings).
#[test]
fn vanilla_template_pools_match() {
    let (worldgen, biomes, _) = load_vanilla_data();
    let cells = Cells::new(vanilla_source());
    let ctx = DecodeCtx { worldgen: &worldgen, biomes: &biomes, cells: &cells };
    load_processor_lists(&ctx).expect("processor lists");
    let pools = load_template_pools(&ctx).expect("template pools");
    let golden: Vec<&str> = include_str!("fixtures/jigsaw_codec_cases.golden").lines().filter(|l| l.starts_with("POOL\t")).collect();
    assert_eq!(pools.len(), 245);
    assert_eq!(golden.len(), pools.len());
    let mut wrong = Vec::new();
    for line in golden {
        let parts: Vec<&str> = line.split('\t').collect();
        let id = mc_core::ident::Identifier::parse(parts[1]).unwrap();
        let pool = pools.get(&id).unwrap_or_else(|| panic!("pool {id}")).value();
        let mut f = Fnv::default();
        for (element, weight) in pool.get_templates() {
            f.s(&format!("{weight}|{}", canon(&Tag::Compound(encode_element(&**element)))));
        }
        let ours = format!("{}\t{}\t{}", pool.get_fallback().id().unwrap(), pool.get_templates().len(), f.hex());
        let vanilla = format!("{}\t{}\t{}", parts[2], parts[3], parts[4]);
        if ours != vanilla {
            wrong.push(format!("{id}: vanilla {vanilla}, ours {ours}"));
        }
    }
    assert!(wrong.is_empty(), "{} pools differ:\n{}", wrong.len(), wrong.join("\n"));
}
