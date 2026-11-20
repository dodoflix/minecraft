//! `AdvancementsFix`: renames the advancement ids of the 1.12 recipes to their flattened names.

use crate::datafix::DataFix;
use crate::fixes::advancements_rename_fix;
use crate::fixes::tables::{OrderedMap, table};
use crate::schema::Schema;
use std::sync::LazyLock;

static RENAMES: LazyLock<OrderedMap<String, String>> =
    LazyLock::new(|| table("advancements_fix").field("RENAMES").string_map());

/// `new AdvancementsFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    advancements_rename_fix::new(
        "AdvancementsFix",
        output_schema,
        changes_type,
        "AdvancementsFix",
        |name| RENAMES.get(name).map_or(name, String::as_str).to_string(),
    )
}
