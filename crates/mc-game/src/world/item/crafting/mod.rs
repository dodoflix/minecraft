//! `net.minecraft.world.item.crafting`: recipes, ingredients, recipe inputs, the recipe manager, recipe displays.
//!
//! Recipes are decoded from the `recipe` registry ([`decoder`]) and served by [`RecipeManager`] (a [`ReloadListener`]
//! (`crate::server::reloadable_resources::ReloadListener`), reachable as `level.recipe_access()`).

mod brewing_recipe;
mod crafting_recipes;
pub mod decoder;
pub mod display;
mod ingredient;
mod inputs;
mod recipe;
mod recipe_manager;
mod single_item_recipes;
mod smithing_recipes;
pub mod stacked_contents;
mod util;

pub use brewing_recipe::{BrewingRecipe, PotionIngredient};
pub use crafting_recipes::{ShapedPatternData, ShapedRecipe, ShapedRecipePattern, ShapelessRecipe};
pub use ingredient::{Ingredient, ItemHolderSet, PlacementInfo};
pub use inputs::{BrewingInput, CraftingInput, PositionedCraftingInput, RecipeInput, SingleRecipeInput, SmithingRecipeInput};
pub use recipe::{
    CommonInfo, CookingBookCategory, CookingBookInfo, CraftingBookCategory, CraftingBookInfo, Recipe, RecipeKind, SmithingIngredients, default_crafting_reminder,
};
pub use recipe_manager::{
    update_recipes_packet,
    RecipeCache, RecipeCheck, RecipeHolder, RecipeManager, RecipePropertySet, SelectableRecipe, ServerDisplayInfo, SingleInputEntry, SingleInputSet,
};
pub use single_item_recipes::{CookingKind, CookingRecipe, StonecutterRecipe};
pub use smithing_recipes::{SmithingTransformRecipe, SmithingTrimRecipe, apply_trim};
pub use stacked_contents::{IngredientInfo, StackedContents, StackedItemContents};
pub use util::{create_with_original_components, template_apply};
