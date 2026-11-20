//! `BlockEntitySignDoubleSidedEditableTextFix`: moves the text of a sign into its front side and adds an empty back side.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_empty_component;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `BlockEntitySignDoubleSidedEditableTextFix.FIELDS_TO_DROP`.
pub const FIELDS_TO_DROP: [&str; 10] = [
    "Text1",
    "Text2",
    "Text3",
    "Text4",
    "FilteredText1",
    "FilteredText2",
    "FilteredText3",
    "FilteredText4",
    "Color",
    "GlowingText",
];

/// `new BlockEntitySignDoubleSidedEditableTextFix(outputSchema, name, entityName)`.
pub fn new(output_schema: &Schema, name: &str, entity_name: &str) -> DataFix {
    named_entity_write_read_fix::new(
        "BlockEntitySignDoubleSidedEditableTextFix",
        output_schema,
        true,
        name,
        &References::BLOCK_ENTITY,
        entity_name,
        fix,
    )
}

fn fix(input: Dynamic) -> Dynamic {
    let mut input = input
        .set("front_text", fix_front_text_tag(&input))
        .set("back_text", create_default_text(&input));
    input = input.set("is_waxed", input.create_boolean(false));
    input = input.set("_filtered_correct", input.create_boolean(true));
    for field in FIELDS_TO_DROP {
        input = input.remove(field);
    }
    input
}

fn fix_front_text_tag(tag: &Dynamic) -> Dynamic {
    let empty_line = create_empty_component(tag.ops());
    let lines: Vec<Dynamic> = get_lines(tag, "Text")
        .into_iter()
        .map(|line| line.unwrap_or_else(|| empty_line.clone()))
        .collect();
    let mut text = tag
        .empty_map()
        .set("messages", tag.create_list(lines.clone()));
    text = text.set(
        "color",
        tag.get("Color")
            .result()
            .unwrap_or_else(|| tag.create_string("black")),
    );
    text = text.set(
        "has_glowing_text",
        tag.get("GlowingText")
            .result()
            .unwrap_or_else(|| tag.create_boolean(false)),
    );
    let filtered_lines = get_lines(tag, "FilteredText");
    if filtered_lines.iter().any(Option::is_some) {
        let filtered: Vec<Dynamic> = filtered_lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| line.unwrap_or_else(|| lines[index].clone()))
            .collect();
        text = text.set("filtered_messages", tag.create_list(filtered));
    }
    text
}

fn get_lines(tag: &Dynamic, line_prefix: &str) -> Vec<Option<Dynamic>> {
    (1..=4)
        .map(|i| tag.get(&format!("{line_prefix}{i}")).result())
        .collect()
}

fn create_default_text(tag: &Dynamic) -> Dynamic {
    tag.empty_map()
        .set("messages", create_empty_lines(tag))
        .set("color", tag.create_string("black"))
        .set("has_glowing_text", tag.create_boolean(false))
}

fn create_empty_lines(tag: &Dynamic) -> Dynamic {
    let empty_component = create_empty_component(tag.ops());
    tag.create_list(std::iter::repeat_n(empty_component, 4))
}
