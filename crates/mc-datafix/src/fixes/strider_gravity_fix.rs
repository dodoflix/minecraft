//! `StriderGravityFix`: striders with `NoGravity: true` get it reset to `false`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new StriderGravityFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "StriderGravityFix",
        output_schema,
        changes_type,
        "StriderGravityFix",
        &References::ENTITY,
        "minecraft:strider",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    if input.get("NoGravity").as_boolean_or(false) {
        input.set("NoGravity", input.create_boolean(false))
    } else {
        input
    }
}
