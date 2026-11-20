//! `MapColorComponentRemovalFix`: removes the `minecraft:map_color` component.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;

/// `new MapColorComponentRemovalFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new(
        "MapColorComponentRemovalFix",
        output_schema,
        "MapColorComponentRemovalFix",
        "minecraft:map_color",
        |_| None,
    )
}
