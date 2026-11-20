//! `EntityArmorStandSilentFix`: drops `Silent` from armor stands that are silent and not markers.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityArmorStandSilentFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityArmorStandSilentFix",
        output_schema,
        changes_type,
        "EntityArmorStandSilentFix",
        &References::ENTITY,
        "ArmorStand",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    if input.get("Silent").as_boolean_or(false) && !input.get("Marker").as_boolean_or(false) {
        input.remove("Silent")
    } else {
        input
    }
}
