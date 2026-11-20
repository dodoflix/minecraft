//! `RecipesRenameningFix`: the public `RECIPES` table.

use crate::fixes::tables::table;
use std::sync::LazyLock;

static RECIPES: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    table("recipes_renamening_fix")
        .field("RECIPES")
        .string_pairs()
});

/// `RecipesRenameningFix.RECIPES`.
pub fn recipes() -> &'static [(&'static str, &'static str)] {
    &RECIPES
}
