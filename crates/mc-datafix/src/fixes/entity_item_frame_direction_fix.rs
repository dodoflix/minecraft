//! `EntityItemFrameDirectionFix`: converts the 2D `Facing` of item frames to the 3D direction.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityItemFrameDirectionFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityItemFrameDirectionFix",
        output_schema,
        changes_type,
        "EntityItemFrameDirectionFix",
        &References::ENTITY,
        "minecraft:item_frame",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    let facing = direction_2d_to_3d(input.get("Facing").as_byte_or(0));
    input.set("Facing", input.create_byte(facing))
}

fn direction_2d_to_3d(dir: i8) -> i8 {
    match dir {
        0 => 3,
        1 => 4,
        3 => 5,
        _ => 2,
    }
}
