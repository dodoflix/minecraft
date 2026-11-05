//! `RecipeInput` and its implementations: `CraftingInput`, `SingleRecipeInput`, `SmithingRecipeInput`, `BrewingInput`.

use super::recipe::{Recipe, RecipeKind};
use super::stacked_contents::StackedItemContents;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

/// `RecipeInput` plus the typed dispatch of `Recipe<I>.matches/assemble` (vanilla's generics): a recipe of another input
/// kind never matches and assembles to the empty stack.
pub trait RecipeInput {
    fn get_item(&self, index: usize) -> &ItemStack;
    fn size(&self) -> usize;
    fn is_empty(&self) -> bool {
        (0..self.size()).all(|i| self.get_item(i).is_empty())
    }
    /// `recipe.matches(this, level)`.
    fn matches(&self, recipe: &Recipe, level: &ServerLevel) -> bool;
    /// `recipe.assemble(this)`.
    fn assemble(&self, recipe: &Recipe) -> ItemStack;
}

/// `CraftingInput` (trimmed to its bounding box by [`CraftingInput::of_positioned`]).
#[derive(Clone, Debug, Default)]
pub struct CraftingInput {
    pub width: usize,
    pub height: usize,
    pub items: Vec<ItemStack>,
    stacked_contents: StackedItemContents,
    ingredient_count: usize,
}

/// `CraftingInput.Positioned`.
#[derive(Clone, Debug, Default)]
pub struct PositionedCraftingInput {
    pub input: CraftingInput,
    pub left: usize,
    pub top: usize,
}

impl PartialEq for CraftingInput {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.ingredient_count == other.ingredient_count
            && self.items.len() == other.items.len()
            && self.items.iter().zip(&other.items).all(|(a, b)| ItemStack::matches(a, b))
    }
}

impl CraftingInput {
    fn new(width: usize, height: usize, items: Vec<ItemStack>) -> CraftingInput {
        let mut stacked_contents = StackedItemContents::new();
        let mut ingredient_count = 0;
        for item in &items {
            if !item.is_empty() {
                ingredient_count += 1;
                stacked_contents.account_stack_max(item, 1);
            }
        }
        CraftingInput { width, height, items, stacked_contents, ingredient_count }
    }

    /// `CraftingInput.of(width, height, items)`.
    pub fn of(width: usize, height: usize, items: &[ItemStack]) -> CraftingInput {
        Self::of_positioned(width, height, items).input
    }

    /// `CraftingInput.ofPositioned(width, height, items)`.
    pub fn of_positioned(width: usize, height: usize, items: &[ItemStack]) -> PositionedCraftingInput {
        if width == 0 || height == 0 {
            return PositionedCraftingInput::default();
        }
        let (mut left, mut right, mut top, mut bottom) = (width - 1, 0usize, height - 1, 0usize);
        for y in 0..height {
            let mut row_empty = true;
            for x in 0..width {
                if !items[x + y * width].is_empty() {
                    left = left.min(x);
                    right = right.max(x);
                    row_empty = false;
                }
            }
            if !row_empty {
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
        if right < left || bottom < top {
            return PositionedCraftingInput::default();
        }
        let (new_width, new_height) = (right - left + 1, bottom - top + 1);
        if new_width == width && new_height == height {
            return PositionedCraftingInput { input: CraftingInput::new(width, height, items.to_vec()), left, top };
        }
        let new_items = (0..new_height).flat_map(|y| (0..new_width).map(move |x| (x, y))).map(|(x, y)| items[x + left + (y + top) * width].clone()).collect();
        PositionedCraftingInput { input: CraftingInput::new(new_width, new_height, new_items), left, top }
    }

    /// `getItem(x, y)`.
    pub fn get_item_at(&self, x: usize, y: usize) -> &ItemStack {
        &self.items[x + y * self.width]
    }

    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// `ingredientCount()`: the non-empty slots.
    pub fn ingredient_count(&self) -> usize {
        self.ingredient_count
    }

    /// `stackedContents()`.
    pub fn stacked_contents(&self) -> &StackedItemContents {
        &self.stacked_contents
    }

    pub fn is_empty(&self) -> bool {
        self.ingredient_count == 0
    }
}

impl RecipeInput for CraftingInput {
    fn get_item(&self, index: usize) -> &ItemStack {
        &self.items[index]
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn is_empty(&self) -> bool {
        self.ingredient_count == 0
    }
    fn matches(&self, recipe: &Recipe, level: &ServerLevel) -> bool {
        recipe.matches_crafting(self, level)
    }
    fn assemble(&self, recipe: &Recipe) -> ItemStack {
        recipe.assemble_crafting(self)
    }
}

/// `SingleRecipeInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct SingleRecipeInput(pub ItemStack);

impl SingleRecipeInput {
    pub fn item(&self) -> &ItemStack {
        &self.0
    }
}

impl RecipeInput for SingleRecipeInput {
    fn get_item(&self, index: usize) -> &ItemStack {
        assert!(index == 0, "No item for index {index}");
        &self.0
    }
    fn size(&self) -> usize {
        1
    }
    fn matches(&self, recipe: &Recipe, _level: &ServerLevel) -> bool {
        match &recipe.kind {
            RecipeKind::Cooking(r) => r.input.test(&self.0),
            RecipeKind::Stonecutting(r) => r.input.test(&self.0),
            _ => false,
        }
    }
    fn assemble(&self, recipe: &Recipe) -> ItemStack {
        match &recipe.kind {
            RecipeKind::Cooking(r) => r.result.create(),
            RecipeKind::Stonecutting(r) => r.result.create(),
            _ => ItemStack::empty(),
        }
    }
}

/// `SmithingRecipeInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct SmithingRecipeInput {
    pub template: ItemStack,
    pub base: ItemStack,
    pub addition: ItemStack,
}

impl RecipeInput for SmithingRecipeInput {
    fn get_item(&self, index: usize) -> &ItemStack {
        match index {
            0 => &self.template,
            1 => &self.base,
            2 => &self.addition,
            _ => panic!("Recipe does not contain slot {index}"),
        }
    }
    fn size(&self) -> usize {
        3
    }
    fn is_empty(&self) -> bool {
        self.template.is_empty() && self.base.is_empty() && self.addition.is_empty()
    }
    fn matches(&self, recipe: &Recipe, _level: &ServerLevel) -> bool {
        recipe.matches_smithing(self)
    }
    fn assemble(&self, recipe: &Recipe) -> ItemStack {
        recipe.assemble_smithing(self)
    }
}

/// `BrewingInput`.
#[derive(Clone, Debug, PartialEq)]
pub struct BrewingInput {
    pub input: ItemStack,
    pub reagent: ItemStack,
}

impl RecipeInput for BrewingInput {
    fn get_item(&self, index: usize) -> &ItemStack {
        match index {
            0 => &self.input,
            1 => &self.reagent,
            _ => panic!("No item for index {index}"),
        }
    }
    fn size(&self) -> usize {
        2
    }
    fn matches(&self, recipe: &Recipe, _level: &ServerLevel) -> bool {
        recipe.matches_brewing(self)
    }
    fn assemble(&self, recipe: &Recipe) -> ItemStack {
        recipe.assemble_brewing(self)
    }
}
