//! `ItemStackTheFlatteningFix`: gives an item stack the flattened name of its legacy id and `Damage`, and moves the
//! `Damage` of tools and weapons into their `tag`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::fixes::tables::item_stack_the_flattening_fix::{is_damage_id, update_item};
use crate::schema::Schema;
use crate::value::Value;

/// `new ItemStackTheFlatteningFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "ItemStackTheFlatteningFix",
        output_schema,
        changes_type,
        |df| {
            let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
            let id_f = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let tag_f = item_stack_type.find_field("tag");
            df.fix_type_everywhere_typed(
                "ItemInstanceTheFlatteningFix",
                &item_stack_type,
                move |input| {
                    let Some(id) = input.get_optional(&id_f) else {
                        return input;
                    };
                    let name = id.second().as_lossy_str();
                    let rest = input.get(&remainder_finder()).into_dynamic();
                    let data = rest.get("Damage").as_int_or(0);
                    let tag = is_damage_id(name).then(|| input.get_or_create_typed(&tag_f));
                    let mut output = match update_item(name, data) {
                        Some(new_value) => input.set(
                            &id_f,
                            Value::named(References::ITEM_NAME.type_name(), Value::str(new_value)),
                        ),
                        None => input,
                    };
                    if let Some(tag) = tag {
                        let tag_rest = tag.get(&remainder_finder()).into_dynamic();
                        let tag_rest = tag_rest.set("Damage", tag_rest.create_int(data));
                        output = output.set_typed(&tag_f, tag.set_remainder(tag_rest));
                    }
                    output.set_remainder(rest.remove("Damage"))
                },
            )
        },
    )
}
