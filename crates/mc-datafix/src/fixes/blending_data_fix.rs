//! `BlendingDataFix`: chunks that predate blending get the `blending_data` section range of the overworld.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;

const STATUSES_TO_SKIP_BLENDING: [&str; 4] = [
    "minecraft:empty",
    "minecraft:structure_starts",
    "minecraft:structure_references",
    "minecraft:biomes",
];

/// `new BlendingDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    let name = format!("Blending Data Fix v{}", output_schema.version_key());
    DataFix::new("BlendingDataFix", output_schema, false, move |df| {
        let chunk_type = df.output_schema().get_type(&References::CHUNK);
        df.fix_type_everywhere_typed(&name, &chunk_type, |chunk| {
            chunk.update_remainder(update_chunk_tag)
        })
    })
}

fn update_chunk_tag(chunk_tag: Dynamic) -> Dynamic {
    let context = chunk_tag.get("__context");
    let mut chunk_tag = chunk_tag.remove("blending_data");
    let is_overworld =
        context.get("dimension").as_string().result().as_deref() == Some("minecraft:overworld");
    if let (true, Some(status)) = (is_overworld, chunk_tag.get("Status").result()) {
        let status = ensure_namespaced(&status.as_string_or("empty"));
        if !STATUSES_TO_SKIP_BLENDING.contains(&status.as_str()) {
            chunk_tag = update_blending_data(&chunk_tag, 384, -64);
        } else if let Some(below_zero_retrogen) = chunk_tag.get("below_zero_retrogen").result() {
            let target_status = ensure_namespaced(
                &below_zero_retrogen
                    .get("target_status")
                    .as_string_or("empty"),
            );
            if !STATUSES_TO_SKIP_BLENDING.contains(&target_status.as_str()) {
                chunk_tag = update_blending_data(&chunk_tag, 256, 0);
            }
        }
    }
    chunk_tag
}

fn update_blending_data(chunk_tag: &Dynamic, height: i32, min_y: i32) -> Dynamic {
    chunk_tag.set(
        "blending_data",
        chunk_tag.create_map([
            (
                chunk_tag.create_string("min_section"),
                chunk_tag.create_int(min_y >> 4),
            ),
            (
                chunk_tag.create_string("max_section"),
                chunk_tag.create_int((min_y + height) >> 4),
            ),
        ]),
    )
}
