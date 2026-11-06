//! `RecipeHolder`, `RecipeMap`/`RecipeManager`, `RecipePropertySet`, `SelectableRecipe`, `RecipeCache` and the cached
//! `RecipeManager.createCheck`.
//!
//! [`RecipeManager`] is a cheap-to-clone handle (an `Arc`); it is also the [`ReloadListener`] that rebuilds itself from the
//! `recipe` registry on every (re)load: ingredient tags are bound against the reloaded tags and `finalizeRecipeLoading`
//! (property sets, stonecutter list, recipe book displays) runs against the enabled feature flags.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_data::{Item, RecipeType};
use mc_protocol::packets::play::inventory::{RecipeDisplayEntry, RecipeDisplayId};
use mc_world::level_storage::FeatureFlagSet;

use super::display::recipe_display_enabled;
use super::ingredient::Ingredient;
use super::inputs::{CraftingInput, RecipeInput};
use super::recipe::{Recipe, RecipeKind};
use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::server::reloadable_resources::{
    ORDER_RECIPE_MANAGER, RELOAD_LISTENERS, ReloadContext, ReloadListener,
    ReloadListenerRegistration,
};
use crate::world::item::{ItemStack, item_is_enabled};

/// `RecipeHolder<T>`: a recipe with its registry key.
#[derive(Clone, Debug)]
pub struct RecipeHolder {
    pub id: Identifier,
    pub value: Arc<Recipe>,
}

impl PartialEq for RecipeHolder {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RecipeHolder {}

/// `RecipePropertySet`: a set of items (what the client's slot placement checks use).
#[derive(Clone, Debug, Default)]
pub struct RecipePropertySet {
    items: Vec<Item>,
    lookup: HashSet<Item>,
}

impl RecipePropertySet {
    pub const SMITHING_BASE: &'static str = "minecraft:smithing_base";
    pub const SMITHING_TEMPLATE: &'static str = "minecraft:smithing_template";
    pub const SMITHING_ADDITION: &'static str = "minecraft:smithing_addition";
    pub const FURNACE_INPUT: &'static str = "minecraft:furnace_input";
    pub const BLAST_FURNACE_INPUT: &'static str = "minecraft:blast_furnace_input";
    pub const SMOKER_INPUT: &'static str = "minecraft:smoker_input";
    pub const CAMPFIRE_INPUT: &'static str = "minecraft:campfire_input";
    pub const BREWING_INPUTS: &'static str = "minecraft:brewing_input";
    pub const BREWING_REAGENTS: &'static str = "minecraft:brewing_reagent";

    /// `RecipePropertySet.create(ingredients)`.
    pub fn create<'a>(ingredients: impl IntoIterator<Item = &'a Ingredient>) -> RecipePropertySet {
        let mut set = RecipePropertySet::default();
        for item in ingredients
            .into_iter()
            .flat_map(|i| i.items().iter().copied())
        {
            if set.lookup.insert(item) {
                set.items.push(item);
            }
        }
        set
    }

    /// `test(stack)`.
    pub fn test(&self, stack: &ItemStack) -> bool {
        self.lookup.contains(&stack.item())
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

/// `SelectableRecipe<T>`.
#[derive(Clone, Debug)]
pub struct SelectableRecipe {
    pub option_display: mc_protocol::packets::play::inventory::SlotDisplay,
    pub recipe: Option<RecipeHolder>,
}

/// `SelectableRecipe.SingleInputEntry<T>`.
#[derive(Clone, Debug)]
pub struct SingleInputEntry {
    pub input: Ingredient,
    pub recipe: SelectableRecipe,
}

/// `SelectableRecipe.SingleInputSet<StonecutterRecipe>`.
#[derive(Clone, Debug, Default)]
pub struct SingleInputSet {
    pub entries: Vec<SingleInputEntry>,
}

impl SingleInputSet {
    /// `acceptsInput(stack)`.
    pub fn accepts_input(&self, input: &ItemStack) -> bool {
        self.entries.iter().any(|e| e.input.test(input))
    }

    /// `selectByInput(stack)`.
    pub fn select_by_input(&self, input: &ItemStack) -> SingleInputSet {
        SingleInputSet {
            entries: self
                .entries
                .iter()
                .filter(|e| e.input.test(input))
                .cloned()
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn size(&self) -> usize {
        self.entries.len()
    }
}

/// `RecipeManager.ServerDisplayInfo`.
#[derive(Clone, Debug)]
pub struct ServerDisplayInfo {
    pub display: RecipeDisplayEntry,
    pub parent: RecipeHolder,
}

#[derive(Default)]
struct RecipeData {
    recipes: Vec<RecipeHolder>,
    by_key: HashMap<Identifier, usize>,
    by_type: HashMap<RecipeType, Vec<usize>>,
    property_sets: Vec<(Identifier, RecipePropertySet)>,
    stonecutter_recipes: SingleInputSet,
    all_displays: Vec<ServerDisplayInfo>,
    recipe_to_display: HashMap<Identifier, Vec<usize>>,
    learnable_recipes: Vec<RecipeHolder>,
    registries: RegistryAccess,
}

/// `RecipeManager`.
#[derive(Clone, Default)]
pub struct RecipeManager {
    data: Arc<RecipeData>,
}

impl RecipeManager {
    /// An empty manager (no recipes), like a server whose data packs hold none.
    pub fn empty() -> RecipeManager {
        RecipeManager::default()
    }

    /// `new RecipeManager(registries)` + `finalizeRecipeLoading(enabledFlags)`.
    pub fn new(registries: &RegistryAccess, enabled_features: FeatureFlagSet) -> RecipeManager {
        let registry = registries.registry_or_throw(keys::RECIPE);
        let mut recipes = Vec::with_capacity(registry.len());
        for entry in registry.entries() {
            let Some(recipe) = entry
                .value
                .as_ref()
                .and_then(|v| v.downcast_ref::<Recipe>())
            else {
                continue;
            };
            recipes.push(RecipeHolder {
                id: entry.id.clone(),
                value: Arc::new(recipe.clone().resolve(registries)),
            });
        }
        let mut data = RecipeData {
            registries: registries.clone(),
            ..RecipeData::default()
        };
        for (index, holder) in recipes.iter().enumerate() {
            data.by_key.insert(holder.id.clone(), index);
            data.by_type
                .entry(holder.value.recipe_type())
                .or_default()
                .push(index);
        }
        data.learnable_recipes = recipes
            .iter()
            .filter(|r| !r.value.is_special())
            .cloned()
            .collect();
        data.recipes = recipes;
        data.finalize_recipe_loading(enabled_features);
        RecipeManager {
            data: Arc::new(data),
        }
    }

    /// `getRecipeFor(type, input, level, recipeHint)`: the hint if it is of `ty` and still matches, else the first match.
    pub fn get_recipe_for<I: RecipeInput>(
        &self,
        ty: RecipeType,
        input: &I,
        level: &ServerLevel,
        hint: Option<&Identifier>,
    ) -> Option<RecipeHolder> {
        if let Some(hinted) = hint.and_then(|id| self.by_key_typed(ty, id))
            && input.matches(&hinted.value, level)
        {
            return Some(hinted.clone());
        }
        self.recipes_for(ty, input, level).next().cloned()
    }

    /// `RecipeMap.getRecipesFor(type, input, level)`.
    pub fn recipes_for<'a, I: RecipeInput>(
        &'a self,
        ty: RecipeType,
        input: &'a I,
        level: &'a ServerLevel,
    ) -> impl Iterator<Item = &'a RecipeHolder> {
        let empty = input.is_empty();
        self.by_type(ty)
            .filter(move |r| !empty && input.matches(&r.value, level))
    }

    /// `byType(type)`: every recipe of `ty` in registry order.
    pub fn by_type(&self, ty: RecipeType) -> impl Iterator<Item = &RecipeHolder> {
        self.data
            .by_type
            .get(&ty)
            .into_iter()
            .flatten()
            .map(|i| &self.data.recipes[*i])
    }

    /// `byKey(recipeId)`.
    pub fn by_key(&self, id: &Identifier) -> Option<&RecipeHolder> {
        self.data.by_key.get(id).map(|i| &self.data.recipes[*i])
    }

    fn by_key_typed(&self, ty: RecipeType, id: &Identifier) -> Option<&RecipeHolder> {
        self.by_key(id).filter(|r| r.value.recipe_type() == ty)
    }

    /// `getRecipes()`: every recipe in registry order.
    pub fn recipes(&self) -> &[RecipeHolder] {
        &self.data.recipes
    }

    /// `getLearnableRecipes()`: the non-special recipes.
    pub fn learnable_recipes(&self) -> &[RecipeHolder] {
        &self.data.learnable_recipes
    }

    /// `propertySet(id)` (`RecipePropertySet.EMPTY` for an unknown key).
    pub fn property_set(&self, id: &str) -> &RecipePropertySet {
        static EMPTY: std::sync::LazyLock<RecipePropertySet> =
            std::sync::LazyLock::new(RecipePropertySet::default);
        self.data
            .property_sets
            .iter()
            .find(|(k, _)| k.to_string() == id)
            .map(|(_, s)| s)
            .unwrap_or(&EMPTY)
    }

    /// `getSynchronizedItemProperties()`.
    pub fn synchronized_item_properties(&self) -> &[(Identifier, RecipePropertySet)] {
        &self.data.property_sets
    }

    /// `stonecutterRecipes()` / `getSynchronizedStonecutterRecipes()`.
    pub fn stonecutter_recipes(&self) -> &SingleInputSet {
        &self.data.stonecutter_recipes
    }

    /// `getRecipeFromDisplay(id)`.
    pub fn get_recipe_from_display(&self, id: RecipeDisplayId) -> Option<&ServerDisplayInfo> {
        usize::try_from(id.index)
            .ok()
            .and_then(|i| self.data.all_displays.get(i))
    }

    /// `listDisplaysForRecipe(id, output)`.
    pub fn list_displays_for_recipe(
        &self,
        id: &Identifier,
        mut output: impl FnMut(&RecipeDisplayEntry),
    ) {
        for index in self.data.recipe_to_display.get(id).into_iter().flatten() {
            output(&self.data.all_displays[*index].display);
        }
    }

    /// The registries the recipes were resolved against (item tags).
    pub fn registries(&self) -> &RegistryAccess {
        &self.data.registries
    }

    /// `ItemStack.is(ItemTags.X)` against the tags this manager was built with.
    pub fn item_is_in_tag(&self, stack: &ItemStack, tag: &Identifier) -> bool {
        self.data
            .registries
            .is_in_tag(keys::ITEM, &item_name(stack.item()), tag)
    }

    /// `RecipeManager.createCheck(type)`.
    pub fn create_check(ty: RecipeType) -> RecipeCheck {
        RecipeCheck {
            ty,
            last_recipe: None,
        }
    }
}

fn item_name(item: Item) -> Identifier {
    Identifier::parse(
        mc_data::registry::by_name("minecraft:item")
            .and_then(|r| r.name_of(item.id()))
            .unwrap_or("minecraft:air"),
    )
    .expect("valid item name")
}

impl RecipeData {
    fn finalize_recipe_loading(&mut self, enabled: FeatureFlagSet) {
        let mut collectors: Vec<(&'static str, Vec<&Ingredient>)> =
            PROPERTY_SETS.iter().map(|k| (*k, Vec::new())).collect();
        let mut stonecutter = Vec::new();
        for holder in &self.recipes {
            let recipe = &*holder.value;
            if !recipe.is_special() && recipe.placement_info().is_impossible_to_place() {
                log::warn!(
                    "Recipe {} can't be placed due to empty ingredients and will be ignored",
                    holder.id
                );
                continue;
            }
            for (key, ingredients) in &mut collectors {
                collect_property_ingredients(key, recipe, ingredients);
            }
            if let Some(s) = recipe.as_stonecutter()
                && is_ingredient_enabled(enabled, s.input())
                && crate::world::item::crafting::display::slot_display_enabled(
                    &s.result_display(),
                    enabled,
                )
            {
                stonecutter.push(SingleInputEntry {
                    input: s.input().clone(),
                    recipe: SelectableRecipe {
                        option_display: s.result_display(),
                        recipe: Some(holder.clone()),
                    },
                });
            }
        }
        self.property_sets = collectors
            .into_iter()
            .map(|(key, ingredients)| {
                let enabled_ingredients: Vec<&Ingredient> = ingredients
                    .into_iter()
                    .filter(|i| is_ingredient_enabled(enabled, i))
                    .collect();
                (
                    Identifier::parse(key).expect("valid property set key"),
                    RecipePropertySet::create(enabled_ingredients),
                )
            })
            .collect();
        self.stonecutter_recipes = SingleInputSet {
            entries: stonecutter,
        };
        self.all_displays = unpack_recipe_info(&self.recipes, enabled);
        for (index, info) in self.all_displays.iter().enumerate() {
            self.recipe_to_display
                .entry(info.parent.id.clone())
                .or_default()
                .push(index);
        }
    }
}

/// The keys of `RecipeManager.RECIPE_PROPERTY_SETS` (in declaration order of the `Map.of`).
const PROPERTY_SETS: [&str; 9] = [
    RecipePropertySet::SMITHING_ADDITION,
    RecipePropertySet::SMITHING_BASE,
    RecipePropertySet::SMITHING_TEMPLATE,
    RecipePropertySet::FURNACE_INPUT,
    RecipePropertySet::BLAST_FURNACE_INPUT,
    RecipePropertySet::SMOKER_INPUT,
    RecipePropertySet::CAMPFIRE_INPUT,
    RecipePropertySet::BREWING_INPUTS,
    RecipePropertySet::BREWING_REAGENTS,
];

/// The `IngredientExtractor` of `key` applied to `recipe`.
fn collect_property_ingredients<'a>(key: &str, recipe: &'a Recipe, out: &mut Vec<&'a Ingredient>) {
    let smithing = recipe.as_smithing();
    let for_single_input = |ty: RecipeType| match &recipe.kind {
        RecipeKind::Cooking(r) if recipe.recipe_type() == ty => Some(&r.input),
        _ => None,
    };
    let picked = match key {
        RecipePropertySet::SMITHING_ADDITION => smithing.and_then(|s| s.addition),
        RecipePropertySet::SMITHING_BASE => smithing.map(|s| s.base),
        RecipePropertySet::SMITHING_TEMPLATE => smithing.and_then(|s| s.template),
        RecipePropertySet::FURNACE_INPUT => for_single_input(RecipeType::SMELTING),
        RecipePropertySet::BLAST_FURNACE_INPUT => for_single_input(RecipeType::BLASTING),
        RecipePropertySet::SMOKER_INPUT => for_single_input(RecipeType::SMOKING),
        RecipePropertySet::CAMPFIRE_INPUT => for_single_input(RecipeType::CAMPFIRE_COOKING),
        RecipePropertySet::BREWING_INPUTS => recipe.as_brewing().map(|b| &b.input.ingredient),
        RecipePropertySet::BREWING_REAGENTS => recipe.as_brewing().map(|b| &b.reagent.ingredient),
        _ => None,
    };
    out.extend(picked);
}

/// `isIngredientEnabled(enabledFlags, ingredient)`.
fn is_ingredient_enabled(enabled: FeatureFlagSet, ingredient: &Ingredient) -> bool {
    ingredient
        .items()
        .iter()
        .all(|i| item_is_enabled(*i, enabled))
}

/// `unpackRecipeInfo(recipes, enabledFeatures)`.
fn unpack_recipe_info(recipes: &[RecipeHolder], enabled: FeatureFlagSet) -> Vec<ServerDisplayInfo> {
    let mut result: Vec<ServerDisplayInfo> = Vec::new();
    let mut recipe_groups: HashMap<&str, i32> = HashMap::new();
    for holder in recipes {
        let recipe = &*holder.value;
        let group_id = if recipe.group().is_empty() {
            None
        } else {
            let next = recipe_groups.len() as i32;
            Some(*recipe_groups.entry(recipe.group()).or_insert(next))
        };
        let placement_check = if recipe.is_special() {
            None
        } else {
            Some(
                recipe
                    .placement_info()
                    .ingredients()
                    .iter()
                    .map(|i| mc_protocol::packets::play::inventory::Ingredient {
                        values: i.wire_set(),
                    })
                    .collect(),
            )
        };
        for display in recipe.display() {
            if recipe_display_enabled(&display, enabled) {
                let id = RecipeDisplayId {
                    index: result.len() as i32,
                };
                let entry = RecipeDisplayEntry {
                    id,
                    display,
                    group: group_id,
                    category: recipe.recipe_book_category(),
                    crafting_requirements: placement_check.clone(),
                };
                result.push(ServerDisplayInfo {
                    display: entry,
                    parent: holder.clone(),
                });
            }
        }
    }
    result
}

/// `RecipeManager.CachedCheck`: remembers the last matching recipe and tries it first.
#[derive(Clone, Debug)]
pub struct RecipeCheck {
    ty: RecipeType,
    last_recipe: Option<Identifier>,
}

impl RecipeCheck {
    pub fn new(ty: RecipeType) -> RecipeCheck {
        RecipeManager::create_check(ty)
    }

    /// `getRecipeFor(input, level)`.
    pub fn get_recipe_for<I: RecipeInput>(
        &mut self,
        input: &I,
        level: &ServerLevel,
    ) -> Option<RecipeHolder> {
        let manager = level.recipe_access();
        let result = manager.get_recipe_for(self.ty, input, level, self.last_recipe.as_ref());
        if let Some(r) = &result {
            self.last_recipe = Some(r.id.clone());
        }
        result
    }
}

/// `RecipeCache`: an LRU of the last crafting results keyed by the ingredient layout.
pub struct RecipeCache {
    entries: Vec<Option<RecipeCacheEntry>>,
    cached_manager: Option<RecipeManager>,
}

struct RecipeCacheEntry {
    key: Vec<ItemStack>,
    width: usize,
    height: usize,
    value: Option<RecipeHolder>,
}

impl RecipeCacheEntry {
    fn matches(&self, input: &CraftingInput) -> bool {
        self.width == input.width()
            && self.height == input.height()
            && self
                .key
                .iter()
                .zip(input.items())
                .all(|(k, i)| ItemStack::is_same_item_same_components(k, i))
    }
}

impl RecipeCache {
    pub fn new(capacity: usize) -> RecipeCache {
        RecipeCache {
            entries: (0..capacity).map(|_| None).collect(),
            cached_manager: None,
        }
    }

    /// `get(level, input)`.
    pub fn get(&mut self, level: &ServerLevel, input: &CraftingInput) -> Option<RecipeHolder> {
        if input.is_empty() {
            return None;
        }
        self.validate_recipe_manager(level);
        for i in 0..self.entries.len() {
            if self.entries[i].as_ref().is_some_and(|e| e.matches(input)) {
                self.move_entry_to_front(i);
                return self.entries[0].as_ref().and_then(|e| e.value.clone());
            }
        }
        self.compute(input, level)
    }

    fn validate_recipe_manager(&mut self, level: &ServerLevel) {
        let manager = level.recipe_access();
        if !self
            .cached_manager
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.data, &manager.data))
        {
            self.cached_manager = Some(manager);
            self.entries.iter_mut().for_each(|e| *e = None);
        }
    }

    fn compute(&mut self, input: &CraftingInput, level: &ServerLevel) -> Option<RecipeHolder> {
        let recipe = level
            .recipe_access()
            .get_recipe_for(RecipeType::CRAFTING, input, level, None);
        self.insert(input, recipe.clone());
        recipe
    }

    fn move_entry_to_front(&mut self, index: usize) {
        if index > 0 {
            self.entries[..=index].rotate_right(1);
        }
    }

    fn insert(&mut self, input: &CraftingInput, recipe: Option<RecipeHolder>) {
        if self.entries.is_empty() {
            return;
        }
        let key = input.items().iter().map(|i| i.copy_with_count(1)).collect();
        self.entries.rotate_right(1);
        self.entries[0] = Some(RecipeCacheEntry {
            key,
            width: input.width(),
            height: input.height(),
            value: recipe,
        });
    }
}

impl ReloadListener for RecipeManager {
    fn name(&self) -> &'static str {
        "recipe_manager"
    }

    fn reload(&mut self, ctx: &ReloadContext) -> Result<(), String> {
        *self = RecipeManager::new(ctx.registries, ctx.enabled_features);
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[linkme::distributed_slice(RELOAD_LISTENERS)]
static RECIPE_MANAGER: ReloadListenerRegistration = ReloadListenerRegistration {
    name: "recipe_manager",
    order: ORDER_RECIPE_MANAGER,
    create: || Box::new(RecipeManager::empty()),
};

/// `new ClientboundUpdateRecipesPacket(manager.getSynchronizedItemProperties(), manager.getSynchronizedStonecutterRecipes())`.
pub fn update_recipes_packet(
    manager: &RecipeManager,
) -> mc_protocol::packets::play::inventory::ClientboundUpdateRecipesPacket {
    use mc_protocol::packets::play::inventory as wire;
    wire::ClientboundUpdateRecipesPacket {
        item_sets: manager
            .synchronized_item_properties()
            .iter()
            .map(|(key, set)| {
                (
                    key.clone(),
                    wire::RecipePropertySet {
                        items: set.items().to_vec(),
                    },
                )
            })
            .collect(),
        stonecutter_recipes: wire::SelectableRecipeSingleInputSet {
            entries: manager
                .stonecutter_recipes()
                .entries
                .iter()
                .map(|e| wire::SelectableRecipeSingleInputEntry {
                    input: wire::Ingredient {
                        values: e.input.wire_set(),
                    },
                    recipe: wire::SelectableRecipe {
                        option_display: e.recipe.option_display.clone(),
                    },
                })
                .collect(),
        },
    }
}
