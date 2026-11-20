//! `LevelFlatGeneratorInfoFix`: rewrites the legacy `generatorOptions` string of flat worlds (numeric block ids, `x`
//! repeat counts) to the namespaced block syntax.

use crate::datafix::DataFix;
use crate::fixes::block_state_data;
use crate::fixes::references::References;
use crate::fixes::tables::entity_block_state_fix;
use crate::schema::Schema;
use mc_core::math::mth::get_int;
use mc_dfu::Dynamic;

const DEFAULT: &str = "minecraft:bedrock,2*minecraft:dirt,minecraft:grass_block;1;village";

/// `new LevelFlatGeneratorInfoFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "LevelFlatGeneratorInfoFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "LevelFlatGeneratorInfoFix",
                &df.input_schema().get_type(&References::LEVEL),
                |input| input.update_remainder(fix),
            )
        },
    )
}

fn fix(input: Dynamic) -> Dynamic {
    if input
        .get("generatorName")
        .as_string_or("")
        .as_lossy_str()
        .eq_ignore_ascii_case("flat")
    {
        input.update("generatorOptions", |options| {
            match options.as_string().result() {
                Some(generator_options) => {
                    options.create_string(fix_string(generator_options.as_lossy_str()))
                }
                None => options,
            }
        })
    } else {
        input
    }
}

/// `fixString(generatorOptions)`.
pub fn fix_string(generator_options: &str) -> String {
    if generator_options.is_empty() {
        return DEFAULT.to_string();
    }
    let mut parts = generator_options.splitn(5, ';');
    let first_part = parts.next().expect("a split has a first part");
    let version;
    let layer_info;
    match parts.next() {
        Some(second_part) => {
            version = get_int(first_part, 0);
            layer_info = second_part;
        }
        None => {
            version = 0;
            layer_info = first_part;
        }
    }
    if !(0..=3).contains(&version) {
        return DEFAULT.to_string();
    }
    let height_separator = if version < 3 { 'x' } else { '*' };
    let layers: Vec<String> = layer_info
        .split(',')
        .map(|layer_string| {
            let list: Vec<&str> = layer_string.splitn(2, height_separator).collect();
            let (height, layer_type) = if list.len() == 2 {
                (get_int(list[0], 0), list[1])
            } else {
                (1, list[0])
            };
            let layer_parts: Vec<&str> = layer_type.splitn(3, ':').collect();
            let name_index = usize::from(layer_parts[0] == "minecraft");
            let Some(block_string) = layer_parts.get(name_index) else {
                panic!(
                    "java.lang.IndexOutOfBoundsException: Index {name_index} out of bounds for length {}",
                    layer_parts.len()
                )
            };
            let block_id = if version == 3 {
                entity_block_state_fix::get_block_id(&format!("minecraft:{block_string}"))
            } else {
                get_int(block_string, 0)
            };
            let data_index = name_index + 1;
            let data = if layer_parts.len() > data_index {
                get_int(layer_parts[data_index], 0)
            } else {
                0
            };
            let name = block_state_data::get_tag((block_id << 4) | data)
                .as_compound()
                .and_then(|c| c.get_string("Name"))
                .unwrap_or("");
            let prefix = if height == 1 {
                String::new()
            } else {
                format!("{height}*")
            };
            format!("{prefix}{name}")
        })
        .collect();
    let mut result = layers.join(",");
    for part in parts {
        result.push(';');
        result.push_str(part);
    }
    result
}
