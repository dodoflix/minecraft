//! `LegacyJigsawRotationFix`: turns the jigsaw block's `facing` property into the `orientation` property.

use crate::datafix::DataFix;
use crate::fixes::legacy_abstract_block_property_fix;
use crate::schema::Schema;

/// `new LegacyJigsawRotationFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    legacy_abstract_block_property_fix::new(
        "LegacyJigsawRotationFix",
        output_schema,
        "jigsaw_rotation_fix",
        |block_id| block_id == "minecraft:jigsaw",
        |_block_id, properties| {
            let facing = properties.get("facing").as_string_or("north");
            let orientation = match facing.as_str() {
                "down" => "down_south",
                "up" => "up_north",
                "north" => "north_up",
                "south" => "south_up",
                "west" => "west_up",
                "east" => "east_up",
                other => other,
            };
            properties
                .remove("facing")
                .set("orientation", properties.create_string(orientation))
        },
    )
}
