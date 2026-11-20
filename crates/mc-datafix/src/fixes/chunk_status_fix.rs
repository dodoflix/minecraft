//! `ChunkStatusFix`: renames the legacy chunk status `postprocessed` to `fullchunk`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ChunkStatusFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ChunkStatusFix", schema, changes_type, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_type = chunk_type.find_field_type("Level");
        let level_f = DSL::field_finder("Level", &level_type);
        df.fix_type_everywhere_typed_new(
            "ChunkStatusFix",
            &chunk_type,
            &df.output_schema().get_type(&References::CHUNK),
            move |input| {
                input.update_typed(&level_f, |level| {
                    let mut tag = level.get_remainder();
                    let status = tag.get("Status").as_string_or("empty");
                    if status == "postprocessed" {
                        tag = tag.set("Status", tag.create_string("fullchunk"));
                    }
                    level.set_remainder(tag)
                })
            },
        )
    })
}
