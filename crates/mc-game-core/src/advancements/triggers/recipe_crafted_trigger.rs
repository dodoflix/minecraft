//! `RecipeCraftedTrigger`: `recipe_crafted`, `crafter_recipe_crafted`.

use super::prelude::*;
use mc_core::ident::Identifier;
use super::recipe_unlocked_trigger::{RecipeSet, recipe_set_codec};

/// `RecipeCraftedTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct RecipeCraftedTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub recipes: RecipeSet,
    pub ingredients: Vec<ItemPredicate>,
}

impl TriggerInstance for RecipeCraftedTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<RecipeCraftedTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(RecipeCraftedTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                recipes: recipe_set_codec().field_of("recipes"),
                ingredients: ItemPredicate::codec().list_of().optional_field_of_default("ingredients", Vec::new()),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for RecipeCraftedTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(RecipeCraftedTriggerInstance);

crate::register_triggers!(RecipeCraftedTriggerInstance {
    RECIPE_CRAFTED = "recipe_crafted",
    CRAFTER_RECIPE_CRAFTED = "crafter_recipe_crafted",
});

impl RecipeCraftedTriggerInstance {
    fn matches(&self, registries: &crate::registry::RegistryAccess, id: &Identifier, used_ingredients: &[ItemStack]) -> bool {
        if !self.recipes.contains(registries, id) {
            return false;
        }
        let mut remaining: Vec<&ItemStack> = used_ingredients.iter().collect();
        for predicate in &self.ingredients {
            match remaining.iter().position(|s| predicate.test(s)) {
                Some(i) => {
                    remaining.remove(i);
                }
                None => return false,
            }
        }
        true
    }
}

impl Trigger<RecipeCraftedTriggerInstance> {
    /// `trigger(player, id, usedIngredients)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, id: &Identifier, used_ingredients: &[ItemStack]) {
        let registries = level.registries.clone();
        self.trigger_with(level, player, |t| t.matches(&registries, id, used_ingredients));
    }
}
