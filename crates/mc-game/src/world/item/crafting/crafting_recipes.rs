//! `ShapedRecipe` (+ `ShapedRecipePattern`) and `ShapelessRecipe`.

use mc_data::Item;
use mc_dfu::{DataResult, MapCodec, STRING};
use mc_protocol::components::ItemStackTemplate;
use mc_protocol::packets::play::inventory::{RecipeDisplay, ShapedCraftingRecipeDisplay, ShapelessCraftingRecipeDisplay, SlotDisplay};

use super::display::{item_display, stack_display};
use super::ingredient::{Ingredient, PlacementInfo};
use super::inputs::CraftingInput;
use super::recipe::{CommonInfo, CraftingBookInfo};
use crate::world::item::ItemStack;

const MAX_SIZE: usize = 3;

/// `ShapedRecipePattern.Data`: the pattern as written in the recipe file.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedPatternData {
    pub key: Vec<(char, Ingredient)>,
    pub pattern: Vec<String>,
}

impl ShapedPatternData {
    fn pattern_codec() -> mc_dfu::Codec<Vec<String>> {
        STRING.list_of().comap_flat_map(
            |strings| {
                if strings.len() > MAX_SIZE {
                    return DataResult::error_str("Invalid pattern: too many rows, 3 is maximum");
                }
                if strings.is_empty() {
                    return DataResult::error_str("Invalid pattern: empty pattern not allowed");
                }
                let first_length = java_len(&strings[0]);
                for line in &strings {
                    if java_len(line) > MAX_SIZE {
                        return DataResult::error_str("Invalid pattern: too many columns, 3 is maximum");
                    }
                    if first_length != java_len(line) {
                        return DataResult::error_str("Invalid pattern: each row must be the same width");
                    }
                }
                DataResult::success(strings)
            },
            |strings: &Vec<String>| strings.clone(),
        )
    }

    fn symbol_codec() -> mc_dfu::Codec<char> {
        STRING.comap_flat_map(
            |symbol| {
                if java_len(&symbol) != 1 {
                    let message = format!("Invalid key entry: '{symbol}' is an invalid symbol (must be 1 character only).");
                    return DataResult::error(move || message.clone());
                }
                if symbol == " " {
                    return DataResult::error_str("Invalid key entry: ' ' is a reserved symbol.");
                }
                DataResult::success(symbol.chars().next().unwrap_or(' '))
            },
            |c: &char| c.to_string(),
        )
    }

    /// `Data.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<ShapedPatternData> {
        mc_dfu::record!(ShapedPatternData {
            key: Self::symbol_codec().unbounded_map(&Ingredient::codec()).field_of("key"),
            pattern: Self::pattern_codec().field_of("pattern"),
        })
    }
}

/// Java `String.length()` (UTF-16 units).
fn java_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `ShapedRecipePattern`.
#[derive(Clone, Debug)]
pub struct ShapedRecipePattern {
    width: usize,
    height: usize,
    ingredients: Vec<Option<Ingredient>>,
    data: Option<ShapedPatternData>,
    ingredient_count: usize,
    symmetrical: bool,
}

impl ShapedRecipePattern {
    fn new(width: usize, height: usize, ingredients: Vec<Option<Ingredient>>, data: Option<ShapedPatternData>) -> Self {
        let ingredient_count = ingredients.iter().flatten().count();
        let symmetrical = is_symmetrical(width, height, &ingredients);
        ShapedRecipePattern { width, height, ingredients, data, ingredient_count, symmetrical }
    }

    /// `ShapedRecipePattern.of(key, pattern)`.
    pub fn of(key: Vec<(char, Ingredient)>, pattern: Vec<String>) -> Result<ShapedRecipePattern, String> {
        Self::unpack(ShapedPatternData { key, pattern }).into_result()
    }

    /// `unpack(data)`.
    fn unpack(data: ShapedPatternData) -> DataResult<ShapedRecipePattern> {
        let shrunk = shrink(&data.pattern);
        let width = shrunk.first().map_or(0, |l| l.len());
        let height = shrunk.len();
        let mut ingredients = Vec::with_capacity(width * height);
        let mut unused_symbols: Vec<char> = data.key.iter().map(|(c, _)| *c).collect();
        for line in &shrunk {
            for symbol in line {
                let ingredient = if *symbol == ' ' {
                    None
                } else {
                    match data.key.iter().find(|(c, _)| c == symbol) {
                        Some((_, i)) => Some(i.clone()),
                        None => {
                            let message = format!("Pattern references symbol '{symbol}' but it's not defined in the key");
                            return DataResult::error(move || message.clone());
                        }
                    }
                };
                unused_symbols.retain(|c| c != symbol);
                ingredients.push(ingredient);
            }
        }
        if !unused_symbols.is_empty() {
            let listed = unused_symbols.iter().map(char::to_string).collect::<Vec<_>>().join(", ");
            let message = format!("Key defines symbols that aren't used in pattern: [{listed}]");
            return DataResult::error(move || message.clone());
        }
        DataResult::success(ShapedRecipePattern::new(width, height, ingredients, Some(data)))
    }

    /// `ShapedRecipePattern.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<ShapedRecipePattern> {
        ShapedPatternData::map_codec().flat_xmap(
            ShapedRecipePattern::unpack,
            |pattern: &ShapedRecipePattern| match &pattern.data {
                Some(data) => DataResult::success(data.clone()),
                None => DataResult::error_str("Cannot encode unpacked recipe"),
            },
        )
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn ingredients(&self) -> &[Option<Ingredient>] {
        &self.ingredients
    }

    /// `matches(input)`.
    pub fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() != self.ingredient_count {
            return false;
        }
        if input.width() == self.width && input.height() == self.height {
            if !self.symmetrical && self.matches_flipped(input, true) {
                return true;
            }
            if self.matches_flipped(input, false) {
                return true;
            }
        }
        false
    }

    fn matches_flipped(&self, input: &CraftingInput, x_flip: bool) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                let expected = if x_flip { &self.ingredients[self.width - x - 1 + y * self.width] } else { &self.ingredients[x + y * self.width] };
                if !Ingredient::test_optional(expected.as_ref(), input.get_item_at(x, y)) {
                    return false;
                }
            }
        }
        true
    }
}

/// `ShapedRecipePattern.shrink(pattern)`: trims empty rows/columns (the rows are char vectors for indexing).
fn shrink(pattern: &[String]) -> Vec<Vec<char>> {
    let rows: Vec<Vec<char>> = pattern.iter().map(|l| l.chars().collect()).collect();
    let first_non_empty = |line: &[char]| line.iter().take_while(|c| **c == ' ').count() as i32;
    let last_non_empty = |line: &[char]| line.iter().rposition(|c| *c != ' ').map_or(-1, |i| i as i32);
    let (mut left, mut right, mut top, mut bottom) = (i32::MAX, 0i32, 0usize, 0usize);
    for (i, line) in rows.iter().enumerate() {
        left = left.min(first_non_empty(line));
        let last_non_space = last_non_empty(line);
        right = right.max(last_non_space);
        if last_non_space < 0 {
            if top == i {
                top += 1;
            }
            bottom += 1;
        } else {
            bottom = 0;
        }
    }
    if rows.len() == bottom {
        return Vec::new();
    }
    (0..rows.len() - bottom - top).map(|line| rows[line + top][left as usize..=right as usize].to_vec()).collect()
}

/// `Util.isSymmetrical(width, height, list)`.
fn is_symmetrical(width: usize, height: usize, list: &[Option<Ingredient>]) -> bool {
    if width == 1 {
        return true;
    }
    let half_width = width / 2;
    for y in 0..height {
        for x in 0..half_width {
            let opposite_x = width - 1 - x;
            if x != opposite_x && list[x + y * width] != list[opposite_x + y * width] {
                return false;
            }
        }
    }
    true
}

/// `ShapedRecipe`.
#[derive(Clone, Debug)]
pub struct ShapedRecipe {
    pub common: CommonInfo,
    pub book: CraftingBookInfo,
    pub pattern: ShapedRecipePattern,
    pub result: ItemStackTemplate,
}

impl ShapedRecipe {
    pub fn map_codec() -> MapCodec<ShapedRecipe> {
        mc_dfu::record!(ShapedRecipe {
            common: CommonInfo::map_codec(),
            book: CraftingBookInfo::map_codec(),
            pattern: ShapedRecipePattern::map_codec(),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    pub(super) fn for_each_ingredient_mut(&mut self, f: &mut dyn FnMut(&mut Ingredient)) {
        self.pattern.ingredients.iter_mut().flatten().for_each(f);
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        let ingredients: Vec<Option<&Ingredient>> = self.pattern.ingredients.iter().map(Option::as_ref).collect();
        PlacementInfo::create_from_optionals(&ingredients)
    }

    pub fn matches(&self, input: &CraftingInput) -> bool {
        self.pattern.matches(input)
    }

    pub fn width(&self) -> usize {
        self.pattern.width
    }

    pub fn height(&self) -> usize {
        self.pattern.height
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        vec![RecipeDisplay::CraftingShaped(ShapedCraftingRecipeDisplay {
            width: self.pattern.width as i32,
            height: self.pattern.height as i32,
            ingredients: self.pattern.ingredients.iter().map(|i| i.as_ref().map_or(SlotDisplay::Empty, Ingredient::display)).collect(),
            result: stack_display(&self.result),
            crafting_station: item_display(Item::CRAFTING_TABLE),
        })]
    }
}

/// `ShapelessRecipe`.
#[derive(Clone, Debug)]
pub struct ShapelessRecipe {
    pub common: CommonInfo,
    pub book: CraftingBookInfo,
    pub result: ItemStackTemplate,
    pub ingredients: Vec<Ingredient>,
}

impl ShapelessRecipe {
    pub fn map_codec() -> MapCodec<ShapelessRecipe> {
        mc_dfu::record!(ShapelessRecipe {
            common: CommonInfo::map_codec(),
            book: CraftingBookInfo::map_codec(),
            result: ItemStackTemplate::codec().field_of("result"),
            ingredients: Ingredient::codec().list_of_range(1, 9).field_of("ingredients"),
        })
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        PlacementInfo::create(&self.ingredients)
    }

    pub fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() != self.ingredients.len() {
            return false;
        }
        if input.items.len() == 1 && self.ingredients.len() == 1 {
            return self.ingredients[0].test(input.get_item_at(0, 0));
        }
        if self.ingredients.iter().any(Ingredient::is_empty) {
            return false;
        }
        input.stacked_contents().clone().can_craft_ingredients(&self.ingredients, 1, None)
    }

    pub fn assemble(&self, _input: &CraftingInput) -> ItemStack {
        self.result.create()
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        vec![RecipeDisplay::CraftingShapeless(ShapelessCraftingRecipeDisplay {
            ingredients: self.ingredients.iter().map(Ingredient::display).collect(),
            result: stack_display(&self.result),
            crafting_station: item_display(Item::CRAFTING_TABLE),
        })]
    }
}
