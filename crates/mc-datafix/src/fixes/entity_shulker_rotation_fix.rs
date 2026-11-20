//! `EntityShulkerRotationFix`: subtracts 180 from the first `Rotation` entry of shulkers.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityShulkerRotationFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "EntityShulkerRotationFix",
        output_schema,
        false,
        "EntityShulkerRotationFix",
        &References::ENTITY,
        "minecraft:shulker",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    let mut rotation: Vec<f64> = input.get("Rotation").as_list(|d| d.as_double_or(180.0));
    if rotation.is_empty() {
        return input;
    }
    rotation[0] -= 180.0;
    input.set(
        "Rotation",
        input.create_list(rotation.into_iter().map(|d| input.create_double(d))),
    )
}
