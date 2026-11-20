//! `EntityShulkerColorFix`: gives shulkers without a `Color` the default colour 10.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityShulkerColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityShulkerColorFix",
        output_schema,
        changes_type,
        "EntityShulkerColorFix",
        &References::ENTITY,
        "minecraft:shulker",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    // Java maps `Dynamic::asNumber` over the lookup, so only a missing `Color` counts as absent
    if input.get("Color").is_present() {
        input
    } else {
        input.set("Color", input.create_byte(10))
    }
}
