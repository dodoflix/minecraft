//! `AreaEffectCloudDurationScaleFix`: area effect clouds get a `potion_duration_scale` of 0.25.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new AreaEffectCloudDurationScaleFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "AreaEffectCloudDurationScaleFix",
        output_schema,
        false,
        "AreaEffectCloudDurationScaleFix",
        &References::ENTITY,
        "minecraft:area_effect_cloud",
        |entity| {
            entity.update_remainder(|tag| tag.set("potion_duration_scale", tag.create_float(0.25)))
        },
    )
}
