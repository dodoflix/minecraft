//! `ItemStackCustomNameToOverrideComponentFix`: banners and maps named by their translation key use `item_name`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::extract_translation_string;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;

const MAP_NAMES: [&str; 11] = [
    "filled_map.buried_treasure",
    "filled_map.explorer_jungle",
    "filled_map.explorer_swamp",
    "filled_map.mansion",
    "filled_map.monument",
    "filled_map.trial_chambers",
    "filled_map.village_desert",
    "filled_map.village_plains",
    "filled_map.village_savanna",
    "filled_map.village_snowy",
    "filled_map.village_taiga",
];

/// `new ItemStackCustomNameToOverrideComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "ItemStackCustomNameToOverrideComponentFix",
        output_schema,
        false,
        |df| {
            let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
            let id_finder = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let components_finder = item_stack_type.find_field("components");
            df.fix_type_everywhere_typed(
                "ItemStack custom_name to item_name component fix",
                &item_stack_type,
                move |input| {
                    let id = input
                        .get_optional(&id_finder)
                        .map(|id| id.second().as_str().to_string());
                    match id.as_deref() {
                        Some("minecraft:white_banner") => {
                            input.update_typed(&components_finder, fix_banner)
                        }
                        Some("minecraft:filled_map") => {
                            input.update_typed(&components_finder, fix_map)
                        }
                        _ => input,
                    }
                },
            )
        },
    )
}

fn fix_map(value: Typed) -> Typed {
    fix_custom_name(value, |key| MAP_NAMES.contains(&key))
}

fn fix_banner(value: Typed) -> Typed {
    fix_custom_name(value, |key| key == "block.minecraft.ominous_banner")
}

fn fix_custom_name(typed: Typed, expected_translation_key: impl Fn(&str) -> bool) -> Typed {
    write_and_read_typed_or_throw(&typed, &typed.ty, |value| {
        let has_correct_translation_key = value
            .get("minecraft:custom_name")
            .as_string()
            .result()
            .and_then(|name| extract_translation_string(&name))
            .filter(|key| expected_translation_key(key))
            .is_some();
        if has_correct_translation_key {
            value.rename_field("minecraft:custom_name", "minecraft:item_name")
        } else {
            value
        }
    })
}
