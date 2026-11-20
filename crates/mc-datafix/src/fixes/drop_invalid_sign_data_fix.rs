//! `DropInvalidSignDataFix`: re-reads signs whose filtered text was not converted, repairs the filtered lines and drops
//! the legacy text fields.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::block_entity_sign_double_sided_editable_text_fix::FIELDS_TO_DROP;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_empty_component;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// `new DropInvalidSignDataFix(outputSchema, entityName)`.
pub fn new(output_schema: &Schema, entity_name: &str) -> DataFix {
    let entity_name = entity_name.to_string();
    DataFix::new("DropInvalidSignDataFix", output_schema, false, move |df| {
        let entity_type = df.input_schema().get_type(&References::BLOCK_ENTITY);
        let entity_choice_type = df
            .input_schema()
            .get_choice_type(&References::BLOCK_ENTITY, &entity_name);
        let entity_f = DSL::named_choice(&entity_name, &entity_choice_type);
        let choice_type = entity_choice_type.clone();
        df.fix_type_everywhere_typed(
            &format!("DropInvalidSignDataFix for {entity_name}"),
            &entity_type,
            move |input| {
                let choice_type = choice_type.clone();
                input.update_typed_as(&entity_f, &entity_choice_type, move |entity| {
                    let filtered_correct = entity
                        .get_remainder()
                        .get("_filtered_correct")
                        .as_boolean_or(false);
                    if filtered_correct {
                        entity.update_remainder(|remainder| remainder.remove("_filtered_correct"))
                    } else {
                        write_and_read_typed_or_throw(&entity, &choice_type, fix)
                    }
                })
            },
        )
    })
}

fn fix(tag: Dynamic) -> Dynamic {
    let mut tag = tag.update("front_text", fix_text);
    tag = tag.update("back_text", fix_text);
    for field in FIELDS_TO_DROP {
        tag = tag.remove(field);
    }
    tag
}

fn fix_text(tag: Dynamic) -> Dynamic {
    let Some(filtered_lines) = tag.get("filtered_messages").as_stream_opt().result() else {
        return tag;
    };
    let empty_component = create_empty_component(tag.ops());
    let lines: Vec<Dynamic> = tag
        .get("messages")
        .as_stream_opt()
        .result()
        .unwrap_or_default();
    let new_filtered_lines: Vec<Dynamic> = filtered_lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let fallback_line = lines.get(index).unwrap_or(&empty_component).clone();
            if line == empty_component {
                fallback_line
            } else {
                line
            }
        })
        .collect();
    if new_filtered_lines == lines {
        tag.remove("filtered_messages")
    } else {
        tag.set("filtered_messages", tag.create_list(new_filtered_lines))
    }
}
