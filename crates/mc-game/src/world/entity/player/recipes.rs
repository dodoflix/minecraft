//! `ServerPlayer`'s recipe side: `awardRecipes`, `resetRecipes`, `awardRecipesByKey`, `triggerRecipeCrafted` and the
//! recipe book packets handlers.

use mc_core::ident::Identifier;
use mc_protocol::packets::play::inventory::{ServerboundRecipeBookChangeSettingsPacket, ServerboundRecipeBookSeenRecipePacket};

use super::server_player::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::item::crafting::{RecipeHolder, ServerRecipeBook};

impl ServerPlayer {
    /// `awardRecipes(recipes)` (`ServerRecipeBook.addRecipes`): learns the recipes that are new and not special, sends one
    /// `recipe_book_add` for all their displays and returns how many displays were added.
    pub fn award_recipes(&mut self, level: &mut ServerLevel, recipes: &[RecipeHolder]) -> i32 {
        let manager = level.recipe_access();
        let mut entries = Vec::new();
        for recipe in recipes {
            if self.recipe_book.try_add_recipe(recipe, &manager, &mut entries) {
                // CriteriaTriggers::RECIPE_UNLOCKED.trigger(level, self, &recipe.id)
            }
        }
        let count = entries.len() as i32;
        if let Some(packet) = ServerRecipeBook::add_packet(entries) {
            self.send(&packet);
        }
        count
    }

    /// `awardRecipesByKey(recipeIds)`: ids the manager does not know are skipped.
    pub fn award_recipes_by_key(&mut self, level: &mut ServerLevel, recipe_ids: &[Identifier]) {
        let manager = level.recipe_access();
        let recipes: Vec<RecipeHolder> = recipe_ids.iter().filter_map(|id| manager.by_key(id).cloned()).collect();
        self.award_recipes(level, &recipes);
    }

    /// `resetRecipes(recipes)` (`ServerRecipeBook.removeRecipes`): forgets the known ones and sends `recipe_book_remove`.
    pub fn reset_recipes(&mut self, level: &ServerLevel, recipes: &[RecipeHolder]) -> i32 {
        let (packet, count) = self.recipe_book.remove_recipes(recipes, &level.recipe_access());
        if let Some(packet) = packet {
            self.send(&packet);
        }
        count
    }

    /// `triggerRecipeCrafted(recipe, itemStacks)`.
    pub fn trigger_recipe_crafted(&mut self, _level: &mut ServerLevel, _recipe: &RecipeHolder, _items: &[ItemStack]) {
        // CriteriaTriggers::RECIPE_CRAFTED.trigger(level, self, &recipe.id, items)
    }

    /// `handleRecipeBookSeenRecipePacket`.
    pub fn handle_recipe_book_seen_recipe(&mut self, level: &ServerLevel, packet: &ServerboundRecipeBookSeenRecipePacket) {
        if let Some(entry) = level.recipe_access().get_recipe_from_display(packet.recipe) {
            self.recipe_book.remove_highlight(&entry.parent.id);
        }
    }

    /// `handleRecipeBookChangeSettingsPacket`.
    pub fn handle_recipe_book_change_settings(&mut self, packet: &ServerboundRecipeBookChangeSettingsPacket) {
        self.recipe_book.set_book_setting(packet.book_type, packet.is_open, packet.is_filtering);
    }
}
