//! `ChunkDeleteIgnoredLightDataFix`: drops the light data of the sections of chunks without `isLightOn`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ChunkDeleteIgnoredLightDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "ChunkDeleteIgnoredLightDataFix",
        output_schema,
        true,
        |df| {
            let chunk_type = df.input_schema().get_type(&References::CHUNK);
            let sections_finder = chunk_type.find_field("sections");
            df.fix_type_everywhere_typed(
                "ChunkDeleteIgnoredLightDataFix",
                &chunk_type,
                move |chunk| {
                    let is_light_on = chunk
                        .get(&DSL::remainder_finder())
                        .into_dynamic()
                        .get("isLightOn")
                        .as_boolean_or(false);
                    if is_light_on {
                        chunk
                    } else {
                        chunk.update_typed(&sections_finder, |section| {
                            section
                                .update_remainder(|tag| tag.remove("BlockLight").remove("SkyLight"))
                        })
                    }
                },
            )
        },
    )
}
