//! `TridentAnimationFix`: renames the `spear` animation of the `minecraft:consumable` component to `trident`.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;

/// `new TridentAnimationFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new(
        "TridentAnimationFix",
        output_schema,
        "TridentAnimationFix",
        "minecraft:consumable",
        |input| {
            Some(input.update("animation", |animation| {
                let optional = animation.as_string().result().unwrap_or_default();
                if optional == "spear" {
                    animation.create_string("trident")
                } else {
                    animation
                }
            }))
        },
    )
}
