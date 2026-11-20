//! `ItemRenameFix`: renames item ids (the `ITEM_NAME` type). Abstract in Java (`fix_item` is the subclass's
//! `fixItem`); [`create`] is the `ItemRenameFix.create` factory.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;
use std::sync::Arc;

/// `new ItemRenameFix(outputSchema, name)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    fix_item: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let fix_item = Arc::new(fix_item);
    DataFix::new(class, output_schema, false, move |df| {
        let item_name_type =
            DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string());
        if df.input_schema().get_type(&References::ITEM_NAME) != item_name_type {
            panic!("java.lang.IllegalStateException: item name type is not what was expected.");
        }
        let fix_item = fix_item.clone();
        df.fix_type_everywhere(&name, &item_name_type, move |_, input| {
            input.map_second(|item| Value::Str(item.as_java_string().map_lossless(|t| fix_item(t))))
        })
    })
}

/// `ItemRenameFix.create(outputSchema, name, fixItem)`.
pub fn create(
    output_schema: &Schema,
    name: &str,
    fix_item: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    new("ItemRenameFix$1", output_schema, name, fix_item)
}
