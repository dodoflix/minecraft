//! `ItemShulkerBoxColorFix`: turns a `minecraft:shulker_box` item with a `BlockEntityTag.Color` into the colored
//! shulker box item.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::fixes::tables;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::LazyLock;

static NAMES_BY_COLOR: LazyLock<Vec<Option<String>>> = LazyLock::new(|| {
    tables::table("item_shulker_box_color_fix")
        .field("NAMES_BY_COLOR")
        .string_list()
});

/// `new ItemShulkerBoxColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ItemShulkerBoxColorFix",
        output_schema,
        changes_type,
        |df| {
            let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
            let id_f = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let tag_f = item_stack_type.find_field("tag");
            let block_entity_f = tag_f.ty().find_field("BlockEntityTag");
            df.fix_type_everywhere_typed("ItemShulkerBoxColorFix", &item_stack_type, move |input| {
            let is_shulker_box = input
                .get_optional(&id_f)
                .is_some_and(|id| id.second().as_lossy_str() == "minecraft:shulker_box");
            if !is_shulker_box {
                return input;
            }
            let Some(tag) = input.get_optional_typed(&tag_f) else {
                return input;
            };
            let Some(block_entity) = tag.get_optional_typed(&block_entity_f) else {
                return input;
            };
            let block_entity_rest = block_entity.get(&remainder_finder()).into_dynamic();
            let color = block_entity_rest.get("Color").as_int_or(0);
            // vanilla discards the result of `blockEntityRest.remove("Color")`, so `Color` stays
            let input = input.set_typed(
                &tag_f,
                tag.set_typed(&block_entity_f, block_entity.set_remainder(block_entity_rest)),
            );
            let index = color % 16;
            let name = usize::try_from(index)
                .ok()
                .and_then(|i| NAMES_BY_COLOR[i].as_deref())
                .unwrap_or_else(|| {
                    panic!(
                        "java.lang.ArrayIndexOutOfBoundsException: Index {index} out of bounds for length 16"
                    )
                });
            input.set(
                &id_f,
                Value::named(References::ITEM_NAME.type_name(), Value::str(name)),
            )
        })
        },
    )
}
