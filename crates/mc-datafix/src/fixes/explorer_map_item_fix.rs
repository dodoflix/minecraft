//! `ExplorerMapItemFix`: filled maps with an explorer decoration become the dedicated map items.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::get_plain_translation_key;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// `DECORATION_TYPE_TO_ITEM_ID` and `DECORATION_TYPE_TO_LEGACY_NAME_KEY` (item id, legacy name key) by decoration type.
fn decoration(decoration_type: &str) -> Option<(&'static str, &'static str)> {
    Some(match decoration_type {
        "minecraft:mansion" => ("minecraft:woodland_explorer_map", "filled_map.mansion"),
        "minecraft:monument" => ("minecraft:ocean_explorer_map", "filled_map.monument"),
        "minecraft:trial_chambers" => ("minecraft:trial_explorer_map", "filled_map.trial_chambers"),
        "minecraft:jungle_temple" => (
            "minecraft:jungle_explorer_map",
            "filled_map.explorer_jungle",
        ),
        "minecraft:swamp_hut" => ("minecraft:swamp_explorer_map", "filled_map.explorer_swamp"),
        "minecraft:village_desert" => ("minecraft:desert_village_map", "filled_map.village_desert"),
        "minecraft:village_plains" => ("minecraft:plains_village_map", "filled_map.village_plains"),
        "minecraft:village_savanna" => (
            "minecraft:savanna_village_map",
            "filled_map.village_savanna",
        ),
        "minecraft:village_snowy" => ("minecraft:snowy_village_map", "filled_map.village_snowy"),
        "minecraft:village_taiga" => ("minecraft:taiga_village_map", "filled_map.village_taiga"),
        "minecraft:red_x" => (
            "minecraft:buried_treasure_map",
            "filled_map.buried_treasure",
        ),
        _ => return None,
    })
}

/// `new ExplorerMapItemFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ExplorerMapItemFix", output_schema, false, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let components_finder = item_stack_type.find_field("components");
        df.fix_type_everywhere_typed("ExplorerMapItemFix", &item_stack_type, move |item_stack| {
            let is_filled_map = item_stack
                .get_optional(&id_finder)
                .is_some_and(|id| id.second().as_str() == "minecraft:filled_map");
            if !is_filled_map {
                return item_stack;
            }
            let Some(components) = item_stack.get_optional_typed(&components_finder) else {
                return item_stack;
            };
            let components_data = components.get_remainder();
            let Some(map_decorations) = components_data.get("minecraft:map_decorations").result()
            else {
                return item_stack;
            };
            let Some(explorer_decoration) = map_decorations.get("+").result() else {
                return item_stack;
            };
            let decoration_type =
                ensure_namespaced(&explorer_decoration.get("type").as_string_or(""));
            let Some((new_item_id, legacy_name_key)) = decoration(&decoration_type) else {
                return item_stack;
            };
            let item_type = item_stack.get_type().clone();
            write_and_read_typed_or_throw(&item_stack, &item_type, |data| {
                data.set("id", data.create_string(new_item_id))
                    .update("components", |component_data| {
                        remove_legacy_item_name(component_data, legacy_name_key)
                    })
            })
        })
    })
}

/// `ExplorerMapItemFix.removeLegacyItemName(components, legacyNameKey)`.
fn remove_legacy_item_name(components: Dynamic, legacy_name_key: &str) -> Dynamic {
    let holds_legacy_name = components
        .get("minecraft:item_name")
        .result()
        .and_then(|name| get_plain_translation_key(&name))
        .is_some_and(|name_key| name_key == legacy_name_key);
    if holds_legacy_name {
        components.remove("minecraft:item_name")
    } else {
        components
    }
}
