//! `BlockEntityKeepPacked`: sets `keepPacked` on `DUMMY` block entities.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BlockEntityKeepPacked(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "BlockEntityKeepPacked",
        schema,
        changes_type,
        "BlockEntityKeepPacked",
        &References::BLOCK_ENTITY,
        "DUMMY",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `fixTag(tag)`.
fn fix_tag(tag: Dynamic) -> Dynamic {
    tag.set("keepPacked", tag.create_boolean(true))
}
