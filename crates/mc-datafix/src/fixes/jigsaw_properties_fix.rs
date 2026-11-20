//! `JigsawPropertiesFix`: renames the jigsaw block entity's `attachement_type`/`target_pool` to `name`/`target`/`pool`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new JigsawPropertiesFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "JigsawPropertiesFix",
        schema,
        changes_type,
        "JigsawPropertiesFix",
        &References::BLOCK_ENTITY,
        "minecraft:jigsaw",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(tag: Dynamic) -> Dynamic {
    let old_name = tag.get("attachement_type").as_string_or("minecraft:empty");
    let old_pool = tag.get("target_pool").as_string_or("minecraft:empty");
    tag.set("name", tag.create_string(old_name.clone()))
        .set("target", tag.create_string(old_name))
        .remove("attachement_type")
        .set("pool", tag.create_string(old_pool))
        .remove("target_pool")
}
