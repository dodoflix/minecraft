//! `net.minecraft.util.datafix.fixes.WorldGenSettingsDisallowOldCustomWorldsFix`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new WorldGenSettingsDisallowOldCustomWorldsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    let name = format!(
        "WorldGenSettingsDisallowOldCustomWorldsFix_{}",
        output_schema.version_key()
    );
    DataFix::new(
        "WorldGenSettingsDisallowOldCustomWorldsFix",
        output_schema,
        false,
        move |df| {
            let world_gen_settings_type =
                df.input_schema().get_type(&References::WORLD_GEN_SETTINGS);
            let dimensions_finder = world_gen_settings_type.find_field("dimensions");
            df.fix_type_everywhere_typed(&name, &world_gen_settings_type, move |input| {
                input.update_typed(&dimensions_finder, |dimensions| {
                    let map = dimensions
                        .write()
                        .result_or_partial()
                        .and_then(|tag| tag.get_map_values().result_or_partial());
                    for (_, value) in map.iter().flatten() {
                        if value.get("type").as_string().result().is_none() {
                            panic!(
                                "net.minecraft.nbt.NbtFormatException: Unable load old custom worlds."
                            );
                        }
                    }
                    dimensions
                })
            })
        },
    )
}
