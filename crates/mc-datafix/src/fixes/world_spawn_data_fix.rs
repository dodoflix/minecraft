//! `WorldSpawnDataFix`: the `SpawnX`/`SpawnY`/`SpawnZ`/`SpawnAngle` fields of the level data become a `spawn` compound.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new WorldSpawnDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("WorldSpawnDataFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "WorldSpawnDataFix",
            &df.input_schema().get_type(&References::LEVEL),
            |input| {
                input.update_remainder(|tag| {
                    let spawn_x = tag.get("SpawnX").as_int_or(0);
                    let spawn_y = tag.get("SpawnY").as_int_or(0);
                    let spawn_z = tag.get("SpawnZ").as_int_or(0);
                    let angle = tag.get("SpawnAngle").as_float_or(0.0);
                    let spawn_data = tag
                        .empty_map()
                        .set("dimension", tag.create_string("minecraft:overworld"))
                        .set("pos", tag.create_int_list([spawn_x, spawn_y, spawn_z]))
                        .set("yaw", tag.create_float(angle))
                        .set("pitch", tag.create_float(0.0));
                    tag.remove("SpawnX")
                        .remove("SpawnY")
                        .remove("SpawnZ")
                        .remove("SpawnAngle")
                        .set("spawn", spawn_data)
                })
            },
        )
    })
}
