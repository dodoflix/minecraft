//! `RecipesFix`: the public `RECIPES` table.

use crate::fixes::tables::table;
use std::sync::LazyLock;

static RECIPES: LazyLock<Vec<(&'static str, &'static str)>> =
    LazyLock::new(|| table("recipes_fix").field("RECIPES").string_pairs());

/// `RecipesFix.RECIPES`.
pub fn recipes() -> &'static [(&'static str, &'static str)] {
    &RECIPES
}
