//! `BrewingStandBlockEntity`: 5 slots (3 potion bottles, ingredient, fuel), brew time / fuel counters, the 400 tick
//! (scaled by the fuel's `speed_multiplier`) brewing cycle driven by the `minecraft:brewing` recipes.

use super::base_container::{BaseContainer, send_chest_locked_notifications};
use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, WorldlyContainer, empty_stack, helper};
use crate::world::containers::drop_item_stack;
use crate::world::inventory::{AbstractContainerMenu, BrewingStandMenu, ContainerData, MenuPlayer, MenuProvider};
use crate::world::item::crafting::{BrewingInput, PotionIngredient, RecipeCheck, RecipePropertySet, RecipeType};
use crate::world::item::{ItemStack, ItemStackExt};
use crate::world::level::storage::loot::{LootContext, LootContextParamSets, LootParams};
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{BlockEntityType, Item, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::BREWING_FUEL;
use mc_text::Component;
use std::any::Any;

pub const INGREDIENT_SLOT: usize = 3;
pub const FUEL_SLOT: usize = 4;
const SLOTS_FOR_UP: [usize; 1] = [3];
const SLOTS_FOR_DOWN: [usize; 4] = [0, 1, 2, 3];
const SLOTS_FOR_SIDES: [usize; 4] = [0, 1, 2, 4];
pub const DATA_BREW_TIME: usize = 0;
pub const DATA_FUEL_USES: usize = 1;
pub const DATA_TOTAL_BREW_TIME: usize = 2;
pub const DATA_TOTAL_FUEL_USES: usize = 3;
pub const NUM_DATA_VALUES: usize = 4;
/// `BrewingStandBlock.HAS_BOTTLE`.
const HAS_BOTTLE: [mc_data::props::BooleanProperty; 3] = [props::HAS_BOTTLE_0, props::HAS_BOTTLE_1, props::HAS_BOTTLE_2];

pub struct BrewingStandBlockEntity {
    base: BlockEntityData,
    container: BaseContainer,
    items: Vec<ItemStack>,
    brew_time: i32,
    total_brew_time: i32,
    /// `lastPotionCount` (`null` until the first tick).
    last_potion_count: Option<[bool; 3]>,
    /// `ingredient` (`null` until a brew starts or the saved brew time is positive).
    ingredient: Option<Item>,
    fuel: i32,
    total_fuel: i32,
    speed_multiplier: f32,
    quick_check: RecipeCheck,
}

impl BrewingStandBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::BREWING_STAND, pos, state),
            container: BaseContainer::default(),
            items: vec![ItemStack::empty(); 5],
            brew_time: 0,
            total_brew_time: 0,
            last_potion_count: None,
            ingredient: None,
            fuel: 0,
            total_fuel: 0,
            speed_multiplier: 1.0,
            quick_check: RecipeCheck::new(RecipeType::Brewing),
        }
    }

    /// `getDefaultName`.
    pub fn default_name() -> Component {
        Component::translatable("container.brewing", Vec::new())
    }

    pub fn container_base(&self) -> &BaseContainer {
        &self.container
    }

    pub fn container_base_mut(&mut self) -> &mut BaseContainer {
        &mut self.container
    }

    /// `getLootContext(level)` (`CONTAINER_PROCESS`: block state, block entity, origin, container) handed to `f`.
    fn with_loot_context<R>(&self, level: &ServerLevel, f: impl FnOnce(&mut LootContext) -> R) -> R {
        let slots = crate::server::commands::item::slots::ContainerSlots::new(self);
        let params = LootParams::builder(level)
            .block_state(self.base.block_state)
            .block_entity(Some(self as &dyn BlockEntity))
            .origin(mc_core::phys::Vec3::at_center_of(&self.base.pos))
            .container(&slots)
            .create(&LootContextParamSets::CONTAINER_PROCESS);
        let mut context = LootContext::builder(&params).create(None);
        f(&mut context)
    }

    /// `getUses(level, brewingFuel)`.
    fn get_uses(&self, level: &ServerLevel, fuel: &mc_protocol::components::BrewingFuel) -> i32 {
        self.with_loot_context(level, |context| fuel.uses.get(context, 0))
    }

    /// `getSpeedMultiplier(level, brewingFuel)`.
    fn get_speed_multiplier(&self, level: &ServerLevel, fuel: &mc_protocol::components::BrewingFuel) -> f32 {
        self.with_loot_context(level, |context| fuel.speed_multiplier.get(context, 1.0))
    }

    /// `BlockEntity.setChanged(level, pos, state)`.
    fn set_changed_in(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        super::set_changed(level, pos, state);
    }

    /// `MenuProvider` of the brewing stand at `pos` (`BaseContainerBlockEntity.createMenu`: lock check, then the menu).
    pub fn menu_provider(level: &dyn BlockGetter, pos: BlockPos) -> Option<BrewingStandMenuProvider> {
        let be = level.get_block_entity(pos)?.as_any().downcast_ref::<BrewingStandBlockEntity>()?;
        Some(BrewingStandMenuProvider { pos, title: be.container.get_name(Self::default_name()) })
    }

    /// `getPotionBits()`.
    fn potion_bits(&self) -> [bool; 3] {
        [!self.items[0].is_empty(), !self.items[1].is_empty(), !self.items[2].is_empty()]
    }

    /// `isBrewable(level, entity)`.
    fn is_brewable(&mut self, level: &ServerLevel) -> bool {
        let ingredient = &self.items[INGREDIENT_SLOT];
        if ingredient.is_empty() {
            return false;
        }
        let recipes = level.recipe_access();
        if !recipes.property_set(RecipePropertySet::BREWING_REAGENTS).test(ingredient) {
            return false;
        }
        for dest in 0..3 {
            let stack = &self.items[dest];
            if !stack.is_empty() && self.quick_check.get_recipe_for(&BrewingInput { input: stack.clone(), reagent: ingredient.clone() }, level).is_some() {
                return true;
            }
        }
        false
    }

    /// `doBrew(level, pos, entity)`.
    fn do_brew(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        let mut ingredient = self.items[INGREDIENT_SLOT].clone();
        for dest in 0..3 {
            let container = self.items[dest].clone();
            let input = BrewingInput { input: container.clone(), reagent: ingredient.clone() };
            self.items[dest] = match self.quick_check.get_recipe_for(&input, level) {
                Some(recipe) => recipe.value.as_brewing().map_or_else(ItemStack::empty, |r| r.output().create()),
                None => container,
            };
        }
        let remainder = ingredient.item().data().crafting_remainder;
        ingredient.shrink(1);
        if let Some(remainder) = remainder {
            if ingredient.is_empty() {
                ingredient = ItemStack::new(remainder);
            } else {
                drop_item_stack(level, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), ItemStack::new(remainder));
            }
        }
        self.items[INGREDIENT_SLOT] = ingredient;
        level.level_event(None, 1035, pos, 0);
    }

    /// `canPlaceItem(slot, stack)` for a known level.
    pub fn can_place_item_in(&self, level: &ServerLevel, slot: usize, stack: &ItemStack) -> bool {
        if slot == FUEL_SLOT {
            stack.has(mc_data::DataComponentType::BREWING_FUEL)
        } else {
            let recipes = level.recipe_access();
            if slot == INGREDIENT_SLOT {
                recipes.property_set(RecipePropertySet::BREWING_REAGENTS).test(stack)
            } else {
                PotionIngredient::is_potion_input(stack, &recipes) && self.items[slot].is_empty()
            }
        }
    }
}

impl BlockEntity for BrewingStandBlockEntity {
    super::block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.items = vec![ItemStack::empty(); 5];
        helper::load_all_items(tag, &mut self.items);
        self.brew_time = tag.get_int_or("BrewTime", 0);
        self.total_brew_time = tag.get_int_or("total_brew_time", 400);
        if self.brew_time > 0 {
            self.ingredient = Some(self.items[INGREDIENT_SLOT].item());
        }
        self.fuel = tag.get_int_or("Fuel", 0);
        self.total_fuel = tag.get_int_or("total_fuel", 20);
        self.speed_multiplier = tag.get_float_or("speed_multiplier", 1.0);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        tag.put_int("BrewTime", self.brew_time);
        tag.put_int("total_brew_time", self.total_brew_time);
        if let Some(items) = helper::save_all_items(&self.items, false) {
            tag.put("Items", mc_nbt::Tag::List(items));
        }
        tag.put_int("Fuel", self.fuel);
        tag.put_int("total_fuel", self.total_fuel);
        tag.put_float("speed_multiplier", self.speed_multiplier);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    /// `BrewingStandBlockEntity.serverTick`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.base.pos;
        let self_state = self.base.block_state;
        let brewing_fuel = self.items[FUEL_SLOT].get(&BREWING_FUEL).cloned();
        if self.fuel <= 0
            && let Some(brewing_fuel) = brewing_fuel
        {
            self.fuel = self.get_uses(&*level, &brewing_fuel);
            self.total_fuel = self.fuel;
            self.speed_multiplier = self.get_speed_multiplier(&*level, &brewing_fuel);
            let remainder = self.items[FUEL_SLOT].item().data().crafting_remainder;
            self.items[FUEL_SLOT].shrink(1);
            if let Some(remainder) = remainder {
                if self.items[FUEL_SLOT].is_empty() {
                    self.items[FUEL_SLOT] = ItemStack::new(remainder);
                } else {
                    drop_item_stack(level, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), ItemStack::new(remainder));
                }
            }
            Self::set_changed_in(level, pos, self_state);
        }

        let brewable = self.is_brewable(level);
        let is_brewing = self.brew_time > 0;
        if is_brewing {
            self.brew_time -= 1;
            let is_done_brewing = self.brew_time == 0;
            if is_done_brewing && brewable {
                self.do_brew(level, pos);
            } else if !brewable || Some(self.items[INGREDIENT_SLOT].item()) != self.ingredient {
                self.brew_time = 0;
            }
            Self::set_changed_in(level, pos, self_state);
        } else if brewable && self.fuel > 0 {
            let speed_multiplier = if self.speed_multiplier > 0.0 { self.speed_multiplier } else { 1.0 };
            self.fuel -= 1;
            self.brew_time = (400.0f32 / speed_multiplier).ceil() as i32;
            self.total_brew_time = self.brew_time;
            self.ingredient = Some(self.items[INGREDIENT_SLOT].item());
            Self::set_changed_in(level, pos, self_state);
        }

        let new_count = self.potion_bits();
        if Some(new_count) != self.last_potion_count {
            self.last_potion_count = Some(new_count);
            if self_state.block() != Block::BREWING_STAND {
                return;
            }
            let mut state = self_state;
            for (i, has_bottle) in new_count.iter().enumerate() {
                state = state.set(HAS_BOTTLE[i], *has_bottle);
            }
            level.set_block(pos, state, 2, 512);
        }
    }

    /// `preRemoveSideEffects`: `Containers.dropContents(level, pos, this)` (a `BaseContainerBlockEntity`).
    fn pre_remove_side_effects(&mut self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState) {
        crate::world::containers::drop_contents_list(level, pos, &self.items);
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.container.apply_implicit_components(stack, &mut self.items);
    }

    /// Menu operations replay the recorded `setChanged()` calls right after the operation.
    fn container_pending(&self) -> bool {
        changed_log::has_pending()
    }
    fn container_accessed(&mut self, level: &mut ServerLevel) {
        changed_log::flush(level);
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
}

/// `dataAccess`.
impl ContainerData for BrewingStandBlockEntity {
    fn get(&self, data_id: usize) -> i32 {
        match data_id {
            DATA_BREW_TIME => self.brew_time,
            DATA_FUEL_USES => self.fuel,
            DATA_TOTAL_BREW_TIME => self.total_brew_time,
            DATA_TOTAL_FUEL_USES => self.total_fuel,
            _ => 0,
        }
    }

    fn set(&mut self, data_id: usize, value: i32) {
        match data_id {
            DATA_BREW_TIME => self.brew_time = value,
            DATA_FUEL_USES => self.fuel = value,
            DATA_TOTAL_BREW_TIME => self.total_brew_time = value,
            DATA_TOTAL_FUEL_USES => self.total_fuel = value,
            _ => {}
        }
    }

    fn get_count(&self) -> usize {
        NUM_DATA_VALUES
    }
}

/// `BaseContainerBlockEntity` container methods over the 5 stacks.
impl Container for BrewingStandBlockEntity {
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
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        if let Some(s) = self.items.get_mut(slot) {
            *s = stack;
        }
        self.set_changed();
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
        crate::world::container::still_valid_block_entity(self.base.pos, player)
    }
    /// `canPlaceItem` of a block entity whose `level` is `null` (only the fuel slot answers); with a level use
    /// [`BrewingStandBlockEntity::can_place_item_in`] / the `WorldlyContainer` face methods.
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        slot == FUEL_SLOT && stack.has(mc_data::DataComponentType::BREWING_FUEL)
    }
}

impl WorldlyContainer for BrewingStandBlockEntity {
    fn get_slots_for_face(&self, direction: Direction) -> &'static [usize] {
        match direction {
            Direction::Up => &SLOTS_FOR_UP,
            Direction::Down => &SLOTS_FOR_DOWN,
            _ => &SLOTS_FOR_SIDES,
        }
    }

    fn can_place_item_through_face(&self, level: &ServerLevel, slot: usize, stack: &ItemStack, _direction: Option<Direction>) -> bool {
        self.can_place_item_in(level, slot, stack)
    }

    fn can_take_item_through_face(&self, slot: usize, stack: &ItemStack, _direction: Direction) -> bool {
        if slot == INGREDIENT_SLOT { stack.is(Item::GLASS_BOTTLE) } else { true }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static BREWING_STAND: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::BREWING_STAND, create: |pos, state| Box::new(BrewingStandBlockEntity::new(pos, state)) };

/// `BaseContainerBlockEntity` as `MenuProvider` (`getDisplayName()` = `getName()`).
pub struct BrewingStandMenuProvider {
    pos: BlockPos,
    title: Component,
}

impl MenuProvider for BrewingStandMenuProvider {
    fn get_display_name(&self) -> Component {
        self.title.clone()
    }

    fn create_menu(&self, container_id: i32, player: &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>> {
        let pos = self.pos;
        let can_open = {
            let be = player.level_ref()?.get_block_entity(pos)?.as_any().downcast_ref::<BrewingStandBlockEntity>()?;
            be.container.can_open(&*player)
        };
        if can_open {
            Some(Box::new(BrewingStandMenu::new(container_id, pos)))
        } else {
            let title = self.title.clone();
            player.with_server_player(&mut |level, server_player| {
                send_chest_locked_notifications(level, mc_core::phys::Vec3::at_center_of(&pos), server_player, &title);
            });
            None
        }
    }
}
