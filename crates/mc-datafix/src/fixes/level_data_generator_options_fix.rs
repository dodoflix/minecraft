//! `LevelDataGeneratorOptionsFix`: turns the legacy `generatorOptions` string of `flat` / `buffet` worlds into the
//! structured form (flat: `layers`, `biome`, `structures`; buffet: the parsed JSON).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::fixes::tables::{OrderedMap, table};
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_core::gson::{self, JsonElement};
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_core::math::java::parse_int;
use mc_dfu::extra::java_hash_map_order;
use mc_dfu::{Dyn, Dynamic, JSON};
use std::sync::LazyLock;

static MAP: LazyLock<OrderedMap<String, String>> = LazyLock::new(|| {
    table("level_data_generator_options_fix")
        .field("MAP")
        .string_map()
});

/// `GENERATOR_OPTIONS`.
pub const GENERATOR_OPTIONS: &str = "generatorOptions";

/// `new LevelDataGeneratorOptionsFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "LevelDataGeneratorOptionsFix",
        output_schema,
        changes_type,
        |df| {
            let result_type = df.output_schema().get_type(&References::LEVEL);
            df.fix_type_everywhere_typed_new(
                "LevelDataGeneratorOptionsFix",
                &df.input_schema().get_type(&References::LEVEL),
                &result_type.clone(),
                move |input| write_and_read_typed_or_throw(&input, &result_type, fix_tag),
            )
        },
    )
}

/// The `writeAndReadTypedOrThrow` lambda of `makeRule`.
fn fix_tag(tag: Dynamic) -> Dynamic {
    let generator_options = tag.get("generatorOptions").as_string().result();
    if tag
        .get("generatorName")
        .as_string_or("")
        .eq_ignore_ascii_case("flat")
    {
        let flat_option_string = generator_options.unwrap_or_default();
        tag.set("generatorOptions", convert(&flat_option_string, &tag))
    } else if tag
        .get("generatorName")
        .as_string_or("")
        .eq_ignore_ascii_case("buffet")
        && let Some(generator_options) = generator_options
    {
        let legacy_options = gson::parse_lenient_json_parser(&generator_options)
            .unwrap_or_else(|e| panic!("com.google.gson.JsonSyntaxException: {e}"));
        let json = Dynamic::new(JSON, json_to_dyn(&legacy_options));
        tag.set("generatorOptions", json.convert(&tag.ops()))
    } else {
        tag
    }
}

/// The `JsonElement` as a value of the JSON ops.
fn json_to_dyn(json: &JsonElement) -> Dyn {
    match json {
        JsonElement::Null => Dyn::Empty,
        JsonElement::Bool(b) => Dyn::Bool(*b),
        JsonElement::Number(n) => Dyn::JsonNum(n.clone()),
        JsonElement::String(s) => Dyn::Str(s.clone()),
        JsonElement::Array(a) => Dyn::List(a.iter().map(json_to_dyn).collect()),
        JsonElement::Object(o) => Dyn::Map(
            o.iter()
                .map(|(k, v)| (Dyn::Str(k.to_string()), json_to_dyn(v)))
                .collect(),
        ),
    }
}

/// `String.split(sep)` for a single literal character: trailing empty strings are dropped unless nothing matched.
fn split(s: &str, sep: char) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(sep).collect();
    if parts.len() > 1 {
        while parts.last() == Some(&"") {
            parts.pop();
        }
    }
    parts
}

/// `String.split(sep, 2)` for a single literal character.
fn split2(s: &str, sep: char) -> Vec<&str> {
    s.splitn(2, sep).collect()
}

/// `HashMap.put` on an association list: replaces the value of an existing key in place.
fn put(map: &mut Vec<(String, Vec<(String, String)>)>, key: &str, value: Vec<(String, String)>) {
    match map.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => map.push((key.to_string(), value)),
    }
}

/// `convert(flatOptionString, ops)`, with the ops of `tag`.
fn convert(flat_option_string: &str, tag: &Dynamic) -> Dynamic {
    let mut parts = flat_option_string.split(';');
    let mut biome = "minecraft:plains".to_string();
    let mut structures_options: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let first = if flat_option_string.is_empty() {
        None
    } else {
        parts.next()
    };
    let layer_list = if let Some(first) = first {
        let layer_list = get_layers_info_from_string(first);
        if !layer_list.is_empty() {
            if let Some(biome_id) = parts.next() {
                biome = MAP
                    .get(biome_id)
                    .map_or("minecraft:plains", String::as_str)
                    .to_string();
            }
            if let Some(structures) = parts.next() {
                for structure in split(&structures.to_lowercase(), ',') {
                    let separated = split2(structure, '(');
                    if !separated[0].is_empty() {
                        put(&mut structures_options, separated[0], Vec::new());
                        if separated.len() > 1
                            && separated[1].ends_with(')')
                            && separated[1].len() > 1
                        {
                            let options = &separated[1][..separated[1].len() - 1];
                            for part in split(options, ' ') {
                                let key_value = split2(part, '=');
                                if key_value.len() == 2 {
                                    let (_, structure_options) = structures_options
                                        .iter_mut()
                                        .find(|(k, _)| k == separated[0])
                                        .expect("the structure was just put");
                                    match structure_options
                                        .iter_mut()
                                        .find(|(k, _)| k == key_value[0])
                                    {
                                        Some(option) => option.1 = key_value[1].to_string(),
                                        None => structure_options.push((
                                            key_value[0].to_string(),
                                            key_value[1].to_string(),
                                        )),
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                put(&mut structures_options, "village", Vec::new());
            }
        }
        layer_list
    } else {
        put(&mut structures_options, "village", Vec::new());
        vec![
            (1, "minecraft:bedrock".to_string()),
            (2, "minecraft:dirt".to_string()),
            (1, "minecraft:grass_block".to_string()),
        ]
    };

    let json = tag.ops().is_json();
    let layers = tag.create_list(layer_list.iter().map(|(height, block)| {
        tag.create_map([
            (tag.create_string("height"), tag.create_int(*height)),
            (
                tag.create_string("block"),
                tag.create_string(block.as_str()),
            ),
        ])
    }));
    // `Collectors.toMap` into a `HashMap` keyed by the created string values
    let to_map = |entries: Vec<(Dynamic, Dynamic)>| {
        hash_map_ordered(
            entries,
            |(k, _)| k.value().java_hash(json),
            HASH_MAP_DEFAULT_CAPACITY,
        )
    };
    let structures = tag.create_map(to_map(
        java_hash_map_order(structures_options)
            .into_iter()
            .map(|(name, options)| {
                let options = tag.create_map(to_map(
                    java_hash_map_order(options)
                        .into_iter()
                        .map(|(k, v)| (tag.create_string(k), tag.create_string(v)))
                        .collect(),
                ));
                (tag.create_string(name.to_lowercase()), options)
            })
            .collect(),
    ));
    tag.create_map([
        (tag.create_string("layers"), layers),
        (tag.create_string("biome"), tag.create_string(biome)),
        (tag.create_string("structures"), structures),
    ])
}

/// `getLayerInfoFromString(input)`: `None` where Java returns null (an unparsable height).
fn get_layer_info_from_string(input: &str) -> Option<(i32, String)> {
    let parts = split2(input, '*');
    let height = if parts.len() == 2 {
        parse_int(parts[0])?
    } else {
        1
    };
    Some((height, parts[parts.len() - 1].to_string()))
}

/// `getLayersInfoFromString(input)`: empty as soon as one layer does not parse.
fn get_layers_info_from_string(input: &str) -> Vec<(i32, String)> {
    let mut result = Vec::new();
    for depth in split(input, ',') {
        match get_layer_info_from_string(depth) {
            Some(layer) => result.push(layer),
            None => return Vec::new(),
        }
    }
    result
}
