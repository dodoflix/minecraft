//! `BlockEntityJukeboxFix`: converts the numeric `Record` of jukeboxes to a `RecordItem` stack.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::item_id_fix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::fixes::tables::item_stack_the_flattening_fix;
use crate::schema::Schema;

/// `new BlockEntityJukeboxFix(outputSchema, changesType)`.
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
        "BlockEntityJukeboxFix",
        output_schema,
        changes_type,
        "BlockEntityJukeboxFix",
        &References::BLOCK_ENTITY,
        "minecraft:jukebox",
        move |entity| {
            let jukebox_type =
                input_schema.get_choice_type(&References::BLOCK_ENTITY, "minecraft:jukebox");
            let item_stack_type = jukebox_type.find_field_type("RecordItem");
            let record_item_f = DSL::field_finder("RecordItem", &item_stack_type);
            let tag = entity.get(&DSL::remainder_finder()).into_dynamic();
            let record_id = tag.get("Record").as_int_or(0);
            if record_id > 0 {
                // vanilla discards the result of `tag.remove("Record")`, so `Record` stays
                if let Some(id) =
                    item_stack_the_flattening_fix::update_item(item_id_fix::get_item(record_id), 0)
                {
                    let item_tag = tag.empty_map();
                    let item_tag = item_tag.set("id", item_tag.create_string(id));
                    let item_tag = item_tag.set("Count", item_tag.create_byte(1));
                    let record_item = item_stack_type
                        .read_typed(&item_tag)
                        .result()
                        .unwrap_or_else(|| {
                            panic!(
                                "java.lang.IllegalStateException: Could not create record item stack."
                            )
                        })
                        .0;
                    return entity
                        .set_typed(&record_item_f, record_item)
                        .set_remainder(tag);
                }
            }
            entity
        },
    )
}
