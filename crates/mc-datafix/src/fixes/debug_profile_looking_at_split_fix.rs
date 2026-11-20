//! `DebugProfileLookingAtSplitFix`: renames the `looking_at_block` / `looking_at_fluid` debug entries to their
//! `_state` variants; an existing entry of the new name wins.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_dfu::Dynamic;

/// `new DebugProfileLookingAtSplitFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "DebugProfileLookingAtSplitFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "DebugProfileLookingAtSplitFix",
                &df.input_schema().get_type(&References::DEBUG_PROFILE),
                |input| input.update_remainder(|tag| tag.update("custom", update_options)),
            )
        },
    )
}

fn update_options(custom: Dynamic) -> Dynamic {
    let Some(map) = custom.get_map_values().result() else {
        return custom;
    };
    // the entries of a `HashMap<Dynamic, Dynamic>`: `put` replaces the value of an equal key in place
    let mut new_options: Vec<(Dynamic, Dynamic)> = Vec::new();
    for (key, value) in map {
        match renamed_key(&key) {
            Some(new_key) => {
                if !new_options.iter().any(|(k, _)| *k == new_key) {
                    new_options.push((new_key, value));
                }
            }
            None => match new_options.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => new_options.push((key, value)),
            },
        }
    }
    let new_options = hash_map_ordered(
        new_options,
        |(k, _)| k.hash_code(),
        HASH_MAP_DEFAULT_CAPACITY,
    );
    custom.create_map(new_options)
}

fn renamed_key(key_dynamic: &Dynamic) -> Option<Dynamic> {
    let key = key_dynamic.as_string().result()?;
    let new_key = match key.as_lossy_str() {
        "minecraft:looking_at_block" => "minecraft:looking_at_block_state",
        "minecraft:looking_at_fluid" => "minecraft:looking_at_fluid_state",
        _ => return None,
    };
    Some(key_dynamic.create_string(new_key))
}
