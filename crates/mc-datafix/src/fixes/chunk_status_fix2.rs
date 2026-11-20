//! `ChunkStatusFix2`: renames the chunk statuses of the pre-1.14 pipeline and downgrades the unknown ones to `empty`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;

const RENAMES_AND_DOWNGRADES: [(&str, &str); 10] = [
    ("structure_references", "empty"),
    ("biomes", "empty"),
    ("base", "surface"),
    ("carved", "carvers"),
    ("liquid_carved", "liquid_carvers"),
    ("decorated", "features"),
    ("lighted", "light"),
    ("mobs_spawned", "spawn"),
    ("finalized", "heightmaps"),
    ("fullchunk", "full"),
];

/// `new ChunkStatusFix2(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ChunkStatusFix2", schema, changes_type, |df| {
        let chunk_type = df.input_schema().get_type(&References::CHUNK);
        let level_type = chunk_type.find_field_type("Level");
        let level_f = DSL::field_finder("Level", &level_type);
        df.fix_type_everywhere_typed_new(
            "ChunkStatusFix2",
            &chunk_type,
            &df.output_schema().get_type(&References::CHUNK),
            move |input| {
                input.update_typed(&level_f, |level| {
                    let tag = level.get_remainder();
                    let status = tag.get("Status").as_string_or("empty");
                    let new_status = RENAMES_AND_DOWNGRADES
                        .iter()
                        .find(|(old, _)| *old == status)
                        .map_or("empty", |(_, new)| *new);
                    if status == new_status {
                        level
                    } else {
                        level.set_remainder(tag.set("Status", tag.create_string(new_status)))
                    }
                })
            },
        )
    })
}
