//! `BannerEntityCustomNameToOverrideComponentFix`: an ominous banner block entity named by its translation key uses the
//! `item_name` component.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::extract_translation_string;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;

/// `new BannerEntityCustomNameToOverrideComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "BannerEntityCustomNameToOverrideComponentFix",
        output_schema,
        false,
        |df| {
            let block_entity_type = df.input_schema().get_type(&References::BLOCK_ENTITY);
            let block_entity_id_finder = df
                .input_schema()
                .find_choice_type(&References::BLOCK_ENTITY);
            let custom_name_finder = block_entity_type.find_field("CustomName");
            let text_component_finder =
                DSL::type_finder(&df.input_schema().get_type(&References::TEXT_COMPONENT));
            let id_finder = block_entity_id_finder.ty().finder();
            df.fix_type_everywhere_typed(
                "Banner entity custom_name to item_name component fix",
                &block_entity_type,
                move |input| {
                    let block_entity_id = input.get(&id_finder);
                    if block_entity_id.first().as_lossy_str() == "minecraft:banner" {
                        fix(input, &text_component_finder, &custom_name_finder)
                    } else {
                        input
                    }
                },
            )
        },
    )
}

fn fix(
    input: Typed,
    text_component_finder: &OpticFinder,
    custom_name_finder: &OpticFinder,
) -> Typed {
    let custom_name = input
        .get_optional_typed(custom_name_finder)
        .and_then(|name| {
            name.get_optional(text_component_finder)
                .map(|component| component.second().as_java_string().clone())
        });
    let is_ominous_banner = custom_name
        .as_ref()
        .and_then(extract_translation_string)
        .is_some_and(|key| key == "block.minecraft.ominous_banner");
    if !is_ominous_banner {
        return input;
    }
    let custom_name = custom_name.expect("an ominous banner has a custom name");
    write_and_read_typed_or_throw(&input, &input.ty, |dynamic| {
        let components = dynamic.create_map([
            (
                dynamic.create_string("minecraft:item_name"),
                dynamic.create_string(custom_name),
            ),
            (
                dynamic.create_string("minecraft:hide_additional_tooltip"),
                dynamic.empty_map(),
            ),
        ]);
        dynamic.set("components", components).remove("CustomName")
    })
}
