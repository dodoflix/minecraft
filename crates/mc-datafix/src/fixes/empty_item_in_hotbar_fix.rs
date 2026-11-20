//! `EmptyItemInHotbarFix`: replaces air and empty item stacks of a hotbar by the empty stack.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::value::Value;

/// `new EmptyItemInHotbarFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("EmptyItemInHotbarFix", output_schema, false, |df| {
        let item_stack_f = DSL::type_finder(&df.input_schema().get_type(&References::ITEM_STACK));
        df.fix_type_everywhere_typed(
            "EmptyItemInHotbarFix",
            &df.input_schema().get_type(&References::HOTBAR),
            move |input| {
                input.update(&item_stack_f, |named_stack| {
                    named_stack.map_second(|item_stack| {
                        let id = item_stack
                            .first()
                            .as_option()
                            .map(|p| p.second().as_lossy_str());
                        let remainder = item_stack.second().second().as_dynamic();
                        let is_air = id.is_none_or(|id| id == "minecraft:air");
                        let is_empty = remainder.get("Count").as_int_or(0) <= 0;
                        if !is_air && !is_empty {
                            item_stack
                        } else {
                            Value::pair(
                                Value::none(),
                                Value::pair(Value::none(), Value::Dynamic(remainder.empty_map())),
                            )
                        }
                    })
                })
            },
        )
    })
}
