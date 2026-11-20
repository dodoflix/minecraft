//! `EntityCatSplitFix`: renames tamed `minecraft:ocelot` entities (cat types 1..3) to `minecraft:cat`.

use crate::datafix::DataFix;
use crate::fixes::simple_entity_rename_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityCatSplitFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simple_entity_rename_fix::new(
        "EntityCatSplitFix",
        "EntityCatSplitFix",
        output_schema,
        changes_type,
        get_new_name_and_tag,
    )
}

/// `EntityCatSplitFix.getNewNameAndTag(name, tag)`; the `Trusting` flag set for `CatType` 0 is discarded, as in vanilla.
fn get_new_name_and_tag(name: &str, mut tag: Dynamic) -> (String, Dynamic) {
    if name == "minecraft:ocelot" {
        let ty = tag.get("CatType").as_int_or(0);
        if ty > 0 && ty < 4 {
            tag = tag.set("CatType", tag.create_int(ty));
            tag = tag.set(
                "OwnerUUID",
                tag.create_string(tag.get("OwnerUUID").as_string_or("")),
            );
            return ("minecraft:cat".to_string(), tag);
        }
    }
    (name.to_string(), tag)
}
