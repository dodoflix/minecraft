//! `HeightmapRenamingFix`: renames the heightmaps of `Level.Heightmaps` to their 1.14 names.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new HeightmapRenamingFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("HeightmapRenamingFix", output_schema, changes_type, |df| {
        let input_type = df.input_schema().get_type(&References::CHUNK);
        let level_f = input_type.find_field("Level");
        df.fix_type_everywhere_typed("HeightmapRenamingFix", &input_type, move |input| {
            input.update_typed(&level_f, |level| level.update_remainder(fix))
        })
    })
}

/// `fix(tag)`.
fn fix(tag: Dynamic) -> Dynamic {
    let Some(mut heightmaps_tag) = tag.get("Heightmaps").result() else {
        return tag;
    };
    if let Some(liquid) = heightmaps_tag.get("LIQUID").result() {
        heightmaps_tag = heightmaps_tag.remove("LIQUID");
        heightmaps_tag = heightmaps_tag.set("WORLD_SURFACE_WG", liquid);
    }
    if let Some(solid) = heightmaps_tag.get("SOLID").result() {
        heightmaps_tag = heightmaps_tag.remove("SOLID");
        heightmaps_tag = heightmaps_tag.set("OCEAN_FLOOR_WG", solid.clone());
        heightmaps_tag = heightmaps_tag.set("OCEAN_FLOOR", solid);
    }
    if let Some(light) = heightmaps_tag.get("LIGHT").result() {
        heightmaps_tag = heightmaps_tag.remove("LIGHT");
        heightmaps_tag = heightmaps_tag.set("LIGHT_BLOCKING", light);
    }
    if let Some(rain) = heightmaps_tag.get("RAIN").result() {
        heightmaps_tag = heightmaps_tag.remove("RAIN");
        heightmaps_tag = heightmaps_tag.set("MOTION_BLOCKING", rain.clone());
        heightmaps_tag = heightmaps_tag.set("MOTION_BLOCKING_NO_LEAVES", rain);
    }
    tag.set("Heightmaps", heightmaps_tag)
}
