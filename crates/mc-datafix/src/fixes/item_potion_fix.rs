//! `ItemPotionFix`: moves the legacy potion damage value of a potion item into the `Potion` tag (and the splash flag
//! into the item id).

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::fixes::tables;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::LazyLock;

static POTIONS: LazyLock<Vec<Option<String>>> = LazyLock::new(|| {
    tables::table("item_potion_fix")
        .field("POTIONS")
        .string_list()
});

/// `new ItemPotionFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemPotionFix", output_schema, changes_type, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let tag_finder = item_stack_type.find_field("tag");
        df.fix_type_everywhere_typed("ItemPotionFix", &item_stack_type, move |input| {
            let is_potion = input
                .get_optional(&id_finder)
                .is_some_and(|id| id.second().as_lossy_str() == "minecraft:potion");
            if !is_potion {
                return input;
            }
            let mut rest = input.get(&remainder_finder()).into_dynamic();
            let Some(tag) = input.get_optional_typed(&tag_finder) else {
                return input;
            };
            let damage = rest.get("Damage").as_short_or(0);
            let mut output = input.clone();
            let tag_rest = tag.get(&remainder_finder()).into_dynamic();
            if tag_rest.get("Potion").as_string().result().is_none() {
                let potion_name = POTIONS[(damage & 127) as usize].as_deref();
                let new_tag = tag.set_remainder(tag_rest.set(
                    "Potion",
                    tag_rest.create_string(potion_name.unwrap_or("minecraft:water")),
                ));
                output = input.set_typed(&tag_finder, new_tag);
                if damage & 16384 == 16384 {
                    output = output.set(
                        &id_finder,
                        Value::named(
                            References::ITEM_NAME.type_name(),
                            Value::str("minecraft:splash_potion"),
                        ),
                    );
                }
            }
            if damage != 0 {
                rest = rest.set("Damage", rest.create_short(0));
            }
            output.set_remainder(rest)
        })
    })
}
