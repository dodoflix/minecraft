//! The normal crafting recipes that are not shaped/shapeless (`TransmuteRecipe`, `DyeRecipe`, `ImbueRecipe`) and every
//! `CustomRecipe` (decorated pot, book cloning, map extending, firework rocket/star/fade, banner duplicate, shield
//! decoration, repair item).

use std::sync::OnceLock;

use mc_core::ident::Identifier;
use mc_data::{DataComponentType, Item, RecipeSerializer};
use mc_dfu::{BOOL, Codec, DataResult, MapCodec};
use mc_protocol::components::values::*;
use mc_protocol::components::{DataComponentPatch, DyeColor, ItemStackTemplate};
use mc_protocol::packets::play::inventory::{RecipeDisplay, ShapedCraftingRecipeDisplay, ShapelessCraftingRecipeDisplay, SlotDisplay};

use super::crafting_recipes::ShapedRecipePattern;
use super::display::{item_display, stack_display};
use super::ingredient::{Ingredient, PlacementInfo};
use super::inputs::CraftingInput;
use super::recipe::{CommonInfo, CraftingBookInfo, default_crafting_reminder};
use super::util::{banner_color, create_with_original_components, dye_firework_color, dye_texture_diffuse_color, patch_of, template_apply};
use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

/// `MinMaxBounds.Ints` (the bounds type of the recipes' material counts / book generations).
pub use mc_protocol::components::values::Ints;

/// `MinMaxBounds.validateContainedInRange(allowed)` for `Ints`: the bounds must be bounded on both sides inside `[lo, hi]`.
fn contained_in_range(lo: i32, hi: i32) -> impl Fn(Ints) -> DataResult<Ints> + Clone + Send + Sync + 'static {
    move |target: Ints| {
        let (min, max) = (target.bounds.min, target.bounds.max);
        if min.is_some_and(|m| m >= lo) && max.is_some_and(|m| m <= hi) {
            return DataResult::success(target);
        }
        let side = |v: Option<i32>, open: &str, close: &str, infinity: &str| match v {
            Some(v) => format!("{open}{v}{close}"),
            None => infinity.to_owned(),
        };
        let message = format!(
            "Range must be within [{lo}..{hi}], but was {}..{}",
            side(min, "[", "", "(-\u{221e}"),
            side(max, "", "]", "+\u{221e})")
        );
        DataResult::error(move || message.clone())
    }
}

// ---- TransmuteResult -------------------------------------------------------------------------------------------------

/// `TransmuteResult`: the result item (or the input item when absent), count and components.
#[derive(Clone, Debug, PartialEq)]
pub struct TransmuteResult {
    pub item: Option<Item>,
    pub count: i32,
    pub components: DataComponentPatch,
}

impl TransmuteResult {
    /// `TransmuteResult.CODEC`: the map form or a bare item id.
    pub fn codec() -> Codec<TransmuteResult> {
        let item = mc_protocol::components::holder::registry_by_name_codec::<Item>().validate(|item| {
            if item == Item::AIR { DataResult::error_str("Item must not be minecraft:air") } else { DataResult::success(item) }
        });
        let map = mc_dfu::record!(TransmuteResult {
            item: item.optional_field_of("id"),
            count: mc_dfu::extra::extra_int_range(1, 99).optional_field_of_default("count", 1),
            components: DataComponentPatch::codec().optional_field_of_default("components", DataComponentPatch::empty()),
        })
        .codec();
        map.with_alternative_conv(&item, |item| TransmuteResult { item: Some(item), count: 1, components: DataComponentPatch::empty() })
    }

    /// `resolve(inputItem, count)`.
    fn resolve(&self, input_item: Item, count: i32) -> ItemStackTemplate {
        ItemStackTemplate { item: self.item.unwrap_or(input_item), count, components: self.components.clone() }
    }
}

// ---- TransmuteRecipe -------------------------------------------------------------------------------------------------

/// `TransmuteRecipe`.
#[derive(Clone, Debug)]
pub struct TransmuteRecipe {
    pub common: CommonInfo,
    pub book: CraftingBookInfo,
    pub input: Ingredient,
    pub material: Ingredient,
    pub material_count: Ints,
    pub result: TransmuteResult,
    pub add_material_count_to_result: bool,
}

impl TransmuteRecipe {
    pub fn map_codec() -> MapCodec<TransmuteRecipe> {
        let ingredient = Ingredient::codec();
        let bounds = Ints::codec().validate(contained_in_range(1, 8));
        mc_dfu::record!(TransmuteRecipe {
            common: CommonInfo::map_codec(),
            book: CraftingBookInfo::map_codec(),
            input: ingredient.field_of("input"),
            material: ingredient.field_of("material"),
            material_count: bounds.optional_field_of_default("material_count", Ints::exactly(1)),
            result: TransmuteResult::codec().field_of("result"),
            add_material_count_to_result: BOOL.optional_field_of_default("add_material_count_to_result", false),
        })
    }

    fn min_material_count(&self) -> i32 {
        self.material_count.bounds.min.unwrap_or(1)
    }

    fn max_material_count(&self) -> i32 {
        self.material_count.bounds.max.unwrap_or(8)
    }

    fn compute_result_size(&self, material_count: i32) -> i32 {
        if self.add_material_count_to_result { material_count.wrapping_add(self.result.count) } else { self.result.count }
    }

    fn compute_result(&self, input_ingredient: &ItemStack, material_count: i32) -> ItemStack {
        create_with_original_components(&self.result.resolve(input_ingredient.item(), self.result.count), input_ingredient, material_count)
    }

    pub fn matches(&self, input: &CraftingInput) -> bool {
        let (min, max) = (self.min_material_count(), self.max_material_count());
        let ingredient_count = input.ingredient_count() as i32;
        if !(ingredient_count > min && ingredient_count <= max + 1) {
            return false;
        }
        let mut found_input: Option<&ItemStack> = None;
        let mut material_count = 0;
        for stack in input.items() {
            if stack.is_empty() {
                continue;
            }
            if self.input.test(stack) {
                if found_input.is_some() {
                    return false;
                }
                found_input = Some(stack);
            } else {
                if !self.material.test(stack) {
                    return false;
                }
                material_count += 1;
                if material_count > max {
                    return false;
                }
            }
        }
        match found_input {
            Some(found) if !found.is_empty() && self.material_count.matches(material_count) => {
                let result_count = self.compute_result_size(material_count);
                if result_count != 1 {
                    return true;
                }
                let result = self.compute_result(found, 0);
                !result.is_empty() && !ItemStack::is_same_item_same_components(found, &result)
            }
            _ => false,
        }
    }

    pub fn assemble(&self, input: &CraftingInput) -> ItemStack {
        if self.add_material_count_to_result {
            let mut material_count = 0;
            let mut input_ingredient = ItemStack::empty();
            for stack in input.items().iter().filter(|s| !s.is_empty()) {
                if self.input.test(stack) {
                    input_ingredient = stack.clone();
                } else if self.material.test(stack) {
                    material_count += 1;
                }
            }
            self.compute_result(&input_ingredient, material_count)
        } else {
            for stack in input.items() {
                if !stack.is_empty() && self.input.test(stack) {
                    return self.compute_result(stack, 0);
                }
            }
            ItemStack::empty()
        }
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        let mut ingredients = vec![self.input.clone()];
        ingredients.extend(std::iter::repeat_n(self.material.clone(), self.max_material_count().max(0) as usize));
        PlacementInfo::create(&ingredients)
    }

    fn stack_display(&self, item: Item, result_count: i32) -> SlotDisplay {
        SlotDisplay::ItemStack { stack: self.result.resolve(item, result_count) }
    }

    fn result_display(&self, result_count: i32) -> SlotDisplay {
        match self.result.item {
            Some(item) => self.stack_display(item, result_count),
            None => SlotDisplay::Composite { contents: self.input.items().iter().map(|i| self.stack_display(*i, result_count)).collect() },
        }
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        let mut displays = Vec::new();
        let mut ingredient_slots = vec![self.input.display()];
        let material_display = self.material.display();
        for material_count in self.min_material_count()..=self.max_material_count() {
            ingredient_slots.push(material_display.clone());
            displays.push(RecipeDisplay::CraftingShapeless(ShapelessCraftingRecipeDisplay {
                ingredients: ingredient_slots.clone(),
                result: self.result_display(self.compute_result_size(material_count)),
                crafting_station: item_display(Item::CRAFTING_TABLE),
            }));
        }
        displays
    }
}

// ---- DyeRecipe -------------------------------------------------------------------------------------------------------

/// `DyeRecipe`.
#[derive(Clone, Debug)]
pub struct DyeRecipe {
    pub common: CommonInfo,
    pub book: CraftingBookInfo,
    pub target: Ingredient,
    pub dye: Ingredient,
    pub result: ItemStackTemplate,
}

/// `DyedItemColor.applyDyes(currentDye, dyes)`.
pub fn apply_dyes(current: Option<&DyedItemColor>, dyes: &[DyeColor]) -> DyedItemColor {
    let (mut red_total, mut green_total, mut blue_total, mut intensity_total, mut color_count) = (0i32, 0i32, 0i32, 0i32, 0i32);
    let mut add = |color: i32| {
        let (red, green, blue) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
        intensity_total += red.max(green.max(blue));
        red_total += red;
        green_total += green;
        blue_total += blue;
        color_count += 1;
    };
    if let Some(current) = current {
        add(current.rgb);
    }
    for dye in dyes {
        add(dye_texture_diffuse_color(*dye));
    }
    let (mut red, mut green, mut blue) = (red_total / color_count, green_total / color_count, blue_total / color_count);
    let average_intensity = intensity_total as f32 / color_count as f32;
    let result_intensity = red.max(green.max(blue)) as f32;
    red = (red as f32 * average_intensity / result_intensity) as i32;
    green = (green as f32 * average_intensity / result_intensity) as i32;
    blue = (blue as f32 * average_intensity / result_intensity) as i32;
    // `ARGB.color(0, red, green, blue)`
    DyedItemColor { rgb: (red << 16) | (green << 8) | blue }
}

impl DyeRecipe {
    pub fn map_codec() -> MapCodec<DyeRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(DyeRecipe {
            common: CommonInfo::map_codec(),
            book: CraftingBookInfo::map_codec(),
            target: ingredient.field_of("target"),
            dye: ingredient.field_of("dye"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    pub fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() < 2 {
            return false;
        }
        let (mut has_target, mut has_dyes) = (false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.target.test(stack) {
                if has_target {
                    return false;
                }
                has_target = true;
            } else {
                if !self.dye.test(stack) || !stack.has(DataComponentType::DYE) {
                    return false;
                }
                has_dyes = true;
            }
        }
        has_dyes && has_target
    }

    pub fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut dyes = Vec::new();
        let mut target_stack = ItemStack::empty();
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.target.test(stack) {
                if !target_stack.is_empty() {
                    return ItemStack::empty();
                }
                target_stack = stack.clone();
            } else {
                if !self.dye.test(stack) {
                    return ItemStack::empty();
                }
                dyes.push(stack.get(&DYE).copied().unwrap_or(DyeColor::White));
            }
        }
        if target_stack.is_empty() || dyes.is_empty() {
            return ItemStack::empty();
        }
        let new_color = apply_dyes(target_stack.get(&DYED_COLOR), &dyes);
        let mut result = create_with_original_components(&self.result, &target_stack, 0);
        result.set(DYED_COLOR, new_color);
        result
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        PlacementInfo::create(&[self.target.clone(), self.dye.clone()])
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        let dyes_with_dye_component = SlotDisplay::OnlyWithComponent { source: Box::new(self.dye.display()), component: DataComponentType::DYE };
        let target_display = self.target.display();
        vec![RecipeDisplay::CraftingShapeless(ShapelessCraftingRecipeDisplay {
            ingredients: vec![target_display.clone(), dyes_with_dye_component.clone()],
            result: SlotDisplay::Dyed { dye: Box::new(dyes_with_dye_component), target: Box::new(target_display) },
            crafting_station: item_display(Item::CRAFTING_TABLE),
        })]
    }
}

// ---- ImbueRecipe -----------------------------------------------------------------------------------------------------

/// `ImbueRecipe`.
#[derive(Clone, Debug)]
pub struct ImbueRecipe {
    pub common: CommonInfo,
    pub book: CraftingBookInfo,
    pub source: Ingredient,
    pub material: Ingredient,
    pub result: ItemStackTemplate,
}

impl ImbueRecipe {
    pub fn map_codec() -> MapCodec<ImbueRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(ImbueRecipe {
            common: CommonInfo::map_codec(),
            book: CraftingBookInfo::map_codec(),
            source: ingredient.field_of("source"),
            material: ingredient.field_of("material"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    pub fn matches(&self, input: &CraftingInput) -> bool {
        if !(input.width() == 3 && input.height() == 3 && input.ingredient_count() == 9) {
            return false;
        }
        for y in 0..3 {
            for x in 0..3 {
                let stack = input.get_item_at(x, y);
                if stack.is_empty() {
                    return false;
                }
                let ingredient = if x == 1 && y == 1 { &self.source } else { &self.material };
                if !ingredient.test(stack) {
                    return false;
                }
            }
        }
        true
    }

    pub fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let source = input.get_item_at(1, 1);
        let mut result = self.result.create();
        match source.get(&POTION_CONTENTS) {
            Some(contents) => {
                result.set(POTION_CONTENTS, contents.clone());
            }
            None => {
                result.remove(DataComponentType::POTION_CONTENTS);
            }
        }
        result
    }

    pub(super) fn create_placement_info(&self) -> PlacementInfo {
        let m = &self.material;
        PlacementInfo::create(&[m.clone(), m.clone(), m.clone(), m.clone(), self.source.clone(), m.clone(), m.clone(), m.clone(), m.clone()])
    }

    pub(super) fn display(&self) -> Vec<RecipeDisplay> {
        let material = self.material.display();
        let source = SlotDisplay::WithAnyPotion { display: Box::new(self.source.display()) };
        vec![RecipeDisplay::CraftingShaped(ShapedCraftingRecipeDisplay {
            width: 3,
            height: 3,
            ingredients: vec![
                material.clone(),
                material.clone(),
                material.clone(),
                material.clone(),
                source,
                material.clone(),
                material.clone(),
                material.clone(),
                material,
            ],
            result: SlotDisplay::WithAnyPotion { display: Box::new(stack_display(&self.result)) },
            crafting_station: item_display(Item::CRAFTING_TABLE),
        })]
    }
}

// ---- CustomRecipe subclasses -----------------------------------------------------------------------------------------

/// `DecoratedPotRecipe`.
#[derive(Clone, Debug)]
pub struct DecoratedPotRecipe {
    pub back_pattern: Ingredient,
    pub left_pattern: Ingredient,
    pub right_pattern: Ingredient,
    pub front_pattern: Ingredient,
    pub result: ItemStackTemplate,
}

impl DecoratedPotRecipe {
    fn map_codec() -> MapCodec<DecoratedPotRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(DecoratedPotRecipe {
            back_pattern: ingredient.field_of("back"),
            left_pattern: ingredient.field_of("left"),
            right_pattern: ingredient.field_of("right"),
            front_pattern: ingredient.field_of("front"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        input.width() == 3
            && input.height() == 3
            && input.ingredient_count() == 4
            && self.back_pattern.test(input.get_item_at(1, 0))
            && self.left_pattern.test(input.get_item_at(0, 1))
            && self.right_pattern.test(input.get_item_at(2, 1))
            && self.front_pattern.test(input.get_item_at(1, 2))
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let to_optional = |stack: &ItemStack| {
            (!stack.is_empty()).then(|| ItemStackTemplate { item: stack.item(), count: 1, components: stack.get_components_patch() })
        };
        let decorations = PotDecorations {
            back: to_optional(input.get_item_at(1, 0)),
            left: to_optional(input.get_item_at(0, 1)),
            right: to_optional(input.get_item_at(2, 1)),
            front: to_optional(input.get_item_at(1, 2)),
        };
        template_apply(&self.result, self.result.count, &patch_of(self.result.item, |s| {
            s.set(POT_DECORATIONS, decorations);
        }))
    }
}

/// `BookCloningRecipe`.
#[derive(Clone, Debug)]
pub struct BookCloningRecipe {
    pub source: Ingredient,
    pub material: Ingredient,
    pub allowed_generations: Ints,
    pub result: ItemStackTemplate,
}

impl BookCloningRecipe {
    fn map_codec() -> MapCodec<BookCloningRecipe> {
        let ingredient = Ingredient::codec();
        let generations = Ints::codec().validate(contained_in_range(0, 2));
        mc_dfu::record!(BookCloningRecipe {
            source: ingredient.field_of("source"),
            material: ingredient.field_of("material"),
            allowed_generations: generations.optional_field_of_default("allowed_generations", Ints::between(0, 1)),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() < 2 {
            return false;
        }
        let (mut has_material, mut has_source) = (false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.source.test(stack) {
                match stack.get(&WRITTEN_BOOK_CONTENT) {
                    Some(content) if self.allowed_generations.matches(content.generation) => {}
                    _ => return false,
                }
                if has_source {
                    return false;
                }
                has_source = true;
            } else {
                if !self.material.test(stack) {
                    return false;
                }
                has_material = true;
            }
        }
        has_source && has_material
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut count = 0;
        let mut source = ItemStack::empty();
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.source.test(stack) && stack.has(DataComponentType::WRITTEN_BOOK_CONTENT) {
                if !source.is_empty() {
                    return ItemStack::empty();
                }
                source = stack.clone();
            } else {
                if !self.material.test(stack) {
                    return ItemStack::empty();
                }
                count += 1;
            }
        }
        let Some(content) = source.get(&WRITTEN_BOOK_CONTENT) else { return ItemStack::empty() };
        // `craftCopy()`
        let copied = WrittenBookContent { generation: content.generation.wrapping_add(1), ..content.clone() };
        let mut result = create_with_original_components(&self.result, &source, count - 1);
        result.set(WRITTEN_BOOK_CONTENT, copied);
        result
    }

    fn remaining_items(&self, input: &CraftingInput) -> Vec<ItemStack> {
        let mut result = vec![ItemStack::empty(); input.items().len()];
        for (slot, stack) in input.items().iter().enumerate() {
            if let Some(remainder) = stack.item().data().crafting_remainder {
                result[slot] = ItemStack::new(remainder);
            } else if stack.has(DataComponentType::WRITTEN_BOOK_CONTENT) {
                result[slot] = stack.copy_with_count(1);
                break;
            }
        }
        result
    }
}

/// `MapItem.getSavedData(map, level).scale`: the map slice installs it (`None` when the map data does not exist).
pub type MapScaleLookup = fn(&ServerLevel, &ItemStack) -> Option<i32>;

static MAP_SCALE_LOOKUP: OnceLock<MapScaleLookup> = OnceLock::new();

/// Installed by the maps slice (`MapItemSavedData.scale`); without it no map can be extended.
pub fn set_map_scale_lookup(lookup: MapScaleLookup) {
    let _ = MAP_SCALE_LOOKUP.set(lookup);
}

/// `MapExtendingRecipe`.
#[derive(Clone, Debug)]
pub struct MapExtendingRecipe {
    pattern: ShapedRecipePattern,
    pub map: Ingredient,
    pub material: Ingredient,
    pub result: TransmuteResult,
}

impl MapExtendingRecipe {
    fn new(map: Ingredient, material: Ingredient, result: TransmuteResult) -> MapExtendingRecipe {
        let pattern = ShapedRecipePattern::of(
            vec![('#', material.clone()), ('x', map.clone())],
            vec!["###".to_owned(), "#x#".to_owned(), "###".to_owned()],
        )
        .expect("fixed map extending pattern");
        MapExtendingRecipe { pattern, map, material, result }
    }

    fn map_codec() -> MapCodec<MapExtendingRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(MapExtendingFields {
            map: ingredient.field_of("map"),
            material: ingredient.field_of("material"),
            result: TransmuteResult::codec().field_of("result"),
        })
        .xmap(|f: MapExtendingFields| MapExtendingRecipe::new(f.map, f.material, f.result), |r: &MapExtendingRecipe| MapExtendingFields {
            map: r.map.clone(),
            material: r.material.clone(),
            result: r.result.clone(),
        })
    }

    fn find_filled_map(input: &CraftingInput) -> ItemStack {
        input.items().iter().find(|s| s.has(DataComponentType::MAP_ID)).cloned().unwrap_or_else(ItemStack::empty)
    }

    fn matches(&self, input: &CraftingInput, level: &ServerLevel) -> bool {
        if !self.pattern.matches(input) {
            return false;
        }
        let map = Self::find_filled_map(input);
        if map.is_empty() {
            return false;
        }
        MAP_SCALE_LOOKUP.get().and_then(|lookup| lookup(level, &map)).is_some_and(|scale| scale < 4)
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let source_map = Self::find_filled_map(input);
        let mut map = create_with_original_components(&self.result.resolve(source_map.item(), self.result.count), &source_map, 0);
        map.set(MAP_POST_PROCESSING, MapPostProcessing::Scale);
        map
    }
}

#[derive(Clone)]
struct MapExtendingFields {
    map: Ingredient,
    material: Ingredient,
    result: TransmuteResult,
}

/// `FireworkRocketRecipe`.
#[derive(Clone, Debug)]
pub struct FireworkRocketRecipe {
    pub shell: Ingredient,
    pub fuel: Ingredient,
    pub star: Ingredient,
    pub result: ItemStackTemplate,
}

impl FireworkRocketRecipe {
    fn map_codec() -> MapCodec<FireworkRocketRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(FireworkRocketRecipe {
            shell: ingredient.field_of("shell"),
            fuel: ingredient.field_of("fuel"),
            star: ingredient.field_of("star"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() < 2 {
            return false;
        }
        let (mut has_shell, mut fuel_count) = (false, 0);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.shell.test(stack) {
                if has_shell {
                    return false;
                }
                has_shell = true;
            } else if self.fuel.test(stack) {
                fuel_count += 1;
                if fuel_count > 3 {
                    return false;
                }
            } else if !self.star.test(stack) {
                return false;
            }
        }
        has_shell && fuel_count >= 1
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut explosions = Vec::new();
        let mut fuel_count = 0;
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.fuel.test(stack) {
                fuel_count += 1;
            } else if self.star.test(stack) {
                if let Some(explosion) = stack.get(&FIREWORK_EXPLOSION) {
                    explosions.push(explosion.clone());
                }
            }
        }
        template_apply(&self.result, self.result.count, &patch_of(self.result.item, |s| {
            s.set(FIREWORKS, Fireworks { flight_duration: fuel_count, explosions });
        }))
    }
}

/// `FireworkStarRecipe`.
#[derive(Clone, Debug)]
pub struct FireworkStarRecipe {
    pub shapes: Vec<(FireworkShape, Ingredient)>,
    pub trail: Ingredient,
    pub twinkle: Ingredient,
    pub fuel: Ingredient,
    pub dye: Ingredient,
    pub result: ItemStackTemplate,
}

impl FireworkStarRecipe {
    fn map_codec() -> MapCodec<FireworkStarRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(FireworkStarRecipe {
            shapes: FireworkShape::codec().unbounded_map(&ingredient).field_of("shapes"),
            trail: ingredient.field_of("trail"),
            twinkle: ingredient.field_of("twinkle"),
            fuel: ingredient.field_of("fuel"),
            dye: ingredient.field_of("dye"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn find_shape(&self, stack: &ItemStack) -> Option<FireworkShape> {
        self.shapes.iter().find(|(_, i)| i.test(stack)).map(|(s, _)| *s)
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() < 2 {
            return false;
        }
        let (mut has_fuel, mut has_dye, mut has_shape, mut has_trail, mut has_twinkle) = (false, false, false, false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.twinkle.test(stack) {
                if has_twinkle {
                    return false;
                }
                has_twinkle = true;
            } else if self.trail.test(stack) {
                if has_trail {
                    return false;
                }
                has_trail = true;
            } else if self.fuel.test(stack) {
                if has_fuel {
                    return false;
                }
                has_fuel = true;
            } else if self.dye.test(stack) && stack.has(DataComponentType::DYE) {
                has_dye = true;
            } else {
                if self.find_shape(stack).is_none() || has_shape {
                    return false;
                }
                has_shape = true;
            }
        }
        has_fuel && has_dye
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut shape = FireworkShape::SmallBall;
        let (mut has_twinkle, mut has_trail) = (false, false);
        let mut colors = Vec::new();
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if let Some(maybe_shape) = self.find_shape(stack) {
                shape = maybe_shape;
            } else if self.twinkle.test(stack) {
                has_twinkle = true;
            } else if self.trail.test(stack) {
                has_trail = true;
            } else if self.dye.test(stack) {
                colors.push(dye_firework_color(stack.get(&DYE).copied().unwrap_or(DyeColor::White)));
            }
        }
        let mut star = self.result.create();
        star.set(FIREWORK_EXPLOSION, FireworkExplosion { shape, colors, fade_colors: Vec::new(), has_trail, has_twinkle });
        star
    }
}

/// `FireworkStarFadeRecipe`.
#[derive(Clone, Debug)]
pub struct FireworkStarFadeRecipe {
    pub target: Ingredient,
    pub dye: Ingredient,
    pub result: ItemStackTemplate,
}

impl FireworkStarFadeRecipe {
    fn map_codec() -> MapCodec<FireworkStarFadeRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(FireworkStarFadeRecipe {
            target: ingredient.field_of("target"),
            dye: ingredient.field_of("dye"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() < 2 {
            return false;
        }
        let (mut has_dye, mut has_target) = (false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.dye.test(stack) && stack.has(DataComponentType::DYE) {
                has_dye = true;
            } else {
                if !self.target.test(stack) || has_target {
                    return false;
                }
                has_target = true;
            }
        }
        has_target && has_dye
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut colors = Vec::new();
        let mut target_stack: Option<&ItemStack> = None;
        for stack in input.items() {
            if self.dye.test(stack) {
                colors.push(dye_firework_color(stack.get(&DYE).copied().unwrap_or(DyeColor::White)));
            } else if self.target.test(stack) {
                target_stack = Some(stack);
            }
        }
        match target_stack {
            Some(target) if !colors.is_empty() => {
                let mut result = create_with_original_components(&self.result, target, 0);
                let mut explosion = result.get(&FIREWORK_EXPLOSION).cloned().unwrap_or_default();
                explosion.fade_colors = colors;
                result.set(FIREWORK_EXPLOSION, explosion);
                result
            }
            _ => ItemStack::empty(),
        }
    }
}

/// `BannerDuplicateRecipe`.
#[derive(Clone, Debug)]
pub struct BannerDuplicateRecipe {
    pub banner: Ingredient,
    pub result: ItemStackTemplate,
}

fn pattern_count(stack: &ItemStack) -> usize {
    stack.get(&BANNER_PATTERNS).map_or(0, |l| l.layers.len())
}

impl BannerDuplicateRecipe {
    fn map_codec() -> MapCodec<BannerDuplicateRecipe> {
        mc_dfu::record!(BannerDuplicateRecipe {
            banner: Ingredient::codec().field_of("banner"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() != 2 {
            return false;
        }
        let mut color: Option<DyeColor> = None;
        let (mut has_target, mut has_source) = (false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            let Some(banner_color) = banner_color(stack.item()).filter(|_| self.banner.test(stack)) else { return false };
            match color {
                None => color = Some(banner_color),
                Some(c) if c != banner_color => return false,
                _ => {}
            }
            let patterns = pattern_count(stack);
            if patterns > 6 {
                return false;
            }
            if patterns > 0 {
                if has_source {
                    return false;
                }
                has_source = true;
            } else {
                if has_target {
                    return false;
                }
                has_target = true;
            }
        }
        has_source && has_target
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            let patterns = pattern_count(stack);
            if patterns > 0 && patterns <= 6 {
                return create_with_original_components(&self.result, stack, 0);
            }
        }
        ItemStack::empty()
    }

    fn remaining_items(&self, input: &CraftingInput) -> Vec<ItemStack> {
        let mut result = vec![ItemStack::empty(); input.items().len()];
        for (slot, stack) in input.items().iter().enumerate() {
            if stack.is_empty() {
                continue;
            }
            if let Some(remainder) = stack.item().data().crafting_remainder {
                result[slot] = ItemStack::new(remainder);
            } else if pattern_count(stack) > 0 {
                result[slot] = stack.copy_with_count(1);
            }
        }
        result
    }
}

/// `ShieldDecorationRecipe`.
#[derive(Clone, Debug)]
pub struct ShieldDecorationRecipe {
    pub banner: Ingredient,
    pub target: Ingredient,
    pub result: ItemStackTemplate,
}

impl ShieldDecorationRecipe {
    fn map_codec() -> MapCodec<ShieldDecorationRecipe> {
        let ingredient = Ingredient::codec();
        mc_dfu::record!(ShieldDecorationRecipe {
            banner: ingredient.field_of("banner"),
            target: ingredient.field_of("target"),
            result: ItemStackTemplate::codec().field_of("result"),
        })
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        if input.ingredient_count() != 2 {
            return false;
        }
        let (mut has_clear_target, mut has_pattern_banner) = (false, false);
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            if self.banner.test(stack) && banner_color(stack.item()).is_some() {
                if has_pattern_banner {
                    return false;
                }
                has_pattern_banner = true;
            } else {
                if !self.target.test(stack) || has_clear_target || pattern_count(stack) > 0 {
                    return false;
                }
                has_clear_target = true;
            }
        }
        has_clear_target && has_pattern_banner
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        let mut patterns: Option<BannerPatternLayers> = None;
        let mut base_color = DyeColor::White;
        let mut target = ItemStack::empty();
        for stack in input.items().iter().filter(|s| !s.is_empty()) {
            match banner_color(stack.item()).filter(|_| self.banner.test(stack)) {
                Some(color) => {
                    patterns = stack.get(&BANNER_PATTERNS).cloned();
                    base_color = color;
                }
                None if self.target.test(stack) => target = stack.clone(),
                None => {}
            }
        }
        let mut result = create_with_original_components(&self.result, &target, 0);
        match patterns {
            Some(p) => {
                result.set(BANNER_PATTERNS, p);
            }
            None => {
                result.remove(DataComponentType::BANNER_PATTERNS);
            }
        }
        result.set(BASE_COLOR, base_color);
        result
    }
}

/// `RepairItemRecipe` (`INSTANCE`); `curses` are the members of the `curse` enchantment tag, bound when the recipe manager is built.
#[derive(Clone, Debug, Default)]
pub struct RepairItemRecipe {
    pub curses: Vec<Identifier>,
}

impl RepairItemRecipe {
    fn get_items_to_combine(input: &CraftingInput) -> Option<(&ItemStack, &ItemStack)> {
        if input.ingredient_count() != 2 {
            return None;
        }
        let mut first: Option<&ItemStack> = None;
        for stack in input.items() {
            if !stack.is_empty() {
                if let Some(first) = first {
                    return Self::can_combine(first, stack).then_some((first, stack));
                }
                first = Some(stack);
            }
        }
        None
    }

    fn can_combine(first: &ItemStack, second: &ItemStack) -> bool {
        second.is(first.item())
            && first.count() == 1
            && second.count() == 1
            && first.has(DataComponentType::MAX_DAMAGE)
            && second.has(DataComponentType::MAX_DAMAGE)
            && first.has(DataComponentType::DAMAGE)
            && second.has(DataComponentType::DAMAGE)
    }

    fn matches(&self, input: &CraftingInput) -> bool {
        Self::get_items_to_combine(input).is_some()
    }

    fn assemble(&self, input: &CraftingInput) -> ItemStack {
        use crate::world::item::enchantment_helper::{get_enchantments_for_crafting, update_enchantments};
        let Some((first, second)) = Self::get_items_to_combine(input) else { return ItemStack::empty() };
        let durability = first.get_max_damage().max(second.get_max_damage());
        let remaining1 = first.get_max_damage() - first.get_damage_value();
        let remaining2 = second.get_max_damage() - second.get_damage_value();
        let remaining = remaining1 + remaining2 + durability * 5 / 100;
        let mut stack = ItemStack::new(first.item());
        stack.set(MAX_DAMAGE, durability);
        stack.set_damage_value((durability - remaining).max(0));
        let first_enchants = get_enchantments_for_crafting(first);
        let second_enchants = get_enchantments_for_crafting(second);
        let curses = &self.curses;
        update_enchantments(&mut stack, |new_enchantments| {
            // `Sets.union(first.keySet(), second.keySet())`: the first set's entries, then the second's others
            let union = first_enchants.entries().iter().map(|(k, _)| k).chain(second_enchants.entries().iter().map(|(k, _)| k).filter(|k| first_enchants.get_level(k) == 0));
            for enchantment in union {
                if enchantment.name().is_some_and(|n| curses.contains(n)) {
                    let level = first_enchants.get_level(enchantment).max(second_enchants.get_level(enchantment));
                    new_enchantments.set(enchantment, level);
                }
            }
        });
        stack
    }
}

/// Every `CustomRecipe` of vanilla.
#[derive(Clone, Debug)]
pub enum CustomRecipe {
    DecoratedPot(DecoratedPotRecipe),
    BookCloning(BookCloningRecipe),
    MapExtending(MapExtendingRecipe),
    FireworkRocket(FireworkRocketRecipe),
    FireworkStar(FireworkStarRecipe),
    FireworkStarFade(FireworkStarFadeRecipe),
    BannerDuplicate(BannerDuplicateRecipe),
    ShieldDecoration(ShieldDecorationRecipe),
    RepairItem(RepairItemRecipe),
}

impl CustomRecipe {
    pub fn serializer(&self) -> RecipeSerializer {
        match self {
            CustomRecipe::DecoratedPot(_) => RecipeSerializer::CRAFTING_DECORATED_POT,
            CustomRecipe::BookCloning(_) => RecipeSerializer::CRAFTING_SPECIAL_BOOKCLONING,
            CustomRecipe::MapExtending(_) => RecipeSerializer::CRAFTING_SPECIAL_MAPEXTENDING,
            CustomRecipe::FireworkRocket(_) => RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_ROCKET,
            CustomRecipe::FireworkStar(_) => RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_STAR,
            CustomRecipe::FireworkStarFade(_) => RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_STAR_FADE,
            CustomRecipe::BannerDuplicate(_) => RecipeSerializer::CRAFTING_SPECIAL_BANNERDUPLICATE,
            CustomRecipe::ShieldDecoration(_) => RecipeSerializer::CRAFTING_SPECIAL_SHIELDDECORATION,
            CustomRecipe::RepairItem(_) => RecipeSerializer::CRAFTING_SPECIAL_REPAIRITEM,
        }
    }

    /// The map codec of the custom recipe registered as `serializer`.
    pub fn map_codec(serializer: RecipeSerializer) -> Option<MapCodec<CustomRecipe>> {
        let codec = if serializer == RecipeSerializer::CRAFTING_DECORATED_POT {
            DecoratedPotRecipe::map_codec().xmap(CustomRecipe::DecoratedPot, |r: &CustomRecipe| match r {
                CustomRecipe::DecoratedPot(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_BOOKCLONING {
            BookCloningRecipe::map_codec().xmap(CustomRecipe::BookCloning, |r: &CustomRecipe| match r {
                CustomRecipe::BookCloning(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_MAPEXTENDING {
            MapExtendingRecipe::map_codec().xmap(CustomRecipe::MapExtending, |r: &CustomRecipe| match r {
                CustomRecipe::MapExtending(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_ROCKET {
            FireworkRocketRecipe::map_codec().xmap(CustomRecipe::FireworkRocket, |r: &CustomRecipe| match r {
                CustomRecipe::FireworkRocket(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_STAR {
            FireworkStarRecipe::map_codec().xmap(CustomRecipe::FireworkStar, |r: &CustomRecipe| match r {
                CustomRecipe::FireworkStar(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_FIREWORK_STAR_FADE {
            FireworkStarFadeRecipe::map_codec().xmap(CustomRecipe::FireworkStarFade, |r: &CustomRecipe| match r {
                CustomRecipe::FireworkStarFade(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_BANNERDUPLICATE {
            BannerDuplicateRecipe::map_codec().xmap(CustomRecipe::BannerDuplicate, |r: &CustomRecipe| match r {
                CustomRecipe::BannerDuplicate(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_SHIELDDECORATION {
            ShieldDecorationRecipe::map_codec().xmap(CustomRecipe::ShieldDecoration, |r: &CustomRecipe| match r {
                CustomRecipe::ShieldDecoration(r) => r.clone(),
                _ => unreachable!("serializer/kind mismatch"),
            })
        } else if serializer == RecipeSerializer::CRAFTING_SPECIAL_REPAIRITEM {
            MapCodec::unit(RepairItemRecipe::default()).xmap(CustomRecipe::RepairItem, |_: &CustomRecipe| RepairItemRecipe::default())
        } else {
            return None;
        };
        Some(codec)
    }

    pub(super) fn for_each_ingredient_mut(&mut self, f: &mut dyn FnMut(&mut Ingredient)) {
        match self {
            CustomRecipe::DecoratedPot(r) => [&mut r.back_pattern, &mut r.left_pattern, &mut r.right_pattern, &mut r.front_pattern].into_iter().for_each(f),
            CustomRecipe::BookCloning(r) => [&mut r.source, &mut r.material].into_iter().for_each(f),
            CustomRecipe::MapExtending(r) => {
                let (mut map, mut material) = (r.map.clone(), r.material.clone());
                f(&mut map);
                f(&mut material);
                *r = MapExtendingRecipe::new(map, material, r.result.clone());
            }
            CustomRecipe::FireworkRocket(r) => [&mut r.shell, &mut r.fuel, &mut r.star].into_iter().for_each(f),
            CustomRecipe::FireworkStar(r) => {
                r.shapes.iter_mut().for_each(|(_, i)| f(i));
                [&mut r.trail, &mut r.twinkle, &mut r.fuel, &mut r.dye].into_iter().for_each(f);
            }
            CustomRecipe::FireworkStarFade(r) => [&mut r.target, &mut r.dye].into_iter().for_each(f),
            CustomRecipe::BannerDuplicate(r) => f(&mut r.banner),
            CustomRecipe::ShieldDecoration(r) => [&mut r.banner, &mut r.target].into_iter().for_each(f),
            CustomRecipe::RepairItem(_) => {}
        }
    }

    /// Binds what is not an ingredient but depends on the loaded registries (the `curse` tag of `RepairItemRecipe`).
    pub(super) fn resolve_registries(&mut self, registries: &RegistryAccess) {
        if let CustomRecipe::RepairItem(r) = self {
            let registry = registries.registry(keys::ENCHANTMENT);
            r.curses = registry
                .and_then(|reg| Some((reg, reg.tag(&Identifier::parse("minecraft:curse").ok()?)?)))
                .map(|(reg, ids)| ids.iter().filter_map(|id| reg.name_of(*id)).collect())
                .unwrap_or_default();
        }
    }

    pub fn matches(&self, input: &CraftingInput, level: &ServerLevel) -> bool {
        match self {
            CustomRecipe::DecoratedPot(r) => r.matches(input),
            CustomRecipe::BookCloning(r) => r.matches(input),
            CustomRecipe::MapExtending(r) => r.matches(input, level),
            CustomRecipe::FireworkRocket(r) => r.matches(input),
            CustomRecipe::FireworkStar(r) => r.matches(input),
            CustomRecipe::FireworkStarFade(r) => r.matches(input),
            CustomRecipe::BannerDuplicate(r) => r.matches(input),
            CustomRecipe::ShieldDecoration(r) => r.matches(input),
            CustomRecipe::RepairItem(r) => r.matches(input),
        }
    }

    pub fn assemble(&self, input: &CraftingInput) -> ItemStack {
        match self {
            CustomRecipe::DecoratedPot(r) => r.assemble(input),
            CustomRecipe::BookCloning(r) => r.assemble(input),
            CustomRecipe::MapExtending(r) => r.assemble(input),
            CustomRecipe::FireworkRocket(r) => r.assemble(input),
            CustomRecipe::FireworkStar(r) => r.assemble(input),
            CustomRecipe::FireworkStarFade(r) => r.assemble(input),
            CustomRecipe::BannerDuplicate(r) => r.assemble(input),
            CustomRecipe::ShieldDecoration(r) => r.assemble(input),
            CustomRecipe::RepairItem(r) => r.assemble(input),
        }
    }

    pub fn get_remaining_items(&self, input: &CraftingInput) -> Vec<ItemStack> {
        match self {
            CustomRecipe::BookCloning(r) => r.remaining_items(input),
            CustomRecipe::BannerDuplicate(r) => r.remaining_items(input),
            _ => default_crafting_reminder(input),
        }
    }
}

