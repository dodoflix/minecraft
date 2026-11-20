//! `OminousBannerBlockEntityRenameFix`: renames the illager banner translation key in the `CustomName` of banners.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;

/// `new OminousBannerBlockEntityRenameFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    let input_schema = if changes_type {
        output_schema
            .parent()
            .expect("a type-changing fix needs a parent schema")
            .clone()
    } else {
        output_schema.clone()
    };
    named_entity_fix::new(
        "OminousBannerBlockEntityRenameFix",
        output_schema,
        changes_type,
        "OminousBannerBlockEntityRenameFix",
        &References::BLOCK_ENTITY,
        "minecraft:banner",
        move |entity| {
            let custom_name_f = entity.get_type().find_field("CustomName");
            let text_component_f =
                DSL::type_finder(&input_schema.get_type(&References::TEXT_COMPONENT));
            entity.update_typed(&custom_name_f, |custom_name| {
                custom_name.update(&text_component_f, |pair| {
                    pair.map_second(|name| {
                        Value::Str(name.as_java_string().map_text(|t| {
                            t.replace(
                                "\"translate\":\"block.minecraft.illager_banner\"",
                                "\"translate\":\"block.minecraft.ominous_banner\"",
                            )
                        }))
                    })
                })
            })
        },
    )
}
