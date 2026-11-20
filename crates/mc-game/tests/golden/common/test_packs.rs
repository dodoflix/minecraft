//! Data packs the registry-loading tests and the vanilla fixture generator share: valid packs (directory + zip) with new
//! entries and a tag `replace`, packs with unusable metadata, and a pack whose entries must fail the load.

use std::fs;
use std::io::Write;
use std::path::Path;

fn write(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

const VANILLA_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ref/src/data/minecraft");

fn vanilla_json(rel: &str) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(format!("{VANILLA_ROOT}/{rel}"))
            .expect("ref/ is missing: run scripts/setup-ref.sh"),
    )
    .unwrap()
}

/// The data packs of the "custom" oracle scenario: valid packs (directory + zip) with new entries and a tag `replace`,
/// plus packs with unusable metadata and non-pack entries.
pub fn write_custom_packs(world: &Path) {
    let dp = world.join("datapacks");
    fs::create_dir_all(&dp).unwrap();
    write(
        dp.join("good/pack.mcmeta"),
        r##"{"pack": {"description": "good pack", "min_format": 121, "max_format": 121}}"##,
    );
    let mut dim = vanilla_json("dimension_type/overworld.json");
    dim["min_y"] = 0.into();
    dim["height"] = 256.into();
    dim["logical_height"] = 128.into();
    dim["coordinate_scale"] = 2.5.into();
    dim["ambient_light"] = 0.25.into();
    dim["monster_spawn_light_level"] =
        serde_json::json!({"type": "minecraft:uniform", "min_inclusive": 1, "max_inclusive": 9});
    dim["attributes"]["minecraft:visual/sky_color"] = "#112233".into();
    dim["attributes"]["minecraft:visual/fog_end_distance"] = 100.into();
    write(
        dp.join("good/data/test/dimension_type/custom.json"),
        &dim.to_string(),
    );
    let mut biome = vanilla_json("worldgen/biome/plains.json");
    biome["temperature"] = 0.3.into();
    biome["downfall"] = 1.into();
    biome["effects"]["water_color"] = "#abcdef".into();
    biome["effects"]["grass_color_modifier"] = "dark_forest".into();
    write(
        dp.join("good/data/test/worldgen/biome/custom.json"),
        &biome.to_string(),
    );
    write(
        dp.join("good/data/test/damage_type/custom.json"),
        r#"{"message_id": "custom", "scaling": "always", "exhaustion": 0.25, "effects": "freezing", "death_message_type": "fall_variants"}"#,
    );
    write(
        dp.join("good/data/test/chat_type/custom.json"),
        r##"{"chat": {"translation_key": "chat.type.custom", "parameters": ["sender", "content"], "style": {"color": "red", "italic": true}}, "narration": {"translation_key": "n", "parameters": ["content"]}}"##,
    );
    write(
        dp.join("good/data/minecraft/tags/block/wool.json"),
        r#"{"replace": true, "values": ["minecraft:stone", {"id": "test:missing", "required": false}]}"#,
    );
    write(
        dp.join("good/data/minecraft/tags/block/mineable/pickaxe.json"),
        r##"{"values": ["#test:extra"]}"##,
    );
    write(
        dp.join("good/data/test/tags/block/extra.json"),
        r#"{"values": ["minecraft:dirt", "minecraft:dirt"]}"#,
    );
    write(
        dp.join("good/data/minecraft/tags/worldgen/biome/is_forest.json"),
        r#"{"values": ["test:custom"]}"#,
    );
    write_state_data(&dp.join("good/data/test"));
    // zip pack
    {
        let file = fs::File::create(dp.join("zipped.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("pack.mcmeta", opts).unwrap();
        zip.write_all(br#"{"pack": {"description": {"text": "zipped", "color": "gold"}, "min_format": 121, "max_format": [121, 5]}}"#).unwrap();
        zip.start_file("data/zip/damage_type/zipped.json", opts)
            .unwrap();
        zip.write_all(br#"{"message_id": "zipped", "scaling": "never", "exhaustion": 0.0}"#)
            .unwrap();
        zip.finish().unwrap();
    }
    write(dp.join("badmeta/pack.mcmeta"), "{ not json");
    write(
        dp.join("oldformat/pack.mcmeta"),
        r#"{"pack": {"description": "old", "pack_format": 5}}"#,
    );
    write(
        dp.join("badformat/pack.mcmeta"),
        r#"{"pack": {"description": "bad", "pack_format": 100}}"#,
    );
    write(
        dp.join("nodesc/pack.mcmeta"),
        r#"{"pack": {"pack_format": 48}}"#,
    );
    write(dp.join("nopack/pack.mcmeta"), r#"{"other": {}}"#);
    write(dp.join("plain/readme.txt"), "hello");
    write(dp.join("notes.txt"), "hello");
}

/// Block transformers, block state providers and an enchantment whose block states and providers must re-encode like the
/// vanilla codecs do: every provider type, states listed out of codec order, noise parameters with every optional field
/// and states with unknown or malformed properties (which the codec ignores).
fn write_state_data(root: &Path) {
    use serde_json::json;
    let stairs = json!({"id": "minecraft:oak_stairs", "properties": {"half": "top", "waterlogged": "true", "shape": "outer_left", "facing": "east"}});
    let door = json!({"id": "minecraft:oak_door", "properties": {"facing": "west", "half": "upper", "hinge": "right", "open": "true", "powered": "true"}});
    let wire = json!({"id": "minecraft:redstone_wire", "properties": {"east": "side", "north": "up", "power": "7", "south": "side", "west": "none"}});
    let noise = json!({"base_octave": -2, "base_amplitude": 2.5, "octave_count": 3, "normalize": false, "amplitude_modifiers": [1.0, 0.5, 0.25]});
    let providers = json!({
        "stairs": stairs,
        "simple_typed": {"type": "minecraft:simple", "state": stairs},
        "weighted": {"type": "minecraft:weighted", "entries": [
            {"data": stairs, "weight": 3}, {"data": "minecraft:stone", "weight": 1}, {"data": wire, "weight": 2}]},
        "dual": {"type": "minecraft:dual_noise", "variety": [1, 3], "slow_noise": {"base_octave": -10, "base_amplitude": 0.5},
            "slow_scale": 1.0, "seed": 2345, "noise": noise, "scale": 0.5,
            "states": [stairs, "minecraft:allium", wire]},
        "dual_single": {"type": "minecraft:dual_noise", "variety": 2, "slow_noise": {"base_octave": -4}, "slow_scale": 2.0,
            "seed": 7, "noise": {"base_octave": -3, "normalize": "legacy"}, "scale": 1.0, "states": [door]},
        "dual_object": {"type": "minecraft:dual_noise", "variety": {"min_inclusive": 1, "max_inclusive": 4},
            "slow_noise": {"base_octave": -4}, "slow_scale": 2.0, "seed": -7, "noise": {"base_octave": -3}, "scale": 1.0, "states": [door]},
        "noise": {"type": "minecraft:noise", "seed": -5, "noise": {"base_octave": -4, "normalize": "legacy"}, "scale": 2.0, "states": [door, stairs]},
        "noise_threshold": {"type": "minecraft:noise_threshold", "seed": 1, "noise": {"base_octave": -2}, "scale": 0.25,
            "threshold": -0.5, "high_chance": 0.25, "default_state": stairs, "low_states": [door], "high_states": ["minecraft:stone", wire]},
        "rotated": {"type": "minecraft:rotated", "state": {"type": "minecraft:simple", "state": stairs}, "direction": "north"},
        "rotated_reference": {"type": "minecraft:rotated", "state": "test:stairs"},
        "randomized_int": {"type": "minecraft:randomized_int", "source": {"id": "minecraft:wheat", "properties": {"age": "2"}},
            "property": "age", "values": {"type": "minecraft:uniform", "min_inclusive": 0, "max_inclusive": 7}},
        "randomized_int_constant": {"type": "minecraft:randomized_int", "source": {"id": "minecraft:wheat"}, "property": "age", "values": 3},
        "copy_properties": {"type": "minecraft:copy_properties", "source": door},
        "random_block": {"type": "minecraft:random_block", "blocks": ["minecraft:stone", "minecraft:dirt"]},
        "rule_based": {"type": "minecraft:rule_based", "fallback": stairs, "rules": [
            {"if_true": {"type": "minecraft:matching_blocks", "blocks": "minecraft:oak_log"},
             "then": {"type": "minecraft:copy_properties", "source": {"id": "minecraft:stripped_oak_log", "properties": {"axis": "y"}}}},
            {"if_true": {"type": "minecraft:would_survive", "state": stairs}, "then": "test:stairs"}]},
        "lenient_names": {"id": "minecraft:oak_stairs", "properties": {"facing": "nope", "nonexistent": "x", "half": "top"}},
        "lenient_types": {"id": "minecraft:oak_stairs", "properties": {"half": 5, "waterlogged": true, "shape": "outer_left"}},
        "lenient_number": {"id": "minecraft:redstone_wire", "properties": {"power": 7, "north": "up", "east": "bogus"}},
        "lenient_object": {"id": "minecraft:oak_stairs", "properties": "nope"},
    });
    for (name, provider) in providers.as_object().unwrap() {
        write(
            root.join(format!("worldgen/block_state_provider/{name}.json")),
            &provider.to_string(),
        );
    }
    let transformer = json!([
        {"block_state_provider": providers["rule_based"], "sound": "minecraft:item.axe.strip", "particle": "scrape",
         "disallowed_faces": ["down", "up"], "loot": "minecraft:till/rooted_dirt", "drop_strategy": "clicked_face",
         "update_from_neighbors": false, "transform_type": "copper_chest", "consume_on_use": false, "item_damage_per_use": 2},
        {"block_state_provider": door},
        {"block_state_provider": "test:weighted", "particle": "wax_off", "sound": {"sound_id": "minecraft:item.axe.strip", "range": 8.0}},
    ]);
    write(
        root.join("block_transformer/custom_axe.json"),
        &transformer.to_string(),
    );
    let mut enchantment = vanilla_json("enchantment/frost_walker.json");
    let effects = enchantment["effects"]["minecraft:location_changed"]
        .as_array_mut()
        .unwrap();
    effects[0]["effect"]["block_state"] = providers["weighted"].clone();
    effects.push(
        json!({"effect": {"type": "minecraft:replace_block", "offset": [0, -1, 0],
        "block_state": providers["dual"], "trigger_game_event": "minecraft:block_place"}}),
    );
    effects.push(json!({"effect": {"type": "minecraft:replace_block", "offset": [0, -1, 0], "block_state": door}}));
    write(
        root.join("enchantment/custom_frost.json"),
        &enchantment.to_string(),
    );
}

/// A world whose only pack contains entries that must fail the load.
pub fn write_broken_packs(world: &Path) {
    let dp = world.join("datapacks");
    write(
        dp.join("broken/pack.mcmeta"),
        r##"{"pack": {"description": "broken", "min_format": 121, "max_format": 121}}"##,
    );
    write(
        dp.join("broken/data/test/damage_type/bad_type.json"),
        r#"{"message_id": 5, "scaling": "sideways", "exhaustion": "x"}"#,
    );
    write(
        dp.join("broken/data/test/damage_type/bad_syntax.json"),
        "{\"message_id\": ",
    );
    write(
        dp.join("broken/data/test/chat_type/missing_field.json"),
        r#"{"chat": {"translation_key": "x", "parameters": []}}"#,
    );
    write(
        dp.join("broken/data/test/worldgen/block_state_provider/unknown_type.json"),
        r#"{"type": "minecraft:nope"}"#,
    );
    write(
        dp.join("broken/data/test/worldgen/block_state_provider/unknown_block.json"),
        r#"{"id": "minecraft:nope"}"#,
    );
    write(
        dp.join("broken/data/test/worldgen/block_state_provider/empty_weighted.json"),
        r#"{"type": "minecraft:weighted", "entries": []}"#,
    );
    write(
        dp.join("broken/data/test/worldgen/block_state_provider/bad_noise.json"),
        r#"{"type": "minecraft:noise", "seed": 1, "noise": {"base_octave": 99}, "scale": 0, "states": ["minecraft:stone"]}"#,
    );
    write(
        dp.join("broken/data/test/block_transformer/empty.json"),
        "[]",
    );
    write(
        dp.join("broken/data/test/block_transformer/bad_face.json"),
        r#"[{"block_state_provider": {"id": "minecraft:stone"}, "disallowed_faces": ["sideways"]}]"#,
    );
    write(
        dp.join("broken/data/test/block_transformer/missing_reference.json"),
        r#"[{"block_state_provider": "test:missing"}]"#,
    );
    write(
        dp.join("broken/data/test/worldgen/block_state_provider/bad_default_state.json"),
        r#"{"type": "minecraft:noise_threshold", "seed": 1, "noise": {"base_octave": -2}, "scale": 1.0, "threshold": 0.5, "high_chance": 0.5, "default_state": "minecraft:nope", "low_states": [{"id": "minecraft:stone", "properties": 3}], "high_states": []}"#,
    );
}
