//! `net.minecraft.util.datafix.fixes.WorldGenSettingsHeightAndBiomeFix`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// `WorldGenSettingsHeightAndBiomeFix.WAS_PREVIOUSLY_INCREASED_KEY`.
pub const WAS_PREVIOUSLY_INCREASED_KEY: &str = "has_increased_height_already";

/// `new WorldGenSettingsHeightAndBiomeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "WorldGenSettingsHeightAndBiomeFix",
        output_schema,
        true,
        |df| {
            let world_gen_settings_type =
                df.input_schema().get_type(&References::WORLD_GEN_SETTINGS);
            let dimensions_finder = world_gen_settings_type.find_field("dimensions");
            let world_gen_settings_type_new =
                df.output_schema().get_type(&References::WORLD_GEN_SETTINGS);
            let dimensions_type = world_gen_settings_type_new.find_field_type("dimensions");
            df.fix_type_everywhere_typed_new(
                "WorldGenSettingsHeightAndBiomeFix",
                &world_gen_settings_type,
                &world_gen_settings_type_new,
                move |input| {
                    let was_increased_opt = input
                        .get(&DSL::remainder_finder())
                        .into_dynamic()
                        .get(WAS_PREVIOUSLY_INCREASED_KEY);
                    let was_exp_snap = !was_increased_opt.is_present();
                    let was_previously_increased = was_increased_opt.as_boolean_or(true);
                    input
                        .update_remainder(|tag| tag.remove(WAS_PREVIOUSLY_INCREASED_KEY))
                        .update_typed_as(&dimensions_finder, &dimensions_type, |dimensions| {
                            write_and_read_typed_or_throw(&dimensions, &dimensions_type, |tag| {
                                tag.update("minecraft:overworld", |overworld_tag| {
                                    overworld_tag.update("generator", |generator| {
                                        fix_generator(
                                            generator,
                                            was_exp_snap,
                                            was_previously_increased,
                                        )
                                    })
                                })
                            })
                        })
                },
            )
        },
    )
}

fn fix_generator(
    generator: Dynamic,
    was_exp_snap: bool,
    was_previously_increased: bool,
) -> Dynamic {
    let generator_type = generator.get("type").as_string_or("");
    if generator_type == "minecraft:noise" {
        let mut is_large_biomes = false;
        let generator = generator.update("biome_source", |biome_source| {
            let type_ = biome_source.get("type").as_string_or("");
            if type_ == "minecraft:vanilla_layered"
                || (was_exp_snap && type_ == "minecraft:multi_noise")
            {
                if biome_source.get("large_biomes").as_boolean_or(false) {
                    is_large_biomes = true;
                }
                biome_source.create_map([
                    (
                        biome_source.create_string("preset"),
                        biome_source.create_string("minecraft:overworld"),
                    ),
                    (
                        biome_source.create_string("type"),
                        biome_source.create_string("minecraft:multi_noise"),
                    ),
                ])
            } else {
                biome_source
            }
        });
        if is_large_biomes {
            generator.update("settings", |settings| {
                if settings.as_string_or("") == "minecraft:overworld" {
                    settings.create_string("minecraft:large_biomes")
                } else {
                    settings
                }
            })
        } else {
            generator
        }
    } else if generator_type == "minecraft:flat" {
        if was_previously_increased {
            generator
        } else {
            generator.update("settings", |settings| {
                settings.update("layers", update_layers)
            })
        }
    } else {
        generator
    }
}

fn update_layers(layers: Dynamic) -> Dynamic {
    let air_layer = layers.create_map([
        (layers.create_string("height"), layers.create_int(64)),
        (
            layers.create_string("block"),
            layers.create_string("minecraft:air"),
        ),
    ]);
    layers.create_list(std::iter::once(air_layer).chain(layers.as_stream()))
}
