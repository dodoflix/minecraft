//! `net.minecraft.world.item.crafting`: recipes, ingredients, recipe inputs, the recipe manager, recipe displays.
//!
//! Recipes are decoded from the `recipe` registry ([`decoder`]) and served by [`RecipeManager`] (a `ReloadListener`
//! (`crate::server::reloadable_resources::ReloadListener`), reachable as `level.recipe_access()`).

mod brewing_recipe;
mod crafting_recipes;
mod custom_recipes;
pub mod decoder;
pub mod display;
mod ingredient;
mod inputs;
mod place_recipe;
mod play_handlers;
mod recipe;
mod recipe_book;
mod recipe_manager;
mod single_item_recipes;
mod smithing_recipes;
pub mod stacked_contents;
mod util;

pub use brewing_recipe::{BrewingRecipe, PotionIngredient};
pub use crafting_recipes::{ShapedPatternData, ShapedRecipe, ShapedRecipePattern, ShapelessRecipe};
pub use custom_recipes::{
    BannerDuplicateRecipe, BookCloningRecipe, CustomRecipe, DecoratedPotRecipe, DyeRecipe,
    FireworkRocketRecipe, FireworkStarFadeRecipe, FireworkStarRecipe, ImbueRecipe,
    MapExtendingRecipe, RepairItemRecipe, ShieldDecorationRecipe, TransmuteRecipe, TransmuteResult,
    apply_dyes, set_map_scale_lookup,
};
pub use ingredient::{Ingredient, ItemHolderSet, PlacementInfo};
pub use inputs::{
    BrewingInput, CraftingInput, PositionedCraftingInput, RecipeInput, SingleRecipeInput,
    SmithingRecipeInput,
};
pub use place_recipe::{
    PlaceRecipeMenu, PostPlaceAction, RecipeBookMenu, place_recipe, place_recipe_helper,
    place_recipe_sized,
};
pub use recipe::{
    CommonInfo, CookingBookCategory, CookingBookInfo, CraftingBookCategory, CraftingBookInfo,
    Recipe, RecipeKind, SmithingIngredients, default_crafting_reminder,
};
pub use recipe_book::{RecipeBookSettings, ServerRecipeBook};
pub use recipe_manager::{
    RecipeCache, RecipeCheck, RecipeHolder, RecipeManager, RecipePropertySet, SelectableRecipe,
    ServerDisplayInfo, SingleInputEntry, SingleInputSet, update_recipes_packet,
};
pub use single_item_recipes::{CookingKind, CookingRecipe, StonecutterRecipe};
pub use smithing_recipes::{SmithingTransformRecipe, SmithingTrimRecipe, apply_trim};
pub use stacked_contents::{IngredientInfo, StackedContents, StackedItemContents};
pub use util::{create_with_original_components, template_apply};
