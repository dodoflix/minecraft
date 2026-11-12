//! `RecipeCraftingHolder`: a container that remembers the recipe whose result it holds (the crafting result slot).

use super::MenuPlayer;
use crate::world::item::ItemStack;
use crate::world::item::crafting::RecipeHolder;

pub trait RecipeCraftingHolder {
    /// `setRecipeUsed(recipe)`.
    fn set_recipe_used(&mut self, recipe: Option<RecipeHolder>);

    /// `getRecipeUsed()`.
    fn get_recipe_used(&self) -> Option<&RecipeHolder>;

    /// `awardUsedRecipes(player, itemStacks)`: the `recipe_crafted` trigger, and the unlock of a non-special recipe.
    fn award_used_recipes(&mut self, player: &mut dyn MenuPlayer, items: &[ItemStack]) {
        let Some(recipe) = self.get_recipe_used().cloned() else {
            return;
        };
        player.trigger_recipe_crafted(&recipe, items);
        if !recipe.value.is_special() {
            player.award_recipes(std::slice::from_ref(&recipe));
            self.set_recipe_used(None);
        }
    }

    /// `setRecipeUsed(player, recipe)`: refuses a recipe the player has not unlocked when `limited_crafting` is on.
    fn set_recipe_used_by(&mut self, player: &dyn MenuPlayer, recipe: &RecipeHolder) -> bool {
        let limited = player
            .level_ref()
            .is_some_and(|level| level.limited_crafting());
        if !recipe.value.is_special() && limited && !player.recipe_book_contains(&recipe.id) {
            false
        } else {
            self.set_recipe_used(Some(recipe.clone()));
            true
        }
    }
}
