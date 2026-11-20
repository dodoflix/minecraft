//! `HorseBodyArmorItemFix`: moves the armor of a horse-like entity to the body armor item.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new HorseBodyArmorItemFix(outputSchema, entityName, previousBodyArmorTag, clearArmorItems)`.
pub fn new(
    output_schema: &Schema,
    entity_name: &str,
    previous_body_armor_tag: &str,
    clear_armor_items: bool,
) -> DataFix {
    let previous_body_armor_tag = previous_body_armor_tag.to_string();
    named_entity_write_read_fix::new(
        "HorseBodyArmorItemFix",
        output_schema,
        true,
        &format!("Horse armor fix for {entity_name}"),
        &References::ENTITY,
        entity_name,
        move |input| fix(input, &previous_body_armor_tag, clear_armor_items),
    )
}

fn fix(input: Dynamic, previous_body_armor_tag: &str, clear_armor_items: bool) -> Dynamic {
    let Some(body_armor_item) = input.get(previous_body_armor_tag).result() else {
        return input;
    };
    let mut output = input.remove(previous_body_armor_tag);
    if clear_armor_items {
        output = output.update("ArmorItems", |armor_items| {
            let entries: Vec<Dynamic> = armor_items
                .as_stream()
                .into_iter()
                .enumerate()
                .map(|(index, entry)| if index == 2 { entry.empty_map() } else { entry })
                .collect();
            armor_items.create_list(entries)
        });
        output = output.update("ArmorDropChances", |armor_drop_chances| {
            let entries: Vec<Dynamic> = armor_drop_chances
                .as_stream()
                .into_iter()
                .enumerate()
                .map(|(index, entry)| {
                    if index == 2 {
                        entry.create_float(0.085)
                    } else {
                        entry
                    }
                })
                .collect();
            armor_drop_chances.create_list(entries)
        });
    }
    output = output.set("body_armor_item", body_armor_item);
    output.set("body_armor_drop_chance", input.create_float(2.0))
}
