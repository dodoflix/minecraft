//! `LevelLegacyWorldGenSettingsFix`: moves the legacy generator fields of the level data into its `WorldGenSettings`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

const OLD_SETTINGS_KEYS: [&str; 7] = [
    "RandomSeed",
    "generatorName",
    "generatorOptions",
    "generatorVersion",
    "legacy_custom_options",
    "MapFeatures",
    "BonusChest",
];

/// `new LevelLegacyWorldGenSettingsFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "LevelLegacyWorldGenSettingsFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "LevelLegacyWorldGenSettingsFix",
                &df.input_schema().get_type(&References::LEVEL),
                |input| {
                    input.update_remainder(|mut data_tag| {
                        let mut world_gen_settings =
                            data_tag.get("WorldGenSettings").or_else_empty_map();
                        for key in OLD_SETTINGS_KEYS {
                            if let Some(old_setting) = data_tag.get(key).result() {
                                data_tag = data_tag.remove(key);
                                world_gen_settings = world_gen_settings.set(key, old_setting);
                            }
                        }
                        data_tag.set("WorldGenSettings", world_gen_settings)
                    })
                },
            )
        },
    )
}
