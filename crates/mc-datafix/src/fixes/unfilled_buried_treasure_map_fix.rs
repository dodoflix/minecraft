//! `UnfilledBuriedTreasureMapFix`: an empty map keeps no buried treasure map name.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::get_plain_translation_key;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;

/// `new UnfilledBuriedTreasureMapFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("UnfilledBuriedTreasureMapFix", output_schema, false, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let components_finder = item_stack_type.find_field("components");
        df.fix_type_everywhere_typed(
            "UnfilledBuriedTreasureMapFix",
            &item_stack_type,
            move |item_stack| {
                let is_map = item_stack
                    .get_optional(&id_finder)
                    .is_some_and(|id| id.second().as_str() == "minecraft:map");
                if is_map {
                    item_stack.update_typed(&components_finder, |typed| {
                        let ty = typed.get_type().clone();
                        write_and_read_typed_or_throw(&typed, &ty, |value| {
                            let holds_legacy_name = value
                                .get("minecraft:item_name")
                                .result()
                                .and_then(|name| get_plain_translation_key(&name))
                                .is_some_and(|name_key| name_key == "filled_map.buried_treasure");
                            if holds_legacy_name {
                                value.update("minecraft:item_name", |item_name| {
                                    item_name.set(
                                        "translate",
                                        item_name
                                            .create_string("item.minecraft.buried_treasure_map"),
                                    )
                                })
                            } else {
                                value
                            }
                        })
                    })
                } else {
                    item_stack
                }
            },
        )
    })
}
