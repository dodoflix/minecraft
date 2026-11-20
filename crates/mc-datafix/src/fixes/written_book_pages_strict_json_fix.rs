//! `WrittenBookPagesStrictJsonFix`: rewrites the lenient JSON of the pages of written books to strict JSON.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::item_stack_tag_fix;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::rewrite_from_lenient;
use crate::schema::Schema;
use crate::value::Value;

/// `new WrittenBookPagesStrictJsonFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    // changesType is false: the input schema is the output schema
    let input_schema = output_schema.clone();
    item_stack_tag_fix::new(
        "WrittenBookPagesStrictJsonFix",
        output_schema,
        "WrittenBookPagesStrictJsonFix",
        |id| id == "minecraft:written_book",
        move |tag| {
            let text_component_type = input_schema.get_type(&References::TEXT_COMPONENT);
            let item_stack_type = input_schema.get_type(&References::ITEM_STACK);
            let tag_f = item_stack_type.find_field("tag");
            let pages_f = tag_f.ty().find_field("pages");
            let page_f = DSL::type_finder(&text_component_type);
            tag.update_typed(&pages_f, |pages| {
                pages.update(&page_f, |page| {
                    page.map_second(|json| Value::Str(rewrite_from_lenient(json.as_str())))
                })
            })
        },
    )
}
