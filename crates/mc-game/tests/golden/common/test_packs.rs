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
        "simple_unprefixed": {"type": "simple", "state": stairs},
        "random_block_unprefixed": {"type": "random_block", "blocks": ["minecraft:stone"]},
        "state_with_unknown_type": {"id": "minecraft:stone", "type": "minecraft:nope"},
        "typed_with_unknown_block": {"id": "minecraft:nope", "type": "simple", "state": stairs},
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
    // dimensions with inline noise settings (the ones of the world preset below): the `dimension` registry decodes them
    for (id, stem) in dimension_preset()["dimensions"].as_object().unwrap() {
        if let Some(name) = id.strip_prefix("test:") {
            write(
                root.join(format!("dimension/{name}.json")),
                &stem.to_string(),
            );
        }
    }
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
    write_broken_state_data(&dp.join("broken/data/test"));
}

/// Provider and transformer entries that fail in different ways: the error texts (list sizes, appended checks, the
/// `either` wrappers, partial results) must be the ones the codecs produce.
fn write_broken_state_data(root: &Path) {
    let stone = r#"{"id": "minecraft:stone"}"#;
    let nope = r#"{"id": "minecraft:nope"}"#;
    let noise = |params: &str| {
        format!(
            r#"{{"type": "minecraft:noise", "seed": 1, "noise": {params}, "scale": 1.0, "states": [{stone}]}}"#
        )
    };
    let dual = |variety: &str| {
        format!(
            r#"{{"type": "minecraft:dual_noise", "variety": {variety}, "slow_noise": {{"base_octave": -4}}, "slow_scale": 1.0, "seed": 1, "noise": {{"base_octave": -2}}, "scale": 1.0, "states": [{stone}]}}"#
        )
    };
    let weighted =
        |entries: &str| format!(r#"{{"type": "minecraft:weighted", "entries": {entries}}}"#);
    let states = |list: &str| {
        format!(
            r#"{{"type": "minecraft:noise", "seed": 1, "noise": {{"base_octave": -2}}, "scale": 1.0, "states": {list}}}"#
        )
    };
    let modifiers = |count: u32, list: &str| {
        noise(&format!(
            r#"{{"base_octave": -2, "octave_count": {count}, "amplitude_modifiers": {list}}}"#
        ))
    };
    let long_modifiers = format!("[{}]", vec!["1.0"; 33].join(", "));
    let providers: Vec<(&str, String)> = vec![
        ("simple_unprefixed_bad_state", format!(r#"{{"type": "simple", "state": {nope}}}"#)),
        ("unknown_type_unprefixed", r#"{"type": "nope"}"#.to_owned()),
        ("type_not_a_string", r#"{"type": 5}"#.to_owned()),
        ("type_bad_identifier", r#"{"type": "Not Valid"}"#.to_owned()),
        ("both_alternatives_simple", r#"{"id": "minecraft:stone", "type": "simple", "state": {"id": "minecraft:dirt"}}"#.to_owned()),
        ("both_alternatives_random_block", r#"{"id": "minecraft:stone", "type": "minecraft:random_block", "blocks": ["minecraft:dirt"]}"#.to_owned()),
        ("both_alternatives_properties", r#"{"id": "minecraft:oak_stairs", "properties": {"half": "top"}, "type": "minecraft:simple", "state": {"id": "minecraft:oak_stairs", "properties": {"facing": "west"}}}"#.to_owned()),
        ("both_alternatives_random_block_list", r#"{"id": "minecraft:stone", "type": "minecraft:random_block", "blocks": ["minecraft:dirt", "minecraft:oak_log"]}"#.to_owned()),
        ("both_alternatives_rotated", r#"{"id": "minecraft:stone", "type": "minecraft:rotated", "state": {"id": "minecraft:dirt"}, "direction": "north"}"#.to_owned()),
        ("both_alternatives_rotated_inline_typed", r#"{"id": "minecraft:stone", "type": "minecraft:rotated", "state": {"type": "minecraft:random_block", "blocks": "minecraft:dirt"}}"#.to_owned()),
        ("both_alternatives_copy_properties", r#"{"id": "minecraft:oak_log", "properties": {"axis": "x"}, "type": "minecraft:copy_properties", "source": {"id": "minecraft:oak_stairs"}}"#.to_owned()),
        ("both_alternatives_rule_based", r#"{"id": "minecraft:stone", "type": "minecraft:rule_based", "rules": []}"#.to_owned()),
        ("weighted_zero", weighted(&format!(r#"[{{"data": {stone}, "weight": 0}}]"#))),
        ("weighted_bad_entry", weighted(&format!(r#"[{{"data": {nope}, "weight": 1}}]"#))),
        ("weighted_bad_and_good_entry", weighted(&format!(r#"[{{"data": {nope}, "weight": 1}}, {{"data": {stone}, "weight": 2}}]"#))),
        ("weighted_negative", weighted(&format!(r#"[{{"data": {stone}, "weight": -1}}]"#))),
        ("weighted_overflow", weighted(&format!(r#"[{{"data": {stone}, "weight": 2147483647}}, {{"data": {stone}, "weight": 1}}]"#))),
        ("weighted_two_bad_entries", weighted(&format!(r#"[{{"data": {nope}, "weight": 1}}, {{"data": {stone}, "weight": "x"}}]"#))),
        ("states_empty", states("[]")),
        ("states_bad_entry", states(&format!("[{nope}]"))),
        ("states_bad_and_good_entry", states(&format!("[{nope}, {stone}]"))),
        ("variety_bad_type", dual(r#""x""#)),
        ("variety_list_wrong_size", dual("[1]")),
        ("variety_list_bad_element", dual(r#"[1, "x"]"#)),
        ("variety_object_missing_key", dual(r#"{"min_inclusive": 1}"#)),
        ("variety_inverted", dual("[3, 2]")),
        ("variety_too_low", dual("0")),
        ("variety_too_high", dual("[1, 65]")),
        ("normalize_bad_type", noise(r#"{"base_octave": -2, "normalize": 5}"#)),
        ("normalize_bad_string", noise(r#"{"base_octave": -2, "normalize": "nope"}"#)),
        ("amplitude_wrong_size", modifiers(2, "[1.0]")),
        ("amplitude_bad_element", modifiers(1, r#"["x"]"#)),
        ("amplitude_bad_element_and_size", modifiers(1, r#"["x", 0.5, 0.5]"#)),
        ("amplitude_too_long", modifiers(32, &long_modifiers)),
        ("amplitude_out_of_range", modifiers(1, "[2000000.0]")),
    ];
    for (name, json) in &providers {
        write(
            root.join(format!("worldgen/block_state_provider/{name}.json")),
            json,
        );
    }
    let long_transformer = format!(
        "[{}]",
        vec![format!(r#"{{"block_state_provider": {stone}}}"#); 201].join(", ")
    );
    let transformers = [
        ("too_long", long_transformer),
        (
            "element_without_provider",
            r#"[{"particle": "scrape"}]"#.to_owned(),
        ),
        ("element_not_an_object", "[5]".to_owned()),
        (
            "not_a_list",
            format!(r#"{{"block_state_provider": {stone}}}"#),
        ),
        (
            "bad_and_good_element",
            format!(r#"[5, {{"block_state_provider": {stone}}}]"#),
        ),
        (
            "bad_elements_mixed",
            format!(
                r#"[{{"block_state_provider": {stone}, "particle": "x"}}, {{"sound": 5}}, {{"block_state_provider": {stone}, "disallowed_faces": ["x"]}}]"#
            ),
        ),
        (
            "bad_faces",
            format!(r#"[{{"block_state_provider": {stone}, "disallowed_faces": ["x", "y"]}}]"#),
        ),
        (
            "bad_fields",
            format!(
                r#"[{{"block_state_provider": {stone}, "particle": "x", "drop_strategy": "y", "item_damage_per_use": -1}}]"#
            ),
        ),
    ];
    for (name, json) in &transformers {
        write(root.join(format!("block_transformer/{name}.json")), json);
    }
}

/// A `world_preset` (`test:custom`) whose dimensions carry inline noise settings: every vanilla one, the overworld's with
/// every density function and material rule reference expanded, and one that writes each codec in unusual forms (ints for
/// floats, unprefixed types, a `constant` function, defaults written out, all anchor kinds). A server started with
/// `level-type=test:custom` stores these dimensions in `world_gen_settings.dat`.
pub fn write_dimension_pack(world: &Path) {
    let dp = world.join("datapacks");
    write(
        dp.join("dimensions/pack.mcmeta"),
        r##"{"pack": {"description": "dimensions", "min_format": 121, "max_format": 121}}"##,
    );
    write(
        dp.join("dimensions/data/test/worldgen/world_preset/custom.json"),
        &dimension_preset().to_string(),
    );
}

fn dimension_preset() -> serde_json::Value {
    use serde_json::json;
    let stem = |dimension_type: &str, settings: serde_json::Value| {
        json!({"type": dimension_type, "generator": {"type": "minecraft:noise", "settings": settings,
            "biome_source": {"type": "minecraft:fixed", "biome": "minecraft:plains"}}})
    };
    let settings = |name: &str| vanilla_json(&format!("worldgen/noise_settings/{name}.json"));
    let mut overworld = settings("overworld");
    expand_references(&mut overworld);
    let mut nether = settings("nether");
    expand_references(&mut nether);
    let mut dimensions = serde_json::Map::new();
    let mut add = |id: &str, value: serde_json::Value| {
        dimensions.insert(id.to_owned(), value);
    };
    add(
        "minecraft:overworld",
        stem("minecraft:overworld", overworld),
    );
    add(
        "minecraft:the_nether",
        stem("minecraft:the_nether", settings("nether")),
    );
    add(
        "minecraft:the_end",
        stem("minecraft:the_end", settings("end")),
    );
    add(
        "test:amplified",
        stem("minecraft:overworld", settings("amplified")),
    );
    add(
        "test:large_biomes",
        stem("minecraft:overworld", settings("large_biomes")),
    );
    add("test:caves", stem("minecraft:overworld", settings("caves")));
    add(
        "test:floating_islands",
        stem("minecraft:overworld", settings("floating_islands")),
    );
    add("test:nether_expanded", stem("minecraft:the_nether", nether));
    add(
        "test:synthetic",
        stem("minecraft:overworld", synthetic_noise_settings()),
    );
    json!({"dimensions": dimensions})
}

/// Replaces the density function, material rule and material condition references of `value` by the entries they name.
fn expand_references(value: &mut serde_json::Value) {
    use serde_json::Value;
    const FUNCTIONS: [&str; 33] = [
        "input",
        "left",
        "right",
        "alpha",
        "first",
        "second",
        "base",
        "exponent",
        "coordinate",
        "density",
        "upper_bound",
        "richness",
        "filler_gap",
        "when_in_range",
        "when_out_of_range",
        "multiple",
        "shift_x",
        "shift_y",
        "shift_z",
        "temperature",
        "vegetation",
        "continents",
        "erosion",
        "depth",
        "ridges",
        "chunk_surface_level",
        "final_density",
        "barrier",
        "fluid_level_floodedness",
        "fluid_level_spread",
        "lava",
        "exclusion",
        "surface_level",
    ];
    let load = |registry: &str, id: &str| {
        let path = id.strip_prefix("minecraft:").expect("vanilla reference");
        vanilla_json(&format!("worldgen/{registry}/{path}.json"))
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let registry = match key.as_str() {
                    "function" => Some("density_function"),
                    k if FUNCTIONS.contains(&k) => Some("density_function"),
                    "material_rule" | "then_run" => Some("material_rule"),
                    "if_true" | "invert" => Some("material_condition"),
                    _ => None,
                };
                if let (Some(registry), Value::String(id)) = (registry, &*child) {
                    *child = load(registry, id);
                }
                if key == "sequence"
                    && let Value::Array(rules) = child
                {
                    for rule in rules.iter_mut() {
                        if let Value::String(id) = &*rule {
                            *rule = load("material_rule", id);
                        }
                    }
                }
                expand_references(child);
            }
        }
        Value::Array(list) => list.iter_mut().for_each(expand_references),
        _ => {}
    }
}

/// Noise settings in forms the vanilla data does not use.
fn synthetic_noise_settings() -> serde_json::Value {
    use serde_json::json;
    let stone = json!({"type": "block", "result_state": "minecraft:stone"});
    let tree = json!({
        "type": "add",
        "left": {"type": "floor", "input": 0.25},
        "right": {"type": "minecraft:mul",
            "left": {"type": "round", "input": 1, "multiple": {"type": "minecraft:constant", "value": 2}},
            "right": {"type": "min", "left": {"type": "ceil", "input": -1.5, "multiple": 1.0},
                "right": {"type": "max", "left": {"type": "truncate", "input": 3, "multiple": "minecraft:y"},
                    "right": {"type": "sub", "left": {"type": "div", "left": 4, "right": 2},
                        "right": {"type": "pow", "base": 2, "exponent": {"type": "sign", "input": {"type": "log",
                            "input": {"type": "sqrt", "input": {"type": "reciprocal", "input": {"type": "negate",
                            "input": {"type": "cube", "input": {"type": "square", "input": {"type": "abs", "input": "minecraft:y"}}}}}}}}}}}}}
    });
    let selects = json!({
        "type": "interval_select", "input": "minecraft:y", "thresholds": [-0.5, 0, 1],
        "functions": [0.125, {"type": "range_choice", "input": 0, "min_inclusive": -1, "max_exclusive": 1,
            "when_in_range": {"type": "slice", "axis": "y", "coordinate": 64, "input": "minecraft:y"},
            "when_out_of_range": {"type": "distance_to_point", "point": [1, 2, 3.0], "metric": "manhattan"}},
            {"type": "find_top_surface", "density": "minecraft:y", "upper_bound": 320, "lower_bound": -64, "cell_height": 4},
            {"type": "gradient", "axis": "x", "tiling": "mirrored_repeat", "from_coordinate": 0, "to_coordinate": 16,
                "from_value": 1, "to_value": -1}]
    });
    let shaped = json!({
        "type": "minecraft:clamp", "min": -2, "max": 2,
        "input": {"type": "lerp", "alpha": {"type": "gradient", "axis": "y", "tiling": "clamp_to_edge", "from_coordinate": 0, "to_coordinate": 128, "from_value": 0, "to_value": 1},
            "first": {"type": "interpolated", "cell_size_xz": 4, "cell_size_y": 8, "input": {"type": "cache", "input": {"type": "blend_density", "input": "minecraft:y"}}},
            "second": {"type": "spline", "spline": {"coordinate": "minecraft:y", "points": [
                {"location": -1, "value": 0.5, "derivative": 0},
                {"location": 0.5, "value": {"coordinate": "minecraft:zero", "points": [{"location": 0, "value": 1, "derivative": 0.25}]}, "derivative": 1.5}]}}}
    });
    let blended = json!({"type": "minecraft:add",
        "left": {"type": "old_blended_noise", "xz_scale": 1, "y_scale": 2.5, "xz_factor": 80, "y_factor": 160, "smear_scale_multiplier": 4},
        "right": {"type": "add", "left": {"type": "minecraft:blend_offset"}, "right": {"type": "end_outer_islands"}}});
    let noise = json!({"type": "minecraft:add",
        "left": {"type": "minecraft:noise", "noise": "minecraft:surface", "xz_scale": 1, "y_scale": 0.5,
            "shift_x": "minecraft:shift_x", "shift_y": 0, "shift_z": {"type": "minecraft:shift_a", "noise": "minecraft:offset"}},
        "right": {"type": "add", "left": {"type": "shift_b", "noise": "minecraft:offset"}, "right": {"type": "shift", "noise": "minecraft:offset"}}});
    let rules = json!({"type": "minecraft:sequence", "sequence": [
        {"type": "block", "result_state": {"id": "minecraft:lava", "properties": {"level": "3"}}},
        {"type": "condition", "if_true": {"type": "not", "invert": {"type": "hole"}}, "then_run": {"type": "bandlands"}},
        {"type": "condition", "if_true": {"type": "vertical_gradient", "random_name": "bedrock_floor",
            "true_at_and_below": {"above_bottom": 0}, "false_at_and_above": {"absolute": 5}}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "y_above", "anchor": {"relative_to_sea_level": -2}, "surface_depth_multiplier": 1, "add_stone_depth": false}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "water", "offset": 0, "surface_depth_multiplier": -1, "add_stone_depth": true}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "stone_depth", "offset": 0, "add_surface_depth": false, "secondary_depth_range": 2, "surface_type": "ceiling"}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "noise_threshold", "noise": "minecraft:surface", "min_threshold": -1, "max_threshold": 0.5}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "noise_threshold", "noise": "surface", "min_threshold": -1.5, "max_threshold": 0.25, "is_3d": true}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "biome", "biome_is": ["minecraft:plains"]}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "biome", "biome_is": "minecraft:plains"}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "biome", "biome_is": "#minecraft:is_badlands"}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "temperature"}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "steep"}, "then_run": stone},
        {"type": "condition", "if_true": {"type": "above_preliminary_surface"}, "then_run": stone},
        {"type": "ore_vein", "ore_block": "minecraft:iron_ore", "raw_ore_block": {"id": "minecraft:raw_iron_block"},
            "filler_block": "minecraft:granite", "raw_ore_chance": 1, "density": 0.5, "richness": "minecraft:y", "filler_gap": 0}]});
    json!({
        "noise": {"min_y": -64, "height": 384},
        "default_block": {"id": "minecraft:oak_stairs", "properties": {"half": "top", "facing": "east"}},
        "default_fluid": {"id": "minecraft:water"},
        "noise_router": {
            "temperature": 0, "vegetation": 1, "continents": {"type": "constant", "value": 0.5}, "erosion": selects,
            "depth": shaped, "ridges": blended, "chunk_surface_level": noise, "final_density": tree},
        "material_rule": rules,
        "spawn_target": [{"minecraft:zero": 0, "minecraft:y": [-0.5, 0.5]}, {"minecraft:shift_x": {"min": -1, "max": 1}}, {"minecraft:y": [0.25, 0.25]}, {}],
        "sea_level": 63.0,
        "disable_mob_generation": false,
        "aquifers": {"barrier": 0, "fluid_level_floodedness": 0.5, "fluid_level_spread": "minecraft:zero",
            "lava": {"type": "constant", "value": -1}, "exclusion": "minecraft:y", "surface_level": 1},
        "legacy_random_source": true,
        "debug_functions": [{"label": "x", "function": 0.25}]
    })
}
