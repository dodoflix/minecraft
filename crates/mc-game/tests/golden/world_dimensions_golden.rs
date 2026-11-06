//! `LevelStem.CODEC` as the dimension registry and `world_gen_settings.dat` use it, against vanilla: for every crafted
//! stem (presets, inline dimension types, inline-biome flat settings, inline biomes in biome sources and malformed
//! variants of each) the stem is accepted iff vanilla accepts it and re-encodes to the very same NBT.
//! Fixture `tests/fixtures/world_dimensions_stems.txt`, generator `tests/oracle/WorldDimensionsOracle.java`.
//! Also the `WorldDimensions` lifecycle of the game layer: preset -> stored NBT -> decode -> bake.

use crate::common;

use std::path::PathBuf;

use mc_core::ident::Identifier;
use mc_game::registry::keys;
use mc_game::server::world_dimensions::{DimensionDecoder, WorldDimensions};
use mc_world::level_storage::{SpecialWorldProperty, WorldDataConfiguration};
use mc_worldgen::world_preset::{Holder, Lifecycle};

fn fixture() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world_dimensions_stems.txt");
    std::fs::read_to_string(path).unwrap()
}

/// The first position where two SNBT strings differ, with context.
fn first_difference(ours: &str, vanilla: &str) -> String {
    let at = ours
        .bytes()
        .zip(vanilla.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let window = |s: &str| {
        s.get(at.saturating_sub(60)..(at + 80).min(s.len()))
            .unwrap_or("")
            .to_owned()
    };
    format!(
        "at {at}\n  ours:    ...{}\n  vanilla: ...{}",
        window(ours),
        window(vanilla)
    )
}

#[test]
fn level_stems_match_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true).unwrap();
    let access = stem.registries.composite_access();
    let decoder = DimensionDecoder::new(access).unwrap();
    let text = fixture();
    let mut lines = text.lines();
    let (mut accepted, mut rejected) = (0, 0);
    let mut failures = Vec::new();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("STEM ") else {
            continue;
        };
        let (name, json) = rest.split_once(' ').unwrap();
        let want = lines
            .next()
            .unwrap()
            .strip_prefix(&format!("ENC {name} "))
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(json).unwrap();
        let got = decoder.stem(&json);
        match (want.strip_prefix("OK "), got) {
            (Some(snbt), Ok((_, tag))) => {
                let want_tag =
                    mc_nbt::snbt::parse(snbt).unwrap_or_else(|e| panic!("{name}: {e:?}"));
                // `noenc:` cases: inline noise settings are re-encoded from raw JSON (narrowed number tags), and vanilla keeps
                // the partial result of an inline biome with an unresolvable feature; only acceptance is compared
                if tag != want_tag && !name.starts_with("noenc:") {
                    failures.push(format!(
                        "{name}: re-encode differs {}",
                        first_difference(&tag.to_string(), &want_tag.to_string())
                    ));
                }
                accepted += 1;
            }
            (None, Err(_)) => rejected += 1,
            (Some(_), Err(e)) => failures.push(format!("{name}: vanilla accepts, we reject: {e}")),
            (None, Ok(_)) => failures.push(format!("{name}: vanilla rejects, we accept")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!((accepted, rejected), (97, 63));
}

#[test]
fn world_dimensions_lifecycle() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true).unwrap();
    let access = stem.registries.composite_access();
    let decoder = DimensionDecoder::new(access).unwrap();
    let preset = |name: &str| {
        let entry = access
            .registry(keys::WORLD_PRESET)
            .unwrap()
            .entry_by_key(&Identifier::parse(name).unwrap())
            .unwrap()
            .json
            .clone();
        WorldDimensions::from_preset_json(&decoder, &entry).unwrap()
    };

    let normal = preset("minecraft:normal");
    let baked = normal.bake(&decoder, None).unwrap();
    assert_eq!(baked.special_world_property, SpecialWorldProperty::None);
    assert_eq!(baked.lifecycle, Lifecycle::Stable);
    let dimensions = baked.dimensions.registry(keys::LEVEL_STEM).unwrap();
    assert_eq!(
        dimensions
            .names()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end"
        ]
    );
    assert!(
        dimensions
            .entries()
            .iter()
            .all(|e| e.info.stable && e.info.known_pack.is_none())
    );
    let overworld = baked
        .dimensions
        .level_stem(&Identifier::parse("minecraft:overworld").unwrap())
        .unwrap();
    assert!(
        matches!(&overworld.dimension_type, Holder::Reference(id, t) if id.to_string() == "minecraft:overworld" && t.height == 384)
    );

    // stored NBT -> decode -> the same dimensions
    let stored = normal.to_stored();
    let back = WorldDimensions::from_stored(&decoder, &stored).unwrap();
    assert_eq!(back.to_stored(), stored);
    assert_eq!(
        back.bake(&decoder, None)
            .unwrap()
            .dimensions
            .registry(keys::LEVEL_STEM)
            .unwrap()
            .names(),
        dimensions.names()
    );

    // the flat preset: special world property, the other two dimensions stay stable
    let flat = preset("minecraft:flat").bake(&decoder, None).unwrap();
    assert_eq!(flat.special_world_property, SpecialWorldProperty::Flat);
    assert_eq!(flat.lifecycle, Lifecycle::Stable);

    // an overworld that is not vanilla-like registers as experimental
    let mut custom = stored.clone();
    let mut overworld_stem = custom.get("minecraft:overworld").unwrap().clone();
    if let mc_nbt::Tag::Compound(c) = &mut overworld_stem {
        c.put(
            "type",
            mc_nbt::Tag::String("minecraft:the_nether".to_owned()),
        );
    }
    custom.put("minecraft:overworld", overworld_stem);
    let custom = WorldDimensions::from_stored(&decoder, &custom)
        .unwrap()
        .bake(&decoder, None)
        .unwrap();
    assert_eq!(custom.lifecycle, Lifecycle::Experimental);
    let ow = Identifier::parse("minecraft:overworld").unwrap();
    assert!(
        !custom
            .dimensions
            .registry(keys::LEVEL_STEM)
            .unwrap()
            .registration_info(&ow)
            .unwrap()
            .stable
    );

    // a missing overworld is rejected like `new WorldDimensions(map)`
    let mut no_overworld = stored;
    no_overworld.remove("minecraft:overworld");
    assert!(WorldDimensions::from_stored(&decoder, &no_overworld).is_err());
}
