//! `AbstractFurnaceBlockEntity` with its three subclasses (`FurnaceBlockEntity`, `BlastFurnaceBlockEntity`,
//! `SmokerBlockEntity`): fuel burning, cooking progress, the recipes used and their experience, saving.
//!
//! Not implemented: the furnace, smoker and blast furnace menus do not exist yet, so `menu_provider` stays `None` (the
//! block opens nothing), and neither the result slot's `awardUsedRecipesAndPopExperience` nor `fillStackedContents` exist.
//! `container_data` is what those menus read.

use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, Item, RecipeType, props};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::ComponentReader;
use mc_game_core::world::block_entity::base_container::BaseContainer;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log,
    deferred,
};
use mc_game_core::world::container::{Container, WorldlyContainer, empty_stack, helper};
use mc_game_core::world::containers::drop_item_stack;
use mc_game_core::world::entity::experience_orb::ExperienceOrb;
use mc_game_core::world::inventory::{ContainerData, MenuPlayer};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::item::crafting::{RecipeCheck, RecipeHolder, SingleRecipeInput};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::storage::loot::providers::number::{
    ResolvableFloat, ResolvableInt,
};
use mc_game_core::world::level::storage::loot::{LootContext, LootContextParamSets, LootParams};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::{
    COOKING_FUEL, CookingFuel, ResolvableFloat as ProtocolFloat, ResolvableInt as ProtocolInt,
};
use mc_text::Component;
use std::any::Any;

pub const SLOT_INPUT: usize = 0;
pub const SLOT_FUEL: usize = 1;
pub const SLOT_RESULT: usize = 2;
pub const DATA_LIT_TIME: usize = 0;
pub const DATA_LIT_DURATION: usize = 1;
pub const DATA_COOKING_PROGRESS: usize = 2;
pub const DATA_COOKING_TOTAL_TIME: usize = 3;
pub const NUM_DATA_VALUES: usize = 4;
pub const BURN_TIME_STANDARD: i32 = 200;
pub const BURN_COOL_SPEED: i32 = 2;
const SLOTS_FOR_UP: [usize; 1] = [0];
const SLOTS_FOR_DOWN: [usize; 2] = [2, 1];
const SLOTS_FOR_SIDES: [usize; 1] = [1];

/// The three subclasses of `AbstractFurnaceBlockEntity`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FurnaceKind {
    Furnace,
    BlastFurnace,
    Smoker,
}

impl FurnaceKind {
    pub fn block_entity_type(self) -> BlockEntityType {
        match self {
            Self::Furnace => BlockEntityType::FURNACE,
            Self::BlastFurnace => BlockEntityType::BLAST_FURNACE,
            Self::Smoker => BlockEntityType::SMOKER,
        }
    }

    /// The `RecipeType` handed to `RecipeManager.createCheck`.
    pub fn recipe_type(self) -> RecipeType {
        match self {
            Self::Furnace => RecipeType::SMELTING,
            Self::BlastFurnace => RecipeType::BLASTING,
            Self::Smoker => RecipeType::SMOKING,
        }
    }
}

pub struct AbstractFurnaceBlockEntity {
    base: BlockEntityData,
    pub container: BaseContainer,
    kind: FurnaceKind,
    items: [ItemStack; 3],
    lit_time_remaining: i32,
    lit_total_time: i32,
    cooking_timer: i32,
    cooking_total_time: i32,
    speed_multiplier: f32,
    /// `recipesUsed`, in the order the recipes were first used. Vanilla iterates an identity-hashed map, so its order for
    /// several recipes is not stable between runs and cannot be matched.
    recipes_used: Vec<(Identifier, i32)>,
    quick_check: RecipeCheck,
    /// `setItem(0, ..)` replaced the input with a different stack: the cooking time of the new input is owed once a level
    /// is at hand (`container_accessed`).
    input_replaced: bool,
}

impl AbstractFurnaceBlockEntity {
    pub fn new(kind: FurnaceKind, pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(kind.block_entity_type(), pos, state),
            container: BaseContainer::default(),
            kind,
            items: [ItemStack::empty(), ItemStack::empty(), ItemStack::empty()],
            lit_time_remaining: 0,
            lit_total_time: 0,
            cooking_timer: 0,
            cooking_total_time: 0,
            speed_multiplier: 1.0,
            recipes_used: Vec::new(),
            quick_check: RecipeCheck::new(kind.recipe_type()),
            input_replaced: false,
        }
    }

    pub fn kind(&self) -> FurnaceKind {
        self.kind
    }

    /// `serverTick(level, pos, state, entity)`. The level effects (`LIT`, `setChanged`) run once the entity is back in its
    /// chunk, in vanilla's order (comparators read the container when they are notified).
    fn tick(&mut self, level: &mut ServerLevel) {
        let pos = self.base.pos;
        let mut state = self.base.block_state;
        let mut changed = false;
        let mut is_lit;
        let was_lit;
        if self.lit_time_remaining > 0 {
            was_lit = true;
            self.lit_time_remaining -= 1;
            is_lit = self.lit_time_remaining > 0;
        } else {
            was_lit = false;
            is_lit = false;
        }

        let has_ingredient = !self.items[SLOT_INPUT].is_empty();
        let has_fuel = !self.items[SLOT_FUEL].is_empty();
        if is_lit || has_fuel && has_ingredient {
            if has_ingredient {
                let input = SingleRecipeInput(self.items[SLOT_INPUT].clone());
                if let Some(recipe) = self.quick_check.get_recipe_for(&input, level) {
                    let burn_result = burn_result_of(&recipe);
                    if !burn_result.is_empty()
                        && can_burn(&self.items, self.get_max_stack_size(), &burn_result)
                    {
                        if !is_lit {
                            let fuel = self.items[SLOT_FUEL].clone();
                            let new_lit_time = self.get_burn_duration(level, &fuel);
                            let new_speed_multiplier = self.get_speed_multiplier(level, &fuel);
                            self.lit_time_remaining = new_lit_time;
                            self.lit_total_time = new_lit_time;
                            self.speed_multiplier = new_speed_multiplier;
                            if self.cooking_total_time > 0
                                && self.cooking_timer < self.cooking_total_time
                            {
                                let completion_ratio =
                                    self.cooking_timer as f32 / self.cooking_total_time as f32;
                                self.cooking_total_time =
                                    get_total_cook_time(&recipe, self.speed_multiplier);
                                self.cooking_timer =
                                    (completion_ratio * self.cooking_total_time as f32).ceil()
                                        as i32;
                            }
                            if new_lit_time > 0 {
                                self.consume_fuel(level, pos);
                                is_lit = true;
                                changed = true;
                            }
                        }
                        if is_lit {
                            self.cooking_timer += 1;
                            if self.cooking_timer >= self.cooking_total_time {
                                self.cooking_timer = 0;
                                self.cooking_total_time =
                                    get_total_cook_time(&recipe, self.speed_multiplier);
                                burn(&mut self.items, &burn_result);
                                self.set_recipe_used(&recipe);
                                changed = true;
                            }
                        } else {
                            self.cooking_timer = 0;
                        }
                    } else {
                        self.cooking_timer = 0;
                    }
                }
            } else {
                self.cooking_timer = 0;
            }
        } else if self.cooking_timer > 0 {
            self.cooking_timer = mth::clamp_i32(
                self.cooking_timer - BURN_COOL_SPEED,
                0,
                self.cooking_total_time,
            );
        }

        if was_lit != is_lit {
            changed = true;
            state = state.set(props::LIT, is_lit);
            self.base.block_state = state;
            deferred::push(move |level| {
                level.set_block_and_update(pos, state);
            });
        }
        if changed {
            deferred::push(move |level| {
                mc_game_core::world::block_entity::set_changed(level, pos, state);
            });
        }
    }

    /// `consumeFuel(level, pos, items, fuel)`.
    fn consume_fuel(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        let remainder = self.items[SLOT_FUEL].item().data().crafting_remainder;
        self.items[SLOT_FUEL].shrink(1);
        if let Some(remainder) = remainder {
            if self.items[SLOT_FUEL].is_empty() {
                self.items[SLOT_FUEL] = ItemStack::new(remainder);
            } else {
                drop_item_stack(
                    level,
                    f64::from(pos.x()),
                    f64::from(pos.y()),
                    f64::from(pos.z()),
                    ItemStack::new(remainder),
                );
            }
        }
    }

    /// `getLootContext(level)` (`CONTAINER_PROCESS`: block state, block entity, origin, container) handed to `f`.
    fn with_loot_context<R>(
        &self,
        level: &ServerLevel,
        f: impl FnOnce(&mut LootContext) -> R,
    ) -> R {
        let slots = mc_game_core::server::commands::item::slots::ContainerSlots::new(self);
        let params = LootParams::builder(level)
            .block_state(self.base.block_state)
            .block_entity(Some(self as &dyn BlockEntity))
            .origin(Vec3::at_center_of(&self.base.pos))
            .container(&slots)
            .create(&LootContextParamSets::CONTAINER_PROCESS);
        let mut context = LootContext::builder(&params).create(None);
        f(&mut context)
    }

    /// `getBurnDuration(level, fuelItem)`.
    fn get_burn_duration(&self, level: &ServerLevel, fuel: &ItemStack) -> i32 {
        match fuel.get(&COOKING_FUEL) {
            Some(CookingFuel { burn_time, .. }) => self.with_loot_context(level, |context| {
                match burn_time {
                    ProtocolInt::Constant(v) => ResolvableInt::Constant(*v),
                    ProtocolInt::Reference(k) => ResolvableInt::Reference(k.clone()),
                }
                .get(context, 0)
            }),
            None => 0,
        }
    }

    /// `getSpeedMultiplier(level, fuelItem)`.
    fn get_speed_multiplier(&self, level: &ServerLevel, fuel: &ItemStack) -> f32 {
        match fuel.get(&COOKING_FUEL) {
            Some(CookingFuel {
                speed_multiplier, ..
            }) => self.with_loot_context(level, |context| {
                match speed_multiplier {
                    ProtocolFloat::Constant(v) => ResolvableFloat::Constant(*v),
                    ProtocolFloat::Reference(k) => ResolvableFloat::Reference(k.clone()),
                }
                .get(context, 1.0)
            }),
            None => 1.0,
        }
    }

    /// `getTotalCookTime(level, entity)`: the cooking time of the current input (200 without a recipe).
    fn get_total_cook_time_for_input(&mut self, level: &ServerLevel) -> i32 {
        let input = SingleRecipeInput(self.items[SLOT_INPUT].clone());
        match self.quick_check.get_recipe_for(&input, level) {
            Some(recipe) => get_total_cook_time(&recipe, self.speed_multiplier),
            None => BURN_TIME_STANDARD,
        }
    }

    /// `setRecipeUsed(recipeUsed)`.
    fn set_recipe_used(&mut self, recipe: &RecipeHolder) {
        match self
            .recipes_used
            .iter_mut()
            .find(|(id, _)| *id == recipe.id)
        {
            Some((_, n)) => *n += 1,
            None => self.recipes_used.push((recipe.id.clone(), 1)),
        }
    }

    /// `getRecipesToAwardAndPopExperience(level, position)`.
    fn get_recipes_to_award_and_pop_experience(
        &self,
        level: &mut ServerLevel,
        position: Vec3,
    ) -> Vec<RecipeHolder> {
        let mut recipes_to_award = Vec::new();
        for (key, amount) in &self.recipes_used {
            if let Some(recipe) = level.recipe_access().by_key(key).cloned() {
                let experience = recipe.value.as_cooking().map_or(0.0, |c| c.experience());
                create_experience(level, position, *amount, experience);
                recipes_to_award.push(recipe);
            }
        }
        recipes_to_award
    }
}

/// `canBurn(items, maxStackSize, burnResult)`.
fn can_burn(items: &[ItemStack; 3], max_stack_size: i32, burn_result: &ItemStack) -> bool {
    let result_item_stack = &items[SLOT_RESULT];
    if result_item_stack.is_empty() {
        true
    } else if !ItemStack::is_same_item_same_components(result_item_stack, burn_result) {
        false
    } else {
        let result_count = result_item_stack.count() + burn_result.count();
        let max_result_count = max_stack_size.min(burn_result.get_max_stack_size());
        result_count <= max_result_count
    }
}

/// `burn(items, inputItemStack, result)`.
fn burn(items: &mut [ItemStack; 3], result: &ItemStack) {
    if items[SLOT_RESULT].is_empty() {
        items[SLOT_RESULT] = result.copy();
    } else {
        items[SLOT_RESULT].grow(result.count());
    }
    if items[SLOT_INPUT].is(Item::WET_SPONGE)
        && !items[SLOT_FUEL].is_empty()
        && items[SLOT_FUEL].is(Item::BUCKET)
    {
        items[SLOT_FUEL] = ItemStack::new(Item::WATER_BUCKET);
    }
    items[SLOT_INPUT].shrink(1);
}

/// `recipe.value().assemble(input)` of a cooking recipe: the result template's stack.
fn burn_result_of(recipe: &RecipeHolder) -> ItemStack {
    recipe
        .value
        .as_cooking()
        .map_or_else(ItemStack::empty, |c| c.result().create())
}

/// `getTotalCookTime(recipe, entity)`.
fn get_total_cook_time(recipe: &RecipeHolder, speed_multiplier: f32) -> i32 {
    let cooking_total_time = recipe
        .value
        .as_cooking()
        .map_or(BURN_TIME_STANDARD, |c| c.cooking_time());
    if speed_multiplier > 0.0 {
        (cooking_total_time as f32 / speed_multiplier).ceil() as i32
    } else {
        cooking_total_time
    }
}

/// `createExperience(level, position, amount, value)`.
fn create_experience(level: &mut ServerLevel, position: Vec3, amount: i32, value: f32) {
    let product = amount as f32 * value;
    let mut xp_reward = mth::floor_f32(product);
    let xp_fraction = mth::frac_f32(product);
    if xp_fraction != 0.0 && LevelAccessor::random(level).next_float() < xp_fraction {
        xp_reward += 1;
    }
    ExperienceOrb::award(level, &position, xp_reward);
}

impl Container for AbstractFurnaceBlockEntity {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.items.len()
    }
    fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.items.get(slot).unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.items.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let result = helper::remove_item(&mut self.items, slot, count);
        if !result.is_empty() {
            self.set_changed();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.items, slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let Some(old) = self.items.get(slot) else {
            return;
        };
        let same = !stack.is_empty() && ItemStack::is_same_item_same_components(old, &stack);
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        self.items[slot] = stack;
        if slot == SLOT_INPUT && !same {
            self.input_replaced = true;
            let pos = self.base.pos;
            // containers driven without a level at hand (hopper, dropper) get the level half of `setItem` once the entity
            // is back in its chunk
            deferred::push(move |level| {
                level.with_block_entity(pos, |level, be| {
                    if be.container_pending() {
                        be.container_accessed(level);
                    }
                });
            });
        }
    }
    fn clear_content(&mut self) {
        self.items.fill(ItemStack::empty());
    }
    /// `BlockEntity.setChanged()`: the owner marks the chunk dirty after the operation (the block entity has no level).
    fn set_changed(&mut self) {
        changed_log::push(self.base.pos);
    }
    fn as_worldly(&self) -> Option<&dyn WorldlyContainer> {
        Some(self)
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        mc_game_core::world::container::still_valid_block_entity(self.base.pos, player)
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        if slot == SLOT_RESULT {
            false
        } else if slot != SLOT_FUEL {
            true
        } else {
            stack.get(&COOKING_FUEL).is_some()
                || stack.is(Item::BUCKET) && !self.items[SLOT_FUEL].is(Item::BUCKET)
        }
    }
}

impl WorldlyContainer for AbstractFurnaceBlockEntity {
    fn get_slots_for_face(&self, direction: Direction) -> &'static [usize] {
        match direction {
            Direction::Down => &SLOTS_FOR_DOWN,
            Direction::Up => &SLOTS_FOR_UP,
            _ => &SLOTS_FOR_SIDES,
        }
    }

    fn can_place_item_through_face(
        &self,
        _level: &ServerLevel,
        slot: usize,
        stack: &ItemStack,
        _direction: Option<Direction>,
    ) -> bool {
        self.can_place_item(slot, stack)
    }

    fn can_take_item_through_face(
        &self,
        slot: usize,
        stack: &ItemStack,
        direction: Direction,
    ) -> bool {
        if direction == Direction::Down && slot == SLOT_FUEL {
            mc_data::tags::get::<Item>("furnace_fuel_bottom_takeable")
                .is_some_and(|t| t.contains(stack.item()))
        } else {
            true
        }
    }
}

/// `dataAccess`.
impl ContainerData for AbstractFurnaceBlockEntity {
    fn get(&self, data_id: usize) -> i32 {
        match data_id {
            DATA_LIT_TIME => self.lit_time_remaining,
            DATA_LIT_DURATION => self.lit_total_time,
            DATA_COOKING_PROGRESS => self.cooking_timer,
            DATA_COOKING_TOTAL_TIME => self.cooking_total_time,
            _ => 0,
        }
    }

    fn set(&mut self, data_id: usize, value: i32) {
        match data_id {
            DATA_LIT_TIME => self.lit_time_remaining = value,
            DATA_LIT_DURATION => self.lit_total_time = value,
            DATA_COOKING_PROGRESS => self.cooking_timer = value,
            DATA_COOKING_TOTAL_TIME => self.cooking_total_time = value,
            _ => {}
        }
    }

    fn get_count(&self) -> usize {
        NUM_DATA_VALUES
    }
}

impl BlockEntity for AbstractFurnaceBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.items = [ItemStack::empty(), ItemStack::empty(), ItemStack::empty()];
        helper::load_all_items(tag, &mut self.items);
        self.cooking_timer = tag.get_int_or("cooking_time_spent", 0);
        self.cooking_total_time = tag.get_int_or("cooking_total_time", 0);
        self.lit_time_remaining = tag.get_int_or("lit_time_remaining", 0);
        self.lit_total_time = tag.get_int_or("lit_total_time", 0);
        self.speed_multiplier = tag.get_float_or("speed_multiplier", 1.0);
        self.recipes_used.clear();
        if let Some(used) = tag.get_compound("RecipesUsed") {
            for (key, value) in used.iter() {
                if let (Ok(id), Some(n)) = (Identifier::parse(key), value.as_int()) {
                    self.recipes_used.push((id, n));
                }
            }
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        tag.put_int("cooking_time_spent", self.cooking_timer);
        tag.put_int("cooking_total_time", self.cooking_total_time);
        tag.put_int("lit_time_remaining", self.lit_time_remaining);
        tag.put_int("lit_total_time", self.lit_total_time);
        tag.put_float("speed_multiplier", self.speed_multiplier);
        if let Some(items) = helper::save_all_items(&self.items, true) {
            tag.put("Items", Tag::List(items));
        }
        let mut used = CompoundTag::new();
        for (id, n) in &self.recipes_used {
            used.put_int(id.to_string(), *n);
        }
        tag.put("RecipesUsed", used);
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        self.container.remove_components_from_tag(tag);
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        self.container
            .apply_implicit_components(components, &mut self.items);
    }

    fn collect_implicit_components(
        &self,
        builder: &mut mc_protocol::components::map::DataComponentMapBuilder,
    ) {
        *builder = self
            .container
            .collect_implicit_components(std::mem::take(builder), &self.items);
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.container.get_custom_name().cloned()
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        self.tick(level);
    }

    /// The level half of a `setItem` on the input (cooking time, `setChanged`) and the recorded `setChanged()` calls.
    fn container_pending(&self) -> bool {
        self.input_replaced || changed_log::has_pending()
    }

    fn container_accessed(&mut self, level: &mut ServerLevel) {
        if std::mem::take(&mut self.input_replaced) {
            self.cooking_total_time = self.get_total_cook_time_for_input(level);
            self.cooking_timer = 0;
            changed_log::push(self.base.pos);
        }
        // the comparators notified by `setChanged` read this entity: replay once it is back in its chunk
        deferred::push(changed_log::flush);
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_data(&self) -> Option<&dyn ContainerData> {
        Some(self)
    }

    fn container_data_mut(&mut self) -> Option<&mut dyn ContainerData> {
        Some(self)
    }

    /// `preRemoveSideEffects`: drops the contents, then pops the experience of the recipes used.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
    ) {
        mc_game_core::world::containers::drop_contents_at(level, pos, &*self);
        self.get_recipes_to_award_and_pop_experience(level, Vec3::at_center_of(&pos));
    }
}

macro_rules! register_furnace {
    ($static:ident, $ty:ident, $kind:ident) => {
        #[distributed_slice(BLOCK_ENTITY_TYPES)]
        static $static: BlockEntityRegistration = BlockEntityRegistration {
            ty: BlockEntityType::$ty,
            create: |pos, state| {
                Box::new(AbstractFurnaceBlockEntity::new(
                    FurnaceKind::$kind,
                    pos,
                    state,
                ))
            },
        };
    };
}
register_furnace!(FURNACE, FURNACE, Furnace);
register_furnace!(BLAST_FURNACE, BLAST_FURNACE, BlastFurnace);
register_furnace!(SMOKER, SMOKER, Smoker);
