//! `EntityTheRenameningFix`: renames the entity ids of the 1.13 renaming (`minecraft:bred_*` entities lose the prefix).

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::fixes::tables::table;
use crate::schema::Schema;
use std::sync::LazyLock;

static RENAMED_IDS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    table("entity_the_renamening_fix")
        .field("RENAMED_IDS")
        .string_pairs()
});
static RENAMED_BLOCKS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    table("entity_the_renamening_fix")
        .field("RENAMED_BLOCKS")
        .string_pairs()
});
static RENAMED_ITEMS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    table("entity_the_renamening_fix")
        .field("RENAMED_ITEMS")
        .string_pairs()
});

/// `EntityTheRenameningFix.RENAMED_BLOCKS`.
pub fn renamed_blocks() -> &'static [(&'static str, &'static str)] {
    &RENAMED_BLOCKS
}

/// `EntityTheRenameningFix.RENAMED_ITEMS`.
pub fn renamed_items() -> &'static [(&'static str, &'static str)] {
    &RENAMED_ITEMS
}

/// `new EntityTheRenameningFix(schema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityTheRenameningFix",
        "EntityTheRenameningBlock",
        schema,
        changes_type,
        rename,
    )
}

/// `rename(name)`: drops the `minecraft:bred_` prefix, then `RENAMED_IDS.getOrDefault(name, name)`.
fn rename(name: &str) -> String {
    let name = match name.strip_prefix("minecraft:bred_") {
        Some(rest) => format!("minecraft:{rest}"),
        None => name.to_string(),
    };
    RENAMED_IDS
        .iter()
        .find(|(old, _)| *old == name)
        .map_or(name.as_str(), |(_, new)| *new)
        .to_string()
}
