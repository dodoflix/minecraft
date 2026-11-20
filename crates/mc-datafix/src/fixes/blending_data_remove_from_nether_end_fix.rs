//! `BlendingDataRemoveFromNetherEndFix`: removes `blending_data` from chunks outside the overworld.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new BlendingDataRemoveFromNetherEndFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "BlendingDataRemoveFromNetherEndFix",
        output_schema,
        false,
        |df| {
            let chunk_type = df.output_schema().get_type(&References::CHUNK);
            df.fix_type_everywhere_typed(
                "BlendingDataRemoveFromNetherEndFix",
                &chunk_type,
                |chunk| {
                    chunk.update_remainder(|chunk_tag| {
                        let dimension = chunk_tag
                            .get("__context")
                            .get("dimension")
                            .as_string()
                            .result()
                            .unwrap_or_default();
                        if dimension == "minecraft:overworld" {
                            chunk_tag
                        } else {
                            chunk_tag.remove("blending_data")
                        }
                    })
                },
            )
        },
    )
}
