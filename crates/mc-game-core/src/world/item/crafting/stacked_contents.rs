//! `StackedContents` (the bipartite item/ingredient matcher behind shapeless matching and recipe placement) and
//! `StackedItemContents`.
//!
//! Vanilla keys the amounts by identity hash (`Reference2IntOpenHashMap`), whose iteration order is unspecified; here they
//! keep insertion order. The matching result (`tryPick` success, the biggest craftable stack) does not depend on it.

use std::hash::Hash;

use indexmap::IndexMap;
use mc_data::Item;

use super::ingredient::Ingredient;
use super::recipe::Recipe;
use crate::world::entity::player::inventory::is_usable_for_crafting;
use crate::world::item::ItemStack;

/// `StackedContents.IngredientInfo`.
pub trait IngredientInfo<T> {
    fn accepts_item(&self, item: &T) -> bool;
}

/// `StackedContents<T>`.
#[derive(Clone, Debug)]
pub struct StackedContents<T: Copy + Eq + Hash> {
    pub amounts: IndexMap<T, i32>,
}

impl<T: Copy + Eq + Hash> Default for StackedContents<T> {
    fn default() -> Self {
        StackedContents {
            amounts: IndexMap::new(),
        }
    }
}

impl<T: Copy + Eq + Hash> StackedContents<T> {
    fn amount(&self, item: &T) -> i32 {
        self.amounts.get(item).copied().unwrap_or(0)
    }

    fn has_at_least(&self, item: &T, count: i32) -> bool {
        self.amount(item) >= count
    }

    /// `take`: `amounts.addTo(item, -amount)`; taking more than present is an `IllegalStateException`.
    fn take(&mut self, item: T, amount: i32) {
        let entry = self.amounts.entry(item).or_insert(0);
        let previous = *entry;
        *entry = entry.wrapping_sub(amount);
        assert!(
            previous >= amount,
            "Took {amount} items, but only had {previous}"
        );
    }

    fn put(&mut self, item: T, count: i32) {
        let entry = self.amounts.entry(item).or_insert(0);
        *entry = entry.wrapping_add(count);
    }

    /// `tryPick(ingredients, amount, output)`.
    pub fn try_pick<I: IngredientInfo<T>>(
        &mut self,
        ingredients: &[I],
        amount: i32,
        output: Option<&mut dyn FnMut(T)>,
    ) -> bool {
        RecipePicker::new(self, ingredients).try_pick(amount, output)
    }

    /// `tryPickAll(ingredients, maxSize, output)`.
    pub fn try_pick_all<I: IngredientInfo<T>>(
        &mut self,
        ingredients: &[I],
        max_size: i32,
        output: Option<&mut dyn FnMut(T)>,
    ) -> i32 {
        RecipePicker::new(self, ingredients).try_pick_all(max_size, output)
    }

    pub fn clear(&mut self) {
        self.amounts.clear();
    }

    /// `account(item, count)`.
    pub fn account(&mut self, item: T, count: i32) {
        self.put(item, count);
    }

    fn unique_available_ingredient_items<I: IngredientInfo<T>>(&self, ingredients: &[I]) -> Vec<T> {
        self.amounts
            .iter()
            .filter(|(item, count)| **count > 0 && ingredients.iter().any(|i| i.accepts_item(item)))
            .map(|(item, _)| *item)
            .collect()
    }

    /// `getResultUpperBound`.
    pub fn get_result_upper_bound<I: IngredientInfo<T>>(&self, ingredients: &[I]) -> i32 {
        let mut min = i32::MAX;
        'ingredients: for ingredient in ingredients {
            let mut max = 0;
            for (item, &item_count) in &self.amounts {
                if item_count > max {
                    if ingredient.accepts_item(item) {
                        max = item_count;
                    }
                    if max >= min {
                        continue 'ingredients;
                    }
                }
            }
            min = max;
            if max == 0 {
                break;
            }
        }
        min
    }
}

/// `StackedContents.RecipePicker` with the `BitSet` laid out as vanilla does (visited ingredients, visited items,
/// satisfied, connections, residual assignments).
struct RecipePicker<'a, T: Copy + Eq + Hash, I> {
    contents: &'a mut StackedContents<T>,
    ingredients: &'a [I],
    ingredient_count: usize,
    items: Vec<T>,
    item_count: usize,
    visited_ingredient: Vec<bool>,
    visited_item: Vec<bool>,
    satisfied: Vec<bool>,
    connection: Vec<bool>,
    residual: Vec<bool>,
    path: Vec<usize>,
}

impl<'a, T: Copy + Eq + Hash, I: IngredientInfo<T>> RecipePicker<'a, T, I> {
    fn new(contents: &'a mut StackedContents<T>, ingredients: &'a [I]) -> Self {
        let items = contents.unique_available_ingredient_items(ingredients);
        let (ingredient_count, item_count) = (ingredients.len(), items.len());
        let mut picker = RecipePicker {
            contents,
            ingredients,
            ingredient_count,
            visited_ingredient: vec![false; ingredient_count],
            visited_item: vec![false; item_count],
            satisfied: vec![false; ingredient_count],
            connection: vec![false; ingredient_count * item_count],
            residual: vec![false; ingredient_count * item_count],
            items,
            item_count,
            path: Vec::new(),
        };
        for ingredient in 0..ingredient_count {
            for item in 0..item_count {
                if picker.ingredients[ingredient].accepts_item(&picker.items[item]) {
                    picker.connection[item * ingredient_count + ingredient] = true;
                }
            }
        }
        picker
    }

    fn idx(&self, item: usize, ingredient: usize) -> usize {
        item * self.ingredient_count + ingredient
    }

    fn clear_all_visited(&mut self) {
        self.visited_ingredient.fill(false);
        self.visited_item.fill(false);
    }

    fn try_pick(&mut self, capacity: i32, mut output: Option<&mut dyn FnMut(T)>) -> bool {
        if capacity <= 0 {
            return true;
        }
        let mut satisfied_ingredient_count = 0;
        loop {
            let Some(path) = self.try_assigning_new_item(capacity) else {
                let is_valid_assignment = satisfied_ingredient_count == self.ingredient_count;
                let has_output = is_valid_assignment && output.is_some();
                self.clear_all_visited();
                self.satisfied.fill(false);
                for ingredient in 0..self.ingredient_count {
                    for item in 0..self.item_count {
                        let i = self.idx(item, ingredient);
                        if self.residual[i] {
                            self.residual[i] = false;
                            let picked = self.items[item];
                            self.contents.put(picked, capacity);
                            if has_output {
                                if let Some(out) = output.as_mut() {
                                    out(picked);
                                }
                            }
                            break;
                        }
                    }
                }
                return is_valid_assignment;
            };
            let assigned_item = path[0];
            self.contents.take(self.items[assigned_item], capacity);
            let satisfied_ingredient = path.len() - 1;
            self.satisfied[path[satisfied_ingredient]] = true;
            satisfied_ingredient_count += 1;
            for i in 0..path.len() - 1 {
                if i & 1 == 0 {
                    let (item, ingredient) = (path[i], path[i + 1]);
                    let r = self.idx(item, ingredient);
                    self.residual[r] = true;
                } else {
                    let (item, ingredient) = (path[i + 1], path[i]);
                    let r = self.idx(item, ingredient);
                    self.residual[r] = false;
                }
            }
        }
    }

    fn try_assigning_new_item(&mut self, capacity: i32) -> Option<Vec<usize>> {
        self.clear_all_visited();
        for item in 0..self.item_count {
            if self.contents.has_at_least(&self.items[item], capacity) {
                if let Some(path) = self.find_new_item_assignment_path(item) {
                    return Some(path);
                }
            }
        }
        None
    }

    fn find_new_item_assignment_path(&mut self, starting_item: usize) -> Option<Vec<usize>> {
        self.path.clear();
        self.visited_item[starting_item] = true;
        self.path.push(starting_item);
        while !self.path.is_empty() {
            let path_length = self.path.len();
            if (path_length - 1) & 1 == 0 {
                let item_to_assign = self.path[path_length - 1];
                for ingredient in 0..self.ingredient_count {
                    let i = self.idx(item_to_assign, ingredient);
                    if !self.visited_ingredient[ingredient]
                        && self.connection[i]
                        && !self.residual[i]
                    {
                        self.visited_ingredient[ingredient] = true;
                        self.path.push(ingredient);
                        break;
                    }
                }
            } else {
                let last_assigned_ingredient = self.path[path_length - 1];
                if !self.satisfied[last_assigned_ingredient] {
                    return Some(self.path.clone());
                }
                for item in 0..self.item_count {
                    if !self.visited_item[item]
                        && self.residual[self.idx(item, last_assigned_ingredient)]
                    {
                        self.visited_item[item] = true;
                        self.path.push(item);
                        break;
                    }
                }
            }
            if self.path.len() == path_length {
                self.path.pop();
            }
        }
        None
    }

    fn try_pick_all(&mut self, max_size: i32, mut output: Option<&mut dyn FnMut(T)>) -> i32 {
        let mut min = 0i32;
        let mut max = max_size
            .min(self.contents.get_result_upper_bound(self.ingredients))
            .wrapping_add(1);
        loop {
            let mid = (min.wrapping_add(max)) / 2;
            if self.try_pick(mid, None) {
                if max - min <= 1 {
                    if mid > 0 {
                        self.try_pick(mid, output.take());
                    }
                    return mid;
                }
                min = mid;
            } else {
                max = mid;
            }
        }
    }
}

impl IngredientInfo<Item> for Ingredient {
    fn accepts_item(&self, item: &Item) -> bool {
        self.accepts(*item)
    }
}

impl IngredientInfo<Item> for &Ingredient {
    fn accepts_item(&self, item: &Item) -> bool {
        self.accepts(*item)
    }
}

/// `StackedItemContents`.
#[derive(Clone, Debug, Default)]
pub struct StackedItemContents {
    raw: StackedContents<Item>,
}

impl StackedItemContents {
    pub fn new() -> Self {
        Self::default()
    }

    /// `accountSimpleStack`.
    pub fn account_simple_stack(&mut self, stack: &ItemStack) {
        if is_usable_for_crafting(stack) {
            self.account_stack(stack);
        }
    }

    pub fn account_stack(&mut self, stack: &ItemStack) {
        self.account_stack_max(stack, stack.get_max_stack_size());
    }

    /// `accountStack(stack, maxCount)`.
    pub fn account_stack_max(&mut self, stack: &ItemStack, max_count: i32) {
        if !stack.is_empty() {
            self.raw.account(stack.item(), max_count.min(stack.count()));
        }
    }

    /// `canCraft(recipe, output)`.
    pub fn can_craft(&mut self, recipe: &Recipe, output: Option<&mut dyn FnMut(Item)>) -> bool {
        self.can_craft_amount(recipe, 1, output)
    }

    pub fn can_craft_amount(
        &mut self,
        recipe: &Recipe,
        amount: i32,
        output: Option<&mut dyn FnMut(Item)>,
    ) -> bool {
        let placement = recipe.placement_info();
        !placement.is_impossible_to_place()
            && self.can_craft_ingredients(placement.ingredients(), amount, output)
    }

    pub fn can_craft_ingredients<I: IngredientInfo<Item>>(
        &mut self,
        contents: &[I],
        amount: i32,
        output: Option<&mut dyn FnMut(Item)>,
    ) -> bool {
        self.raw.try_pick(contents, amount, output)
    }

    /// `getBiggestCraftableStack(recipe, maxSize, output)`.
    pub fn get_biggest_craftable_stack(
        &mut self,
        recipe: &Recipe,
        max_size: i32,
        output: Option<&mut dyn FnMut(Item)>,
    ) -> i32 {
        self.raw
            .try_pick_all(recipe.placement_info().ingredients(), max_size, output)
    }

    pub fn clear(&mut self) {
        self.raw.clear();
    }
}
