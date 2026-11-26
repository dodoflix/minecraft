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
    // spawner lists with failing entries: the list keeps the entries that decoded (`promotePartial`) and the load goes on
    let mut partial_spawns = biome.clone();
    let spawns = &mut partial_spawns["attributes"]["minecraft:gameplay/natural_mob_spawns"]["argument"]
        ["spawns_by_category"];
    spawns["creature"] = serde_json::json!([
        {"type": "minecraft:sheep", "count": 4, "weight": 12},
        {"type": "minecraft:nope", "count": 4, "weight": 10},
        {"type": "minecraft:pig", "count": 4, "weight": 0},
        {"type": "minecraft:cow", "count": "x", "weight": 1},
        {"type": "minecraft:chicken", "count": 2}
    ]);
    spawns["monster"] =
        serde_json::json!([{"type": "minecraft:zombie", "count": 4, "weight": 5}, 7]);
    spawns["ambient"] = serde_json::json!([{"count": 4}]);
    // a `MISC` category type becomes a pig in `SpawnerData`
    spawns["misc"] = serde_json::json!([
        {"type": "minecraft:arrow", "count": 2, "weight": 3},
        {"type": "minecraft:wolf", "count": 1, "weight": 2}
    ]);
    write(
        dp.join("good/data/test/worldgen/biome/partial_spawns.json"),
        &partial_spawns.to_string(),
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
    write_game_test_data(&dp.join("good/data/test"));
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
    // inline noise definitions inside density functions: the entries load (a noise without a key only fails once a
    // sampler is built for it)
    let inline_noise = json!({"type": "minecraft:add",
        "left": {"type": "minecraft:noise", "noise": {"base_octave": -2, "base_amplitude": 2.0}, "xz_scale": 1, "y_scale": 0.5,
            "shift_x": {"type": "minecraft:shift_a", "noise": {"base_octave": -3}}},
        "right": {"type": "minecraft:shift_b", "noise": {"base_octave": -4, "normalize": "legacy"}}});
    write(
        root.join("worldgen/density_function/inline_noise.json"),
        &inline_noise.to_string(),
    );
    let mut settings = vanilla_json("worldgen/noise_settings/overworld.json");
    settings["noise_router"]["final_density"] = inline_noise;
    settings["noise_router"]["erosion"] = "test:inline_noise".into();
    write(
        root.join("worldgen/noise_settings/inline_noise.json"),
        &settings.to_string(),
    );
}

/// Test environments and test instances: every environment type, references and inline definitions, defaults written out
/// and left out.
fn write_game_test_data(root: &Path) {
    use serde_json::json;
    let environments = [
        (
            "all_types",
            json!({"type": "minecraft:all_of", "definitions": [
                "minecraft:default",
                {"type": "minecraft:clock_time", "clock": "minecraft:overworld", "time": 6000},
                {"type": "minecraft:difficulty", "difficulty": "hard"},
                {"type": "minecraft:function", "setup": "test:setup", "teardown": "test:teardown"},
                {"type": "minecraft:game_rules", "rules": {"minecraft:pvp": false, "minecraft:max_entity_cramming": 5}},
                {"type": "minecraft:timeline_attributes", "timelines": ["minecraft:day", "minecraft:moon"]},
                {"type": "minecraft:weather", "weather": "thunder"},
                {"type": "minecraft:all_of", "definitions": []}
            ]}),
        ),
        ("function_empty", json!({"type": "minecraft:function"})),
        (
            "function_setup",
            json!({"type": "minecraft:function", "setup": "test:setup"}),
        ),
        (
            "rules_empty",
            json!({"type": "minecraft:game_rules", "rules": {}}),
        ),
    ];
    for (name, definition) in environments {
        write(
            root.join(format!("test_environment/{name}.json")),
            &definition.to_string(),
        );
    }
    let instances = [
        (
            "defaults",
            json!({"type": "minecraft:function", "function": "minecraft:always_pass", "environment": "minecraft:default",
                "structure": "minecraft:empty", "max_ticks": 100}),
        ),
        (
            "written_defaults",
            json!({"type": "minecraft:block_based", "environment": "test:all_types", "dimension": "minecraft:overworld",
                "structure": "test:written", "max_ticks": 20, "setup_ticks": 0, "required": true, "rotation": "none",
                "manual_only": false, "max_attempts": 1, "required_successes": 1, "sky_access": false, "padding": 0}),
        ),
        (
            "everything",
            json!({"type": "minecraft:function", "function": "test:everything",
                "environment": {"type": "minecraft:all_of", "definitions": [
                    {"type": "minecraft:weather", "weather": "clear"}, "test:function_setup"]},
                "dimension": "minecraft:the_nether", "structure": "test:everything", "max_ticks": 400, "setup_ticks": 7,
                "required": false, "rotation": "counterclockwise_90", "manual_only": true, "max_attempts": 5,
                "required_successes": 3, "sky_access": true, "padding": 128}),
        ),
        (
            "rotated",
            json!({"type": "minecraft:block_based", "environment": "test:function_empty", "structure": "test:rotated",
                "max_ticks": 1, "rotation": "180"}),
        ),
    ];
    for (name, instance) in instances {
        write(
            root.join(format!("test_instance/{name}.json")),
            &instance.to_string(),
        );
    }
}

/// Number spellings Gson reads but the codecs treat differently from `f64`: entries that load.
pub fn write_number_edge_packs(world: &Path) {
    let dp = world.join("datapacks");
    write(
        dp.join("numbers/pack.mcmeta"),
        r##"{"pack": {"description": "numbers", "min_format": 121, "max_format": 121}}"##,
    );
    for (name, exhaustion) in [
        ("big", "1e400"),
        ("bigger", "1e10000"),
        ("negative", "-1e400"),
        ("tiny", "1e-400"),
        ("overflow", "1e99999999999"),
        ("huge_exponent", "1E+400"),
    ] {
        write(
            dp.join(format!("numbers/data/test/damage_type/{name}.json")),
            &format!(r#"{{"message_id": "n", "scaling": "always", "exhaustion": {exhaustion}}}"#),
        );
    }
    for (name, width) in [("fraction", "2.5"), ("exponent", "1e1"), ("float", "3.0e0")] {
        write(
            dp.join(format!("numbers/data/test/painting_variant/{name}.json")),
            &format!(r#"{{"asset_id": "minecraft:alban", "width": {width}, "height": 1}}"#),
        );
    }
}

/// Number spellings an int or double field cannot take: every entry fails (some with `NumberFormatException`).
pub fn write_broken_number_packs(world: &Path) {
    let dp = world.join("datapacks");
    write(
        dp.join("broken_numbers/pack.mcmeta"),
        r##"{"pack": {"description": "broken numbers", "min_format": 121, "max_format": 121}}"##,
    );
    let long_digits = format!("1{}", "0".repeat(10000));
    for (name, width) in [
        ("exp_10000", "1e10000"),
        ("exp_9999", "1e9999"),
        ("exp_400", "1e400"),
        ("exp_negative", "1e-400"),
        ("exp_overflow", "1e99999999999"),
        ("long_value", "10000000000"),
        ("long_digits", long_digits.as_str()),
    ] {
        write(
            dp.join(format!(
                "broken_numbers/data/test/painting_variant/{name}.json"
            )),
            &format!(r#"{{"asset_id": "minecraft:alban", "width": {width}, "height": 1}}"#),
        );
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
    write_broken_game_test_data(&dp.join("broken/data/test"));
    write_broken_light_levels(&dp.join("broken/data/test"));
}

/// Test environments and test instances that must fail the load.
fn write_broken_game_test_data(root: &Path) {
    use serde_json::json;
    let valid = json!({"type": "minecraft:block_based", "environment": "minecraft:default", "structure": "minecraft:empty",
        "max_ticks": 10});
    let environments = [
        ("env_unknown_type", json!({"type": "minecraft:nope"})),
        ("env_no_type", json!({"definitions": []})),
        (
            "env_missing_definitions",
            json!({"type": "minecraft:all_of"}),
        ),
        (
            "env_missing_reference",
            json!({"type": "minecraft:all_of", "definitions": ["test:missing"]}),
        ),
        (
            "env_nested_unknown_type",
            json!({"type": "minecraft:all_of", "definitions": [{"type": "minecraft:nope"}]}),
        ),
        (
            "env_bad_clock",
            json!({"type": "minecraft:clock_time", "clock": "minecraft:nope", "time": 1}),
        ),
        (
            "env_negative_time",
            json!({"type": "minecraft:clock_time", "clock": "minecraft:overworld", "time": -1}),
        ),
        (
            "env_bad_difficulty",
            json!({"type": "minecraft:difficulty", "difficulty": "impossible"}),
        ),
        (
            "env_bad_function",
            json!({"type": "minecraft:function", "setup": "Not Valid"}),
        ),
        (
            "env_bad_rules",
            json!({"type": "minecraft:game_rules", "rules": {"minecraft:nope": true, "minecraft:pvp": 3}}),
        ),
        (
            "env_rules_not_a_map",
            json!({"type": "minecraft:game_rules", "rules": []}),
        ),
        (
            "env_bad_timeline",
            json!({"type": "minecraft:timeline_attributes", "timelines": ["minecraft:day", "test:nope"]}),
        ),
        (
            "env_bad_weather",
            json!({"type": "minecraft:weather", "weather": "hail"}),
        ),
    ];
    for (name, definition) in environments {
        write(
            root.join(format!("test_environment/{name}.json")),
            &definition.to_string(),
        );
    }
    let mut instances = vec![
        ("inst_unknown_type", json!({"type": "minecraft:nope"})),
        ("inst_no_type", json!({"structure": "minecraft:empty"})),
        (
            "inst_function_missing_function",
            json!({"type": "minecraft:function", "environment": "minecraft:default", "structure": "minecraft:empty", "max_ticks": 1}),
        ),
        ("inst_empty", json!({"type": "minecraft:block_based"})),
    ];
    for (name, field, value) in [
        ("inst_zero_ticks", "max_ticks", json!(0)),
        ("inst_negative_setup", "setup_ticks", json!(-1)),
        ("inst_zero_attempts", "max_attempts", json!(0)),
        ("inst_zero_successes", "required_successes", json!(0)),
        ("inst_big_padding", "padding", json!(129)),
        ("inst_bad_rotation", "rotation", json!("sideways")),
        ("inst_bad_required", "required", json!("yes")),
        ("inst_bad_structure", "structure", json!("Not Valid")),
        ("inst_bad_dimension", "dimension", json!(5)),
        (
            "inst_missing_environment",
            "environment",
            json!("test:missing"),
        ),
        ("inst_fractional_ticks", "max_ticks", json!(2.5)),
    ] {
        let mut instance = valid.clone();
        instance[field] = value;
        instances.push((name, instance));
    }
    for (name, instance) in instances {
        write(
            root.join(format!("test_instance/{name}.json")),
            &instance.to_string(),
        );
    }
}

/// Dimension types whose `monster_spawn_light_level` provider fails with a partial result: the range check of
/// `IntProviders.codec(0, 15)` still sees the partial provider.
fn write_broken_light_levels(root: &Path) {
    let weighted = |entries: serde_json::Value| serde_json::json!({"type": "minecraft:weighted_list", "distribution": entries});
    let cases = [
        (
            "light_partial_too_high",
            weighted(serde_json::json!([{"data": 20, "weight": 1}, {"data": "x", "weight": 1}])),
        ),
        (
            "light_partial_in_range",
            weighted(serde_json::json!([{"data": 5, "weight": 1}, {"data": "x", "weight": 1}])),
        ),
        (
            "light_partial_too_low",
            weighted(serde_json::json!([{"data": -4, "weight": 1}, {"weight": 1}])),
        ),
        (
            "light_partial_empty",
            weighted(serde_json::json!([{"data": "x", "weight": 1}])),
        ),
        (
            "light_weighted_zero",
            weighted(serde_json::json!([{"data": 5, "weight": 0}])),
        ),
        ("light_weighted_empty", weighted(serde_json::json!([]))),
        (
            "light_partial_nested",
            serde_json::json!({"type": "minecraft:clamped", "min_inclusive": 0, "max_inclusive": 99, "source": weighted(
                serde_json::json!([{"data": 30, "weight": 1}, {"data": "x", "weight": 1}])
            )}),
        ),
    ];
    for (name, light) in cases {
        let mut dim = vanilla_json("dimension_type/overworld.json");
        dim["monster_spawn_light_level"] = light;
        write(
            root.join(format!("dimension_type/{name}.json")),
            &dim.to_string(),
        );
    }
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
        ("variety_list_too_long", dual("[1, 2, 3]")),
        ("variety_list_too_long_low", dual("[0, 1, 2]")),
        ("variety_list_too_long_high", dual("[1, 65, 2]")),
        ("variety_list_too_long_bad_third", dual(r#"[1, 2, "x"]"#)),
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
    write_broken_provider_data(root);
}

/// Providers and transformers whose errors travel through reference-or-inline holders, the provider `xor`, predicates
/// with offsets and anchors, and an enchantment whose provider throws.
fn write_broken_provider_data(root: &Path) {
    use serde_json::json;
    let stone = json!({"id": "minecraft:stone"});
    let bad_noise = json!({"type": "minecraft:noise", "seed": 1, "noise": {"base_octave": -2, "normalize": 5}, "scale": 1.0, "states": [stone]});
    let noise_ok =
        json!({"seed": 1, "noise": {"base_octave": -2}, "scale": 1.0, "states": [stone]});
    let with_type = |ty: &str, rest: serde_json::Value| {
        let mut provider = rest;
        provider["type"] = ty.into();
        provider
    };
    // the full-state alternative reads too (`id`), so the two alternatives both succeed
    let both = |ty: &str, rest: serde_json::Value| {
        let mut provider = with_type(ty, rest);
        provider["id"] = "minecraft:stone".into();
        provider
    };
    let one_of_each_predicate = json!([
        {"type": "minecraft:all_of", "predicates": [{"type": "minecraft:true"}]},
        {"type": "minecraft:any_of", "predicates": []},
        {"type": "minecraft:not", "predicate": {"type": "minecraft:true"}},
        {"type": "minecraft:matching_blocks", "blocks": "minecraft:stone"},
        {"type": "minecraft:matching_fluids", "fluids": ["minecraft:water"]},
        {"type": "minecraft:matching_block_tag", "tag": "minecraft:logs", "offset": [0, 1, 0]},
        {"type": "minecraft:matching_biomes", "biomes": "minecraft:plains"},
        {"type": "minecraft:has_sturdy_face", "direction": "up"},
        {"type": "minecraft:solid"},
        {"type": "minecraft:replaceable", "offset": [1, 2, 3]},
        {"type": "minecraft:inside_world_bounds"},
        {"type": "minecraft:would_survive", "state": stone},
        {"type": "minecraft:unobstructed", "offset": [4, 5, 6]},
        {"type": "minecraft:height_range", "min_inclusive": {"absolute": 1}, "max_inclusive": {"below_top": 2}},
        {"type": "minecraft:volume_match", "min": [-1, -2, -3], "max": [3, 2, 1], "match": {"type": "minecraft:solid"}},
    ]);
    let rules = |predicates: serde_json::Value, then: serde_json::Value| {
        json!(
            predicates
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json!({"if_true": p, "then": then}))
                .collect::<Vec<_>>()
        )
    };
    let rule_based =
        |rules: serde_json::Value| json!({"type": "minecraft:rule_based", "rules": rules});
    let predicate = |p: serde_json::Value| rule_based(rules(json!([p]), stone.clone()));
    let offset = |ty: &str, offset: serde_json::Value| {
        predicate(json!({"type": format!("minecraft:{ty}"), "offset": offset}))
    };
    let anchor = |anchor: serde_json::Value| {
        predicate(
            json!({"type": "minecraft:height_range", "min_inclusive": anchor, "max_inclusive": {"absolute": 1}}),
        )
    };
    let providers = vec![
        ("good_target", stone.clone()),
        (
            "both_weighted",
            both(
                "minecraft:weighted",
                json!({"entries": [{"data": stone, "weight": 1}]}),
            ),
        ),
        ("both_noise", both("minecraft:noise", noise_ok.clone())),
        (
            "both_dual_noise",
            both(
                "minecraft:dual_noise",
                json!({"variety": 2, "slow_noise": {"base_octave": -4}, "slow_scale": 1.0, "seed": 1, "noise": {"base_octave": -2}, "scale": 1.0, "states": [stone]}),
            ),
        ),
        (
            "both_noise_threshold",
            both(
                "minecraft:noise_threshold",
                json!({"seed": 1, "noise": {"base_octave": -2}, "scale": 1.0, "threshold": 0.5, "high_chance": 0.5, "default_state": stone, "low_states": [stone], "high_states": [stone]}),
            ),
        ),
        (
            "both_randomized_int",
            both(
                "minecraft:randomized_int",
                json!({"source": stone, "property": "age", "values": 1}),
            ),
        ),
        (
            "both_random_block_tag",
            both(
                "minecraft:random_block",
                json!({"blocks": "#minecraft:logs"}),
            ),
        ),
        (
            "both_rotated_reference",
            both("minecraft:rotated", json!({"state": "test:good_target"})),
        ),
        (
            "both_copy_properties_reference",
            both(
                "minecraft:copy_properties",
                json!({"source": "test:good_target"}),
            ),
        ),
        (
            "both_rule_based_fallback",
            both(
                "minecraft:rule_based",
                json!({"fallback": stone, "rules": []}),
            ),
        ),
        (
            "both_rule_based_fallback_reference",
            both(
                "minecraft:rule_based",
                json!({"fallback": "test:good_target", "rules": []}),
            ),
        ),
        (
            "both_rule_based_predicates",
            both(
                "minecraft:rule_based",
                json!({"rules": rules(one_of_each_predicate.clone(), json!("test:good_target"))}),
            ),
        ),
        (
            "both_rule_based_inline_then",
            both(
                "minecraft:rule_based",
                json!({"rules": rules(json!([{"type": "minecraft:solid"}]), json!({"type": "minecraft:rotated", "state": stone}))}),
            ),
        ),
        (
            "rotated_bad_inline_state",
            with_type("minecraft:rotated", json!({"state": bad_noise})),
        ),
        (
            "rotated_state_invalid_id",
            with_type("minecraft:rotated", json!({"state": "Not Valid"})),
        ),
        (
            "rotated_state_number",
            with_type("minecraft:rotated", json!({"state": 5})),
        ),
        (
            "rotated_state_missing_reference",
            with_type("minecraft:rotated", json!({"state": "test:missing"})),
        ),
        (
            "rule_based_bad_fallback",
            with_type(
                "minecraft:rule_based",
                json!({"fallback": bad_noise, "rules": []}),
            ),
        ),
        (
            "rule_based_rule_missing_fields",
            rule_based(json!([{}, {"then": stone}, {"if_true": {"type": "minecraft:solid"}}])),
        ),
        (
            "rule_based_bad_predicate_type",
            rule_based(rules(
                json!([{"type": "minecraft:nope"}, {"type": 5}, "x"]),
                stone.clone(),
            )),
        ),
        (
            "rule_based_bad_then",
            rule_based(rules(
                json!([{"type": "minecraft:solid"}]),
                json!({"type": "minecraft:nope"}),
            )),
        ),
        (
            "predicate_solid_offset_short",
            offset("solid", json!([1, 2])),
        ),
        (
            "predicate_solid_offset_long",
            offset("solid", json!([1, 2, 3, 4])),
        ),
        (
            "predicate_solid_offset_long_far",
            offset("solid", json!([100, 0, 0, 0])),
        ),
        (
            "predicate_solid_offset_not_ints",
            offset("solid", json!(["a", 1, 2])),
        ),
        ("predicate_solid_offset_number", offset("solid", json!(5))),
        (
            "predicate_solid_offset_far",
            offset("solid", json!([100, -3, 0])),
        ),
        (
            "predicate_solid_offset_fraction",
            offset("solid", json!([1.5, 2.5, 16.5])),
        ),
        (
            "predicate_unobstructed_offset_short",
            offset("unobstructed", json!([1, 2])),
        ),
        (
            "predicate_unobstructed_offset_far",
            offset("unobstructed", json!([100, 0, 0])),
        ),
        (
            "predicate_would_survive_offset_far",
            predicate(
                json!({"type": "minecraft:would_survive", "offset": [0, 17, 0], "state": "minecraft:nope"}),
            ),
        ),
        (
            "predicate_volume_match_bad",
            predicate(
                json!({"type": "minecraft:volume_match", "min": [1, 2], "max": [100, 0, 0], "match": {"type": "minecraft:nope"}}),
            ),
        ),
        (
            "predicate_volume_match_inverted",
            predicate(
                json!({"type": "minecraft:volume_match", "min": [2, 0, 0], "max": [0, 0, 0], "match": {"type": "minecraft:solid"}}),
            ),
        ),
        (
            "predicate_volume_match_inverted_bad_match",
            predicate(
                json!({"type": "minecraft:volume_match", "min": [0, 3, 0], "max": [0, 1, 0], "match": {"type": "minecraft:nope"}}),
            ),
        ),
        (
            "predicate_height_range_both",
            anchor(json!({"absolute": 5, "above_bottom": 3})),
        ),
        ("predicate_height_range_none", anchor(json!({}))),
        (
            "predicate_height_range_out_of_range",
            anchor(json!({"absolute": 99999})),
        ),
        ("predicate_height_range_number", anchor(json!(5))),
        (
            "predicate_height_range_three",
            anchor(json!({"absolute": 1, "above_bottom": 2, "below_top": 3})),
        ),
        (
            "predicate_height_range_all",
            anchor(
                json!({"absolute": 1, "above_bottom": 2, "below_top": 3, "relative_to_sea_level": 4}),
            ),
        ),
        (
            "predicate_height_range_last_two",
            anchor(json!({"below_top": 1, "relative_to_sea_level": 2})),
        ),
    ];
    for (name, provider) in &providers {
        write(
            root.join(format!("worldgen/block_state_provider/{name}.json")),
            &provider.to_string(),
        );
    }
    let transformer = |provider: serde_json::Value| json!([{"block_state_provider": provider}]);
    let transformers = [
        ("holder_bad_inline_provider", transformer(bad_noise.clone())),
        (
            "holder_good_and_bad_inline_provider",
            json!([{"block_state_provider": stone}, {"block_state_provider": bad_noise}]),
        ),
        (
            "holder_both_alternatives",
            transformer(both(
                "minecraft:random_block",
                json!({"blocks": "minecraft:dirt"}),
            )),
        ),
        ("holder_invalid_id", transformer(json!("Not Valid"))),
        ("holder_number", transformer(json!(5))),
        (
            "holder_state_without_id",
            transformer(json!({"properties": {}})),
        ),
        (
            "holder_nested_both_alternatives",
            transformer(with_type(
                "minecraft:rotated",
                json!({"state": both("minecraft:random_block", json!({"blocks": "minecraft:dirt"}))}),
            )),
        ),
    ];
    for (name, json) in &transformers {
        write(
            root.join(format!("block_transformer/{name}.json")),
            &json.to_string(),
        );
    }
    // a provider whose weights overflow throws instead of failing: the exception ends the entry, other errors vanish
    let overflow = json!({"type": "minecraft:weighted", "entries": [{"data": stone, "weight": 2147483647}, {"data": stone, "weight": 1}]});
    let enchantment = |providers: Vec<serde_json::Value>| {
        let mut enchantment = vanilla_json("enchantment/frost_walker.json");
        let effects = enchantment["effects"]["minecraft:location_changed"]
            .as_array_mut()
            .unwrap();
        for provider in providers {
            effects.push(json!({"effect": {"type": "minecraft:replace_block", "offset": [0, -1, 0], "block_state": provider}}));
        }
        enchantment
    };
    for (name, enchantment) in [
        ("overflow", enchantment(vec![overflow.clone()])),
        (
            "bad_provider",
            enchantment(vec![json!({"type": "minecraft:nope"})]),
        ),
        (
            "overflow_and_bad_provider",
            enchantment(vec![
                json!({"type": "minecraft:nope"}),
                overflow.clone(),
                bad_noise.clone(),
            ]),
        ),
        (
            "bad_provider_and_overflow",
            enchantment(vec![bad_noise, overflow]),
        ),
    ] {
        write(
            root.join(format!("enchantment/{name}.json")),
            &enchantment.to_string(),
        );
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

/// A world whose pack holds only `dimension` entries with broken inline noise settings: the worldgen registries load, so
/// the dimension stage reports each entry's error. Every entry is the synthetic settings with some edits (`(JSON pointer
/// into the settings, new value)`; no value removes the key).
pub fn write_broken_dimension_packs(world: &Path) {
    let dp = world.join("datapacks");
    write(
        dp.join("broken_dimensions/pack.mcmeta"),
        r##"{"pack": {"description": "broken dimensions", "min_format": 121, "max_format": 121}}"##,
    );
    for (name, edits) in broken_dimension_cases() {
        let mut value = stem("minecraft:overworld", synthetic_noise_settings());
        for (pointer, replacement) in edits {
            let (parent, key) = pointer.rsplit_once('/').unwrap_or(("", ""));
            let parent = value.pointer_mut(parent).expect("parent of the edit");
            match replacement {
                Some(json) => {
                    let json = serde_json::from_str(json).unwrap();
                    match parent {
                        serde_json::Value::Array(list) => {
                            list[key.parse::<usize>().unwrap()] = json
                        }
                        _ => {
                            parent[key] = json;
                        }
                    }
                }
                None => {
                    parent.as_object_mut().unwrap().remove(key);
                }
            }
        }
        write(
            dp.join(format!("broken_dimensions/data/test/dimension/{name}.json")),
            &value.to_string(),
        );
    }
}

type DimensionEdit = (String, Option<&'static str>);

/// `(entry name, edits)`; pointers of the settings are relative to `/generator/settings`.
fn broken_dimension_cases() -> Vec<(&'static str, Vec<DimensionEdit>)> {
    let at =
        |pointer: &str, json: &'static str| (format!("/generator/settings{pointer}"), Some(json));
    let without = |pointer: &str| (format!("/generator/settings{pointer}"), None);
    let one = |case: (&'static str, DimensionEdit)| (case.0, vec![case.1]);
    let density = |name: &'static str, json: &'static str| {
        one((name, at("/noise_router/final_density", json)))
    };
    let rule = |name: &'static str, json: &'static str| one((name, at("/material_rule", json)));
    let anchor = |name: &'static str, json: &'static str| {
        one((
            name,
            at("/material_rule/sequence/2/if_true/true_at_and_below", json),
        ))
    };
    let spawn = |name: &'static str, json: &'static str| one((name, at("/spawn_target", json)));
    let noise = |name: &'static str, json: &'static str| one((name, at("/noise", json)));
    vec![
        // density function holders
        density("density_unknown_type", r#"{"type": "minecraft:nope"}"#),
        density("density_unknown_type_unprefixed", r#"{"type": "nope"}"#),
        density("density_type_not_a_string", r#"{"type": 5}"#),
        density("density_no_type", "{}"),
        density("density_invalid_id", r#""Not Valid""#),
        density("density_missing_reference", r#""test:missing""#),
        density("density_boolean", "true"),
        density("density_array", "[1, 2]"),
        density("density_null", "null"),
        density(
            "density_constant_out_of_range",
            r#"{"type": "minecraft:constant", "value": 2000000}"#,
        ),
        density("density_number_out_of_range", "2000000.5"),
        density(
            "density_bad_left",
            r#"{"type": "minecraft:add", "left": {"type": "minecraft:nope"}, "right": 1}"#,
        ),
        density(
            "density_both_bad",
            r#"{"type": "minecraft:add", "left": "Not Valid", "right": {}}"#,
        ),
        density(
            "density_missing_right",
            r#"{"type": "minecraft:add", "left": 1}"#,
        ),
        density(
            "density_clamp_inverted",
            r#"{"type": "minecraft:clamp", "input": 0, "min": 5, "max": 2}"#,
        ),
        density(
            "density_gradient_equal",
            r#"{"type": "minecraft:gradient", "axis": "y", "from_coordinate": 3, "to_coordinate": 3, "from_value": 0, "to_value": 1}"#,
        ),
        density(
            "density_point_short",
            r#"{"type": "minecraft:distance_to_point", "point": [1, 2], "metric": "manhattan"}"#,
        ),
        density(
            "density_point_long",
            r#"{"type": "minecraft:distance_to_point", "point": [1, 2, 3, 4], "metric": "manhattan"}"#,
        ),
        density(
            "density_point_not_ints",
            r#"{"type": "minecraft:distance_to_point", "point": ["a", 1, 2], "metric": "manhattan"}"#,
        ),
        density(
            "density_point_number",
            r#"{"type": "minecraft:distance_to_point", "point": 5, "metric": "manhattan"}"#,
        ),
        density(
            "density_point_bad_metric",
            r#"{"type": "minecraft:distance_to_point", "point": [1, 2, 3], "metric": "x"}"#,
        ),
        density(
            "density_top_surface_range",
            r#"{"type": "minecraft:find_top_surface", "density": 0, "upper_bound": 0, "lower_bound": 99999, "cell_height": 0}"#,
        ),
        density(
            "density_blended_noise_range",
            r#"{"type": "minecraft:old_blended_noise", "xz_scale": 5000, "y_scale": 2.5, "xz_factor": 80, "y_factor": 160, "smear_scale_multiplier": 40}"#,
        ),
        // noise holders
        density(
            "noise_inline_octave",
            r#"{"type": "minecraft:noise", "noise": {"base_octave": 99}, "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_inline_modifiers",
            r#"{"type": "minecraft:noise", "noise": {"base_octave": -2, "octave_count": 2, "amplitude_modifiers": [1]}, "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_inline_empty",
            r#"{"type": "minecraft:noise", "noise": {}, "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_invalid_id",
            r#"{"type": "minecraft:noise", "noise": "Not Valid", "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_number",
            r#"{"type": "minecraft:noise", "noise": 5, "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_missing_reference",
            r#"{"type": "minecraft:noise", "noise": "minecraft:nope", "xz_scale": 1, "y_scale": 1}"#,
        ),
        density(
            "noise_shift_inline",
            r#"{"type": "minecraft:shift_a", "noise": {"base_octave": 99}}"#,
        ),
        density(
            "noise_bad_everything",
            r#"{"type": "minecraft:noise", "noise": {"base_octave": 99, "normalize": 5}, "xz_scale": "x", "shift_x": {"type": "nope"}}"#,
        ),
        // interval select, splines
        one((
            "select_few_thresholds",
            at("/noise_router/erosion/thresholds", "[0.5]"),
        )),
        one((
            "select_unordered_thresholds",
            at("/noise_router/erosion/thresholds", "[1, 0, 2]"),
        )),
        one((
            "select_one_function",
            at("/noise_router/erosion/functions", "[1]"),
        )),
        one((
            "select_bad_function",
            at(
                "/noise_router/erosion/functions",
                r#"[1, {"type": "minecraft:nope"}]"#,
            ),
        )),
        one((
            "select_bad_first_function",
            at("/noise_router/erosion/functions/0", r#"{"type": "nope"}"#),
        )),
        one((
            "select_inline_noise_bad_function",
            at(
                "/noise_router/erosion/functions/0",
                r#"{"type": "minecraft:noise", "noise": {"base_octave": 99}, "xz_scale": 1, "y_scale": 1}"#,
            ),
        )),
        one((
            "select_bad_threshold",
            at("/noise_router/erosion/thresholds/0", r#""x""#),
        )),
        one((
            "spline_no_points",
            at("/noise_router/depth/input/second/spline/points", "[]"),
        )),
        one((
            "spline_bad_point",
            at(
                "/noise_router/depth/input/second/spline/points",
                r#"[{"location": "x", "value": 1, "derivative": 0}]"#,
            ),
        )),
        one((
            "spline_bad_nested_value",
            at(
                "/noise_router/depth/input/second/spline/points",
                r#"[{"location": 0, "value": {"coordinate": "minecraft:y", "points": []}, "derivative": 0}]"#,
            ),
        )),
        one((
            "spline_bad_coordinate",
            at(
                "/noise_router/depth/input/second/spline/coordinate",
                r#"{"type": "nope"}"#,
            ),
        )),
        one((
            "spline_not_an_object",
            at("/noise_router/depth/input/second/spline", r#""x""#),
        )),
        // material rules and conditions
        rule("rule_unknown_type", r#"{"type": "minecraft:nope"}"#),
        rule("rule_invalid_id", r#""Not Valid""#),
        rule("rule_missing_reference", r#""test:missing""#),
        rule(
            "rule_sequence_bad_element",
            r#"{"type": "sequence", "sequence": [{"type": "nope"}, {"type": "bandlands"}]}"#,
        ),
        rule(
            "rule_sequence_two_bad",
            r#"{"type": "sequence", "sequence": [{"type": "nope"}, {"type": "x"}]}"#,
        ),
        rule(
            "rule_condition_no_then",
            r#"{"type": "condition", "if_true": {"type": "hole"}}"#,
        ),
        rule(
            "rule_condition_bad_condition",
            r#"{"type": "condition", "if_true": {"type": "nope"}, "then_run": {"type": "bandlands"}}"#,
        ),
        rule(
            "rule_block_unknown_state",
            r#"{"type": "block", "result_state": "minecraft:nope"}"#,
        ),
        one((
            "rule_ore_chance",
            at("/material_rule/sequence/14/raw_ore_chance", "2"),
        )),
        one((
            "condition_biome_missing",
            at(
                "/material_rule/sequence/8/if_true/biome_is",
                r#"["minecraft:nope"]"#,
            ),
        )),
        one((
            "condition_biome_tag_missing",
            at(
                "/material_rule/sequence/9/if_true/biome_is",
                r##""#minecraft:nope""##,
            ),
        )),
        one((
            "condition_multiplier_range",
            at(
                "/material_rule/sequence/3/if_true/surface_depth_multiplier",
                "99",
            ),
        )),
        one((
            "condition_noise_invalid_id",
            at("/material_rule/sequence/6/if_true/noise", r#""Not Valid""#),
        )),
        one((
            "condition_surface_type",
            at("/material_rule/sequence/5/if_true/surface_type", r#""x""#),
        )),
        // vertical anchors
        anchor("anchor_two", r#"{"absolute": 5, "above_bottom": 3}"#),
        anchor(
            "anchor_absolute_and_sea_level",
            r#"{"absolute": 5, "relative_to_sea_level": 1}"#,
        ),
        anchor(
            "anchor_last_three",
            r#"{"above_bottom": 1, "below_top": 2, "relative_to_sea_level": 3}"#,
        ),
        anchor(
            "anchor_all",
            r#"{"absolute": 1, "above_bottom": 2, "below_top": 3, "relative_to_sea_level": 4}"#,
        ),
        anchor(
            "anchor_last_two",
            r#"{"below_top": 1, "relative_to_sea_level": 2}"#,
        ),
        anchor("anchor_none", "{}"),
        anchor("anchor_out_of_range", r#"{"absolute": 99999}"#),
        anchor("anchor_number", "5"),
        anchor("anchor_bad_value", r#"{"above_bottom": "x"}"#),
        anchor(
            "anchor_two_bad_values",
            r#"{"absolute": "x", "below_top": true}"#,
        ),
        // settings
        noise("noise_min_y_multiple", r#"{"min_y": 5, "height": 384}"#),
        noise("noise_height_multiple", r#"{"min_y": -64, "height": 100}"#),
        noise("noise_too_high", r#"{"min_y": 2000, "height": 384}"#),
        noise("noise_empty", "{}"),
        noise("noise_not_an_object", "5"),
        one(("settings_sea_level_string", at("/sea_level", r#""x""#))),
        one(("settings_default_block_number", at("/default_block", "5"))),
        one((
            "settings_default_fluid_unknown",
            at("/default_fluid", r#"{"id": "minecraft:nope"}"#),
        )),
        one(("settings_no_sea_level", without("/sea_level"))),
        one((
            "settings_no_legacy_random_source",
            without("/legacy_random_source"),
        )),
        one((
            "settings_aquifers_bad",
            at("/aquifers", r#"{"barrier": {"type": "nope"}}"#),
        )),
        one((
            "settings_debug_functions_bad",
            at(
                "/debug_functions",
                r#"[{"label": 5, "function": {"type": "nope"}}]"#,
            ),
        )),
        one((
            "settings_router_missing_field",
            without("/noise_router/ridges"),
        )),
        one(("settings_router_empty", at("/noise_router", "{}"))),
        (
            "settings_several_errors",
            vec![
                without("/sea_level"),
                at("/noise", "{}"),
                at("/default_block", "5"),
                without("/noise_router/depth"),
            ],
        ),
        // spawn targets
        spawn(
            "spawn_interval_long",
            r#"[{"minecraft:y": [0.1, 0.2, 0.3]}]"#,
        ),
        spawn(
            "spawn_interval_inverted",
            r#"[{"minecraft:y": [0.5, 0.1]}]"#,
        ),
        spawn("spawn_interval_short", r#"[{"minecraft:y": [0.1]}]"#),
        spawn(
            "spawn_interval_object_missing",
            r#"[{"minecraft:y": {"min": 0.1}}]"#,
        ),
        spawn(
            "spawn_interval_object_inverted",
            r#"[{"minecraft:y": {"min": 0.5, "max": 0.1}}]"#,
        ),
        spawn("spawn_interval_string", r#"[{"minecraft:y": "x"}]"#),
        spawn("spawn_interval_out_of_range", r#"[{"minecraft:y": 3}]"#),
        spawn(
            "spawn_interval_bad_element",
            r#"[{"minecraft:y": [0.1, "x"]}]"#,
        ),
        spawn(
            "spawn_interval_bad_and_long",
            r#"[{"minecraft:y": [0.1, "x", 0.3]}]"#,
        ),
        spawn("spawn_missing_key", r#"[{"minecraft:nope": 0}]"#),
        spawn("spawn_invalid_key", r#"[{"Not Valid": 0}]"#),
        spawn("spawn_not_a_map", "[5]"),
        spawn("spawn_not_a_list", "{}"),
        // the settings of the generator
        one((
            "stem_settings_invalid_id",
            ("/generator/settings".to_owned(), Some(r#""Not Valid""#)),
        )),
        one((
            "stem_settings_missing_reference",
            ("/generator/settings".to_owned(), Some(r#""test:missing""#)),
        )),
        one((
            "stem_settings_number",
            ("/generator/settings".to_owned(), Some("5")),
        )),
        one((
            "stem_settings_missing",
            ("/generator/settings".to_owned(), None),
        )),
    ]
}

/// A noise level stem with inline noise `settings`.
fn stem(dimension_type: &str, settings: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type": dimension_type, "generator": {"type": "minecraft:noise", "settings": settings,
        "biome_source": {"type": "minecraft:fixed", "biome": "minecraft:plains"}}})
}

fn dimension_preset() -> serde_json::Value {
    use serde_json::json;
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
            "true_at_and_below": {"above_bottom": 0}, "false_at_and_above": {"absolute": 5, "above_bottom": 2, "below_top": 3}}, "then_run": stone},
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
