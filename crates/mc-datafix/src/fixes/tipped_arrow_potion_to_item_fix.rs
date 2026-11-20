//! `TippedArrowPotionToItemFix`: moves the potion of an arrow entity into its item stack.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new TippedArrowPotionToItemFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_write_read_fix::new(
        "TippedArrowPotionToItemFix",
        output_schema,
        false,
        "TippedArrowPotionToItemFix",
        &References::ENTITY,
        "minecraft:arrow",
        fix,
    )
}

fn fix(input: Dynamic) -> Dynamic {
    let potion = input.get("Potion").result();
    let custom_potion_effects = input.get("custom_potion_effects").result();
    let color = input.get("Color").result();
    if potion.is_none() && custom_potion_effects.is_none() && color.is_none() {
        return input;
    }
    input
        .remove("Potion")
        .remove("custom_potion_effects")
        .remove("Color")
        .update("item", |item_stack| {
            let mut tag = item_stack.get("tag").or_else_empty_map();
            if let Some(potion) = potion {
                tag = tag.set("Potion", potion);
            }
            if let Some(effects) = custom_potion_effects {
                tag = tag.set("custom_potion_effects", effects);
            }
            if let Some(color) = color {
                tag = tag.set("CustomPotionColor", color);
            }
            item_stack.set("tag", tag)
        })
}
