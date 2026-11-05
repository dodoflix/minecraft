//! `BrewingRecipe` and `PotionIngredient` (brewing mixes are data in 26.3: `recipe/brewing/*.json`).

use mc_data::Item;
use mc_dfu::MapCodec;
use mc_protocol::components::values::{DataComponentPredicate, PotionsPredicate};
use mc_protocol::components::ItemStackTemplate;

use super::ingredient::Ingredient;
use super::inputs::BrewingInput;
use crate::world::item::ItemStack;

/// `PotionIngredient`: an item ingredient with an optional `potion_contents` predicate.
#[derive(Clone, Debug)]
pub struct PotionIngredient {
    pub ingredient: Ingredient,
    pub potions: Option<PotionsPredicate>,
}

impl PotionIngredient {
    /// `PotionIngredient.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<PotionIngredient> {
        mc_dfu::record!(PotionIngredient {
            ingredient: Ingredient::codec().field_of("item"),
            potions: PotionsPredicate::codec().optional_field_of("potion_contents"),
        })
    }

    /// `PotionIngredient.of(item)`.
    pub fn of(item: Item) -> PotionIngredient {
        PotionIngredient { ingredient: Ingredient::of(item), potions: None }
    }

    /// `isPotionInput(stack, recipeAccess)`.
    pub fn is_potion_input(stack: &ItemStack, recipes: &super::RecipeManager) -> bool {
        recipes.property_set(super::RecipePropertySet::BREWING_INPUTS).test(stack)
            || recipes.item_is_in_tag(stack, &mc_core::ident::Identifier::parse("minecraft:brewing_potion_inputs").expect("valid tag"))
    }

    /// `test(stack)`.
    pub fn test(&self, stack: &ItemStack) -> bool {
        if !self.ingredient.test(stack) {
            return false;
        }
        match &self.potions {
            None => true,
            Some(potions) => DataComponentPredicate::PotionContents(potions.clone()).matches(stack),
        }
    }
}

/// `BrewingRecipe`.
#[derive(Clone, Debug)]
pub struct BrewingRecipe {
    pub input: PotionIngredient,
    pub reagent: PotionIngredient,
    pub output: ItemStackTemplate,
}

impl BrewingRecipe {
    pub fn map_codec() -> MapCodec<BrewingRecipe> {
        mc_dfu::record!(BrewingRecipe {
            input: PotionIngredient::map_codec().codec().field_of("input"),
            reagent: PotionIngredient::map_codec().codec().field_of("reagent"),
            output: ItemStackTemplate::codec().field_of("output"),
        })
    }

    /// `getInput()`.
    pub fn input(&self) -> &PotionIngredient {
        &self.input
    }

    /// `getReagent()`.
    pub fn reagent(&self) -> &PotionIngredient {
        &self.reagent
    }

    /// `getOutput()`.
    pub fn output(&self) -> &ItemStackTemplate {
        &self.output
    }

    /// `matches(brewingInput)`.
    pub fn matches(&self, input: &BrewingInput) -> bool {
        self.input.test(&input.input) && self.reagent.test(&input.reagent)
    }
}
