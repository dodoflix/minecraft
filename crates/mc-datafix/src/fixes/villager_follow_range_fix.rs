//! `VillagerFollowRangeFix`: raises a villager's `generic.follow_range` base from 16 to 48.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const ORIGINAL_VALUE: f64 = 16.0;
const NEW_BASE_VALUE: f64 = 48.0;

/// `new VillagerFollowRangeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "VillagerFollowRangeFix",
        output_schema,
        false,
        "Villager Follow Range Fix",
        &References::ENTITY,
        "minecraft:villager",
        |entity| entity.update_remainder(fix_value),
    )
}

fn fix_value(tag: Dynamic) -> Dynamic {
    tag.update("Attributes", |attributes| {
        tag.create_list(attributes.as_stream().into_iter().map(|attribute| {
            if attribute.get("Name").as_string_or("") == "generic.follow_range"
                && attribute.get("Base").as_double_or(0.0) == ORIGINAL_VALUE
            {
                attribute.set("Base", attribute.create_double(NEW_BASE_VALUE))
            } else {
                attribute
            }
        }))
    })
}
