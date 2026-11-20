//! `ItemCustomNameToComponentFix`: converts the plain string `display.Name` of items to a text component.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_text_component_json;
use crate::schema::Schema;
use crate::value::Value;

/// `new ItemCustomNameToComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ItemCustomNameToComponentFix", output_schema, false, |df| {
        let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
        let text_component_type = df.input_schema().get_type(&References::TEXT_COMPONENT);
        let tag_finder = item_stack_type.find_field("tag");
        let display_finder = tag_finder.ty().find_field("display");
        let custom_name_finder = display_finder.ty().find_field("Name");
        let text_component_finder = DSL::type_finder(&text_component_type);
        df.fix_type_everywhere_typed(
            "ItemCustomNameToComponentFix",
            &item_stack_type,
            move |item_stack| {
                item_stack.update_typed(&tag_finder, |tag| {
                    tag.update_typed(&display_finder, |display| {
                        display.update_typed(&custom_name_finder, |custom_name| {
                            custom_name.update(&text_component_finder, |text_component| {
                                text_component.map_second(|text| {
                                    Value::Str(create_text_component_json(
                                        text.as_java_string().clone(),
                                    ))
                                })
                            })
                        })
                    })
                })
            },
        )
    })
}
