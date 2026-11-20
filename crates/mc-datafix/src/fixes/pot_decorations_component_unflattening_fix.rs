//! `PotDecorationsComponentUnflatteningFix`: the `minecraft:pot_decorations` component changes from a list of four
//! item ids to a map keyed by side.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::java_collections::hash_map_ordered;
use mc_dfu::Dynamic;

/// `new PotDecorationsComponentUnflatteningFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "PotDecorationsComponentUnflatteningFix",
        output_schema,
        true,
        |df| {
            df.write_fix_and_read(
                "Pot decoration structure fix",
                &df.input_schema().get_type(&References::DATA_COMPONENTS),
                &df.output_schema().get_type(&References::DATA_COMPONENTS),
                |components| {
                    components.update("minecraft:pot_decorations", |list| unpack_list(&list))
                },
            )
        },
    )
}

/// `PotDecorationsComponentUnflatteningFix.unpackList(original)`.
pub fn unpack_list(original: &Dynamic) -> Dynamic {
    let Some(decoration_ids) = original.as_stream_opt().result() else {
        return original.clone();
    };
    let ids: Vec<Option<String>> = decoration_ids
        .iter()
        .map(|s| s.as_string().result())
        .collect();
    let mut result = Vec::with_capacity(4);
    for (i, side) in ["back", "left", "right", "front"].into_iter().enumerate() {
        let id = ids
            .get(i)
            .cloned()
            .flatten()
            .unwrap_or_else(|| "minecraft:brick".to_string());
        if id.is_empty() {
            return original.clone();
        }
        let new_stack =
            original.create_map([(original.create_string("id"), original.create_string(id))]);
        result.push((original.create_string(side), new_stack));
    }
    // a `HashMap<Dynamic, Dynamic>(4)` hands its iteration order to `createMap`
    original.create_map(hash_map_ordered(result, |(k, _)| k.hash_code(), 4))
}
