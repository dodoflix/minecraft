//! `ItemBannerColorFix`: moves the banner `BlockEntityTag.Base` color into the item's `Damage`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ItemBannerColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemBannerColorFix", output_schema, changes_type, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let tag_f = item_stack_type.find_field("tag");
        let block_entity_f = tag_f.ty().find_field("BlockEntityTag");
        df.fix_type_everywhere_typed("ItemBannerColorFix", &item_stack_type, move |input| {
            let is_banner = input
                .get_optional(&id_f)
                .is_some_and(|id| id.second().as_str() == "minecraft:banner");
            if !is_banner {
                return input;
            }
            let mut rest = input.get(&remainder_finder()).into_dynamic();
            if let Some(tag) = input.get_optional_typed(&tag_f)
                && let Some(block_entity) = tag.get_optional_typed(&block_entity_f)
            {
                let tag_rest = tag.get(&remainder_finder()).into_dynamic();
                let block_entity_rest = block_entity.get_remainder();
                if block_entity_rest.get("Base").as_number().result().is_some() {
                    rest = rest.set(
                        "Damage",
                        rest.create_short((block_entity_rest.get("Base").as_int_or(0) & 15) as i16),
                    );
                    if let Some(display) = tag_rest.get("display").result() {
                        let pick_marker = display.create_map([(
                            display.create_string("Lore"),
                            display.create_list([display.create_string("(+NBT")]),
                        )]);
                        if display == pick_marker {
                            return input.set_remainder(rest);
                        }
                    }
                    // vanilla discards the result of `blockEntityRest.remove("Base")`, so `Base` stays
                    return input.set_remainder(rest).set_typed(
                        &tag_f,
                        tag.set_typed(
                            &block_entity_f,
                            block_entity.set_remainder(block_entity_rest),
                        ),
                    );
                }
            }
            input.set_remainder(rest)
        })
    })
}
