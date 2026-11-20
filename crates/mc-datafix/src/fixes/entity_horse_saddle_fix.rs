//! `EntityHorseSaddleFix`: horses with `Saddle: true` and no `SaddleItem` get a saddle item stack.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;

/// `new EntityHorseSaddleFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    let input_schema = if changes_type {
        output_schema
            .parent()
            .expect("a type-changing fix needs a parent schema")
            .clone()
    } else {
        output_schema.clone()
    };
    named_entity_fix::new(
        "EntityHorseSaddleFix",
        output_schema,
        changes_type,
        "EntityHorseSaddleFix",
        &References::ENTITY,
        "EntityHorse",
        move |entity| {
            let id_f = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let item_stack_type = input_schema.get_type_raw(&References::ITEM_STACK);
            let saddle_f = DSL::field_finder("SaddleItem", &item_stack_type);
            let saddle = entity.get_optional_typed(&saddle_f);
            let tag = entity.get(&DSL::remainder_finder()).into_dynamic();
            if saddle.is_none() && tag.get("Saddle").as_boolean_or(false) {
                let new_saddle = item_stack_type
                    .point_typed(entity.get_ops())
                    .unwrap_or_else(|| panic!("java.lang.IllegalStateException"));
                let mut new_saddle = new_saddle.set(
                    &id_f,
                    Value::named(
                        References::ITEM_NAME.type_name(),
                        Value::str("minecraft:saddle"),
                    ),
                );
                let saddle_tag = tag.empty_map();
                let saddle_tag = saddle_tag.set("Count", saddle_tag.create_byte(1));
                let saddle_tag = saddle_tag.set("Damage", saddle_tag.create_short(0));
                new_saddle = new_saddle.set_remainder(saddle_tag);
                // Java discards the result of `tag.remove("Saddle")`, so the flag stays
                entity.set_typed(&saddle_f, new_saddle).set_remainder(tag)
            } else {
                entity
            }
        },
    )
}
