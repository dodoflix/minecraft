//! `ItemIdFix`: replaces the numeric item id of an item stack by its namespaced item name.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::fixes::tables;
use crate::schema::Schema;
use crate::value::Value;

/// `ItemIdFix.getItem(id)`: the item name of a legacy numeric id, `minecraft:air` for an unknown one.
pub fn get_item(id: i32) -> &'static str {
    tables::item_id_fix::get_item(id)
}

/// `new ItemIdFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("ItemIdFix", output_schema, changes_type, |df| {
        let new_item_type =
            DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string());
        let old_item_type = DSL::or_type(&DSL::int_type(), &new_item_type);
        let id = DSL::field_finder("id", &old_item_type);
        df.fix_type_everywhere_typed_new(
            "ItemIdFix",
            &df.input_schema().get_type(&References::ITEM_STACK),
            &df.output_schema().get_type(&References::ITEM_STACK),
            move |item_stack| {
                item_stack.update_as(&id, &new_item_type, |old_id| match old_id {
                    Value::Left(l) => Value::named(
                        References::ITEM_NAME.type_name(),
                        Value::str(get_item(l.as_int())),
                    ),
                    Value::Right(r) => *r,
                    other => panic!("expected an Either, got {other:?}"),
                })
            },
        )
    })
}
