//! `LodestoneCompassComponentFix`: nests the position and dimension of a lodestone target in `target`.

use crate::datafix::DataFix;
use crate::fixes::data_component_remainder_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new LodestoneCompassComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    data_component_remainder_fix::new_renamed(
        "LodestoneCompassComponentFix",
        output_schema,
        "LodestoneCompassComponentFix",
        "minecraft:lodestone_target",
        "minecraft:lodestone_tracker",
        |input| Some(fix_component(input)),
    )
}

fn fix_component(input: Dynamic) -> Dynamic {
    let pos = input.get("pos").result();
    let dimension = input.get("dimension").result();
    let input = input.remove("pos").remove("dimension");
    match (pos, dimension) {
        (Some(pos), Some(dimension)) => {
            let target = input
                .empty_map()
                .set("pos", pos)
                .set("dimension", dimension);
            input.set("target", target)
        }
        _ => input,
    }
}
