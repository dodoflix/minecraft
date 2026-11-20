//! `MissingDimensionFix`: rebuilds the default dimensions of a world generation setting whose `dimensions` map is empty.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::fixes::world_gen_settings_fix::{default_overworld, vanilla_levels};
use crate::schema::Schema;
use crate::ty::Type;
use crate::value::Value;
use mc_dfu::Dynamic;

fn fields(name: &str, ty: &Type) -> Type {
    DSL::and_type(&DSL::field_type(name, ty), &DSL::remainder_type())
}

fn optional_fields(name: &str, ty: &Type) -> Type {
    DSL::and_type(
        &DSL::optional_type(&DSL::field_type(name, ty)),
        &DSL::remainder_type(),
    )
}

fn optional_fields2(name1: &str, type1: &Type, name2: &str, type2: &Type) -> Type {
    DSL::and_type3(
        &DSL::optional_type(&DSL::field_type(name1, type1)),
        &DSL::optional_type(&DSL::field_type(name2, type2)),
        &DSL::remainder_type(),
    )
}

/// `new MissingDimensionFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("MissingDimensionFix", schema, changes_type, |df| {
        let schema = df.input_schema();
        let biome = schema.get_type(&References::BIOME);
        let block_name = schema.get_type(&References::BLOCK_NAME);
        let generator_type = DSL::tagged_choice_type(
            "type",
            &DSL::string(),
            vec![
                ("minecraft:debug".to_string(), DSL::remainder_type()),
                ("minecraft:flat".to_string(), flat_type(&schema)),
                (
                    "minecraft:noise".to_string(),
                    optional_fields2(
                        "biome_source",
                        &DSL::tagged_choice_type(
                            "type",
                            &DSL::string(),
                            vec![
                                ("minecraft:fixed".to_string(), fields("biome", &biome)),
                                (
                                    "minecraft:multi_noise".to_string(),
                                    DSL::list_type(&fields("biome", &biome)),
                                ),
                                (
                                    "minecraft:checkerboard".to_string(),
                                    fields("biomes", &DSL::list_type(&biome)),
                                ),
                                (
                                    "minecraft:vanilla_layered".to_string(),
                                    DSL::remainder_type(),
                                ),
                                ("minecraft:the_end".to_string(), DSL::remainder_type()),
                            ],
                        ),
                        "settings",
                        &DSL::or_type(
                            &DSL::string(),
                            &optional_fields2(
                                "default_block",
                                &block_name,
                                "default_fluid",
                                &block_name,
                            ),
                        ),
                    ),
                ),
            ],
        );
        let dimensions_type = DSL::compound_list_type_kv(
            &DSL::namespaced_string(),
            &fields("generator", &generator_type),
        );
        let expected_dimensions_type = DSL::and_type(&dimensions_type, &DSL::remainder_type());
        let settings = schema.get_type(&References::WORLD_GEN_SETTINGS);
        let dimensions_finder = DSL::field_finder("dimensions", &expected_dimensions_type);
        if !settings.find_field_type("dimensions").equals_with(
            &expected_dimensions_type,
            false,
            true,
        ) {
            panic!("java.lang.IllegalStateException");
        }
        let dimension_list_finder = dimensions_type.finder();
        df.fix_type_everywhere_typed("MissingDimensionFix", &settings, move |input| {
            let tag = input.get(&DSL::remainder_finder()).into_dynamic();
            input.update_typed(&dimensions_finder, |dimensions| {
                dimensions.update_typed(&dimension_list_finder, |generators| {
                    let Value::List(list) = generators.get_value() else {
                        panic!("java.lang.IllegalStateException: List exptected");
                    };
                    if list.is_empty() {
                        let new_dimensions = recreate_settings(&tag);
                        match dimensions_type.read_typed(&new_dimensions).result() {
                            Some((typed, _)) => typed,
                            None => generators,
                        }
                    } else {
                        generators
                    }
                })
            })
        })
    })
}

fn flat_type(schema: &Schema) -> Type {
    optional_fields(
        "settings",
        &optional_fields2(
            "biome",
            &schema.get_type(&References::BIOME),
            "layers",
            &DSL::list_type(&optional_fields(
                "block",
                &schema.get_type(&References::BLOCK_NAME),
            )),
        ),
    )
}

fn recreate_settings(tag: &Dynamic) -> Dynamic {
    let seed = tag.get("seed").as_long_or(0);
    Dynamic::new(
        tag.ops(),
        vanilla_levels(tag, seed, &default_overworld(tag, seed), false),
    )
}
