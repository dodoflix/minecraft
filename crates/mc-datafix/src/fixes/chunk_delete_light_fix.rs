//! `ChunkDeleteLightFix`: drops the light data of a chunk and its sections.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ChunkDeleteLightFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ChunkDeleteLightFix", output_schema, false, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let sections_finder = chunk_type.find_field("sections");
        df.fix_type_everywhere_typed(
            &format!(
                "ChunkDeleteLightFix for {}",
                df.output_schema().version_key()
            ),
            &chunk_type,
            move |chunk| {
                let chunk = chunk.update_remainder(|tag| tag.remove("isLightOn"));
                chunk.update_typed(&sections_finder, |section| {
                    section.update_remainder(|tag| tag.remove("BlockLight").remove("SkyLight"))
                })
            },
        )
    })
}
