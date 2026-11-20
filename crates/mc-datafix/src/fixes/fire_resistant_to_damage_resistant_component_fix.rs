//! `FireResistantToDamageResistantComponentFix`: `minecraft:fire_resistant` becomes `minecraft:damage_resistant`
//! covering the `#minecraft:is_fire` damage types.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;

/// `new FireResistantToDamageResistantComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new_renamed(
        "FireResistantToDamageResistantComponentFix",
        output_schema,
        "FireResistantToDamageResistantComponentFix",
        "minecraft:fire_resistant",
        "minecraft:damage_resistant",
        |input| {
            Some(
                input
                    .empty_map()
                    .set("types", input.create_string("#minecraft:is_fire")),
            )
        },
    )
}
