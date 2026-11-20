//! `ItemStackMapIdFix`: moves the `Damage` of filled maps to the `map` tag.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ItemStackMapIdFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemStackMapIdFix", output_schema, changes_type, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let id_f = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let tag_f = item_stack_type.find_field("tag");
        df.fix_type_everywhere_typed("ItemInstanceMapIdFix", &item_stack_type, move |input| {
            let is_filled_map = input
                .get_optional(&id_f)
                .is_some_and(|id| id.second().as_str() == "minecraft:filled_map");
            if !is_filled_map {
                return input;
            }
            let rest = input.get(&DSL::remainder_finder()).into_dynamic();
            let tag = input.get_or_create_typed(&tag_f);
            let tag_rest = tag.get(&DSL::remainder_finder()).into_dynamic();
            let tag_rest =
                tag_rest.set("map", tag_rest.create_int(rest.get("Damage").as_int_or(0)));
            input.set_typed(&tag_f, tag.set_remainder(tag_rest))
        })
    })
}
