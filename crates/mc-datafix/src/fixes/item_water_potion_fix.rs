//! `ItemWaterPotionFix`: gives potion items without a `Potion` tag the `minecraft:water` potion.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ItemWaterPotionFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemWaterPotionFix", output_schema, changes_type, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let tag_f = item_stack_type.find_field("tag");
        df.fix_type_everywhere_typed("ItemWaterPotionFix", &item_stack_type, move |input| {
            let is_potion = input.get_optional(&id_f).is_some_and(|id| {
                matches!(
                    id.second().as_str(),
                    "minecraft:potion"
                        | "minecraft:splash_potion"
                        | "minecraft:lingering_potion"
                        | "minecraft:tipped_arrow"
                )
            });
            if !is_potion {
                return input;
            }
            let tag = input.get_or_create_typed(&tag_f);
            let mut tag_rest = tag.get(&remainder_finder()).into_dynamic();
            if tag_rest.get("Potion").as_string().result().is_none() {
                tag_rest = tag_rest.set("Potion", tag_rest.create_string("minecraft:water"));
            }
            input.set_typed(&tag_f, tag.set_remainder(tag_rest))
        })
    })
}
