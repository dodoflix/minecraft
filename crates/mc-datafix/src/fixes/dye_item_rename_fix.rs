//! `DyeItemRenameFix`: the public `RENAMED_IDS` table.

use crate::fixes::tables::table;
use std::sync::LazyLock;

static RENAMED_IDS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    table("dye_item_rename_fix")
        .field("RENAMED_IDS")
        .string_pairs()
});

/// `DyeItemRenameFix.RENAMED_IDS`.
pub fn renamed_ids() -> &'static [(&'static str, &'static str)] {
    &RENAMED_IDS
}
