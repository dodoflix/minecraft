//! `BlockEntityShulkerBoxColorFix`: removes the `Color` of shulker box block entities.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new BlockEntityShulkerBoxColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "BlockEntityShulkerBoxColorFix",
        output_schema,
        changes_type,
        "BlockEntityShulkerBoxColorFix",
        &References::BLOCK_ENTITY,
        "minecraft:shulker_box",
        |entity| entity.update_remainder(|tag| tag.remove("Color")),
    )
}
