//! `net.minecraft.util.datafix.fixes.StructureSettingsFlattenFix`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// `new StructureSettingsFlattenFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("StructureSettingsFlattenFix", output_schema, false, |df| {
        let world_gen_settings_type = df.input_schema().get_type(&References::WORLD_GEN_SETTINGS);
        let dimensions = world_gen_settings_type.find_field("dimensions");
        df.fix_type_everywhere_typed(
            "StructureSettingsFlatten",
            &world_gen_settings_type,
            move |input| {
                input.update_typed(&dimensions, |typed| {
                    write_and_read_typed_or_throw(&typed, dimensions.ty(), |serialized| {
                        serialized.update_map_values(fix_dimension)
                    })
                })
            },
        )
    })
}

fn fix_dimension((key, dimension): (Dynamic, Dynamic)) -> (Dynamic, Dynamic) {
    let fixed = dimension.update("generator", |g| {
        g.update("settings", |s| s.update("structures", fix_structures))
    });
    (key, fixed)
}

fn fix_structures(input: Dynamic) -> Dynamic {
    let structures = input
        .get("structures")
        .or_else_empty_map()
        .update_map_values(|(key, s)| {
            (
                key,
                s.set("type", input.create_string("minecraft:random_spread")),
            )
        });
    match input.get("stronghold").result() {
        Some(stronghold) => structures.set(
            "minecraft:stronghold",
            stronghold.set("type", input.create_string("minecraft:concentric_rings")),
        ),
        None => structures,
    }
}
