//! `OminousBannerRarityFix`: ominous banner block entities and items named by their translation key get the
//! `uncommon` rarity.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::{
    create_translatable_component_json, extract_translation_string,
};
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::value::Value;

/// `new OminousBannerRarityFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("OminousBannerRarityFix", output_schema, false, |df| {
        let input_schema = df.input_schema();
        let block_entity_type = input_schema.get_type(&References::BLOCK_ENTITY);
        let item_stack_type = input_schema.get_type(&References::ITEM_STACK);
        let block_entity_id_finder = input_schema.find_choice_type(&References::BLOCK_ENTITY);
        let item_stack_id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let block_entity_components_field_finder = block_entity_type.find_field("components");
        let item_stack_components_field_finder = item_stack_type.find_field("components");
        let item_name_finder = block_entity_components_field_finder
            .ty()
            .find_field("minecraft:item_name");
        let text_component_finder =
            DSL::type_finder(&input_schema.get_type(&References::TEXT_COMPONENT));
        let id_finder = block_entity_id_finder.ty().finder();
        let (item_name_finder2, text_component_finder2) =
            (item_name_finder.clone(), text_component_finder.clone());
        TypeRewriteRule::seq(
            df.fix_type_everywhere_typed(
                "Ominous Banner block entity common rarity to uncommon rarity fix",
                &block_entity_type,
                move |input| {
                    let block_entity_id = input.get(&id_finder);
                    if block_entity_id.first().as_str() == "minecraft:banner" {
                        fix(
                            input,
                            &block_entity_components_field_finder,
                            &item_name_finder,
                            &text_component_finder,
                        )
                    } else {
                        input
                    }
                },
            ),
            df.fix_type_everywhere_typed(
                "Ominous Banner item stack common rarity to uncommon rarity fix",
                &item_stack_type,
                move |input| {
                    let is_white_banner = input
                        .get_optional(&item_stack_id_finder)
                        .is_some_and(|id| id.second().as_str() == "minecraft:white_banner");
                    if is_white_banner {
                        fix(
                            input,
                            &item_stack_components_field_finder,
                            &item_name_finder2,
                            &text_component_finder2,
                        )
                    } else {
                        input
                    }
                },
            ),
        )
    })
}

fn fix(
    input: Typed,
    components_field_finder: &OpticFinder,
    item_name_finder: &OpticFinder,
    text_component_finder: &OpticFinder,
) -> Typed {
    input.update_typed(components_field_finder, |components| {
        let is_ominous_banner = components
            .get_optional_typed(item_name_finder)
            .and_then(|item_name| item_name.get_optional(text_component_finder))
            .and_then(|component| extract_translation_string(component.second().as_str()))
            .is_some_and(|key| key == "block.minecraft.ominous_banner");
        if is_ominous_banner {
            components
                .update_typed(item_name_finder, |item_name| {
                    item_name.set(
                        text_component_finder,
                        Value::named(
                            References::TEXT_COMPONENT.type_name(),
                            Value::str(create_translatable_component_json(
                                "block.minecraft.ominous_banner",
                            )),
                        ),
                    )
                })
                .update_remainder(|remainder| {
                    remainder.set("minecraft:rarity", remainder.create_string("uncommon"))
                })
        } else {
            components
        }
    })
}
