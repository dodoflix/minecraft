//! `RenameNameplateToNameTagFix`: renames the `minecraft:nameplate_distance` attribute to `minecraft:name_tag_distance`.

use crate::datafix::DataFix;
use crate::fixes::attributes_rename_fix;
use crate::schema::Schema;

/// `new RenameNameplateToNameTagFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    attributes_rename_fix::new(
        "RenameNameplateToNameTagFix",
        output_schema,
        "RenameNameplateToNameTag",
        rename,
    )
}

fn rename(old_name: &str) -> String {
    if old_name == "minecraft:nameplate_distance" {
        "minecraft:name_tag_distance".to_string()
    } else {
        old_name.to_string()
    }
}
