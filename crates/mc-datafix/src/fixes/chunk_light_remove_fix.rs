//! `ChunkLightRemoveFix`: drops the `isLightOn` flag of a chunk.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ChunkLightRemoveFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ChunkLightRemoveFix", schema, changes_type, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_type = chunk_type.find_field_type("Level");
        let level_f = DSL::field_finder("Level", &level_type);
        df.fix_type_everywhere_typed_new(
            "ChunkLightRemoveFix",
            &chunk_type,
            &df.output_schema().get_type(&References::CHUNK),
            move |input| {
                input.update_typed(&level_f, |level| {
                    level.update_remainder(|tag| tag.remove("isLightOn"))
                })
            },
        )
    })
}
