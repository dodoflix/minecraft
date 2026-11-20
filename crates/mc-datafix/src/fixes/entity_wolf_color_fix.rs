//! `EntityWolfColorFix`: inverts the dye color (`15 - color`) of the wolf collar.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityWolfColorFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityWolfColorFix",
        output_schema,
        changes_type,
        "EntityWolfColorFix",
        &References::ENTITY,
        "minecraft:wolf",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    input.update("CollarColor", |color| {
        color.create_byte(15i32.wrapping_sub(color.as_int_or(0)) as i8)
    })
}
