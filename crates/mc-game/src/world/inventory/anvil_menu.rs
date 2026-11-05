//! `AnvilMenu` (`extends ItemCombinerMenu`): repair, combine, enchant-merge and rename with vanilla's cost rules.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, DataSlot, ItemCombinerMenu, ItemCombinerMenuCore, ItemCombinerMenuSlotDefinition, MenuBase,
    MenuPlayer, Slot, SlotBehavior, SlotEnv,
};
use crate::world::item::ItemStack;
use crate::world::item::enchantment::enchantment::Enchantment;
use crate::world::item::enchantment::item_enchantments::{ItemEnchantmentsMutable, holder};
use crate::world::item::enchantment_helper::{can_store_enchantments, definition, get_enchantments_for_crafting, set_enchantments};
use crate::world::item::ItemStackExt;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_data::block::BlockState;
use mc_data::{Item, MenuType};
use mc_data::DataComponentType;
use mc_protocol::components::{CUSTOM_NAME, REPAIR_COST, REPAIRABLE};
use mc_text::Component;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub const INPUT_SLOT: usize = 0;
pub const ADDITIONAL_SLOT: usize = 1;
pub const RESULT_SLOT: usize = 2;
pub const MAX_NAME_LENGTH: usize = 50;

/// `StringUtil.isBlank`: `Character.isWhitespace || Character.isSpaceChar` over UTF-16 code units (Rust's `White_Space`
/// differs at U+0085 and U+001C..U+001F).
pub fn is_blank(s: &str) -> bool {
    s.chars().all(|c| (c.is_whitespace() && c != '\u{85}') || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// `StringUtil.filterText(input)`: drops `isAllowedChatCharacter`-violating UTF-16 code units.
pub fn filter_text(input: &str) -> String {
    let units: Vec<u16> = input.encode_utf16().filter(|c| *c != 167 && *c >= 32 && *c != 127).collect();
    String::from_utf16_lossy(&units)
}

/// `AnvilMenu.calculateIncreasedRepairCost(baseCost)`.
pub fn calculate_increased_repair_cost(base_cost: i32) -> i32 {
    (i64::from(base_cost) * 2 + 1).min(i64::from(i32::MAX)) as i32
}

/// `ItemStack.isValidRepairItem(repairItem)`.
pub fn is_valid_repair_item(stack: &ItemStack, repair_item: &ItemStack, registries: &crate::registry::RegistryAccess) -> bool {
    stack.get(&REPAIRABLE).is_some_and(|r| crate::world::item::enchantment::enchantment::item_set_contains(registries, &r.items, repair_item.item()) && !repair_item.is_empty())
}

/// The menu state the result slot's `mayPickup`/`onTake` read and write (`cost`, `repairItemCountCost`, `itemName`,
/// `onlyRenaming`).
struct AnvilState {
    cost: Rc<RefCell<Vec<i32>>>,
    repair_item_count_cost: Cell<i32>,
    item_name: RefCell<Option<String>>,
    only_renaming: Cell<bool>,
}

impl AnvilState {
    fn cost(&self) -> i32 {
        self.cost.borrow()[0]
    }
    fn set_cost(&self, v: i32) {
        self.cost.borrow_mut()[0] = v;
    }
}

pub struct AnvilMenu {
    core: ItemCombinerMenuCore,
    state: Rc<AnvilState>,
}

/// The anvil's result slot: `mayPickup` needs the level price, `onTake` pays it and consumes the inputs.
struct AnvilResultSlot {
    state: Rc<AnvilState>,
    access: ContainerLevelAccess,
    input_local: usize,
}

impl SlotBehavior for AnvilResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn may_pickup(&self, _slot: &Slot, env: &mut SlotEnv) -> bool {
        let cost = self.state.cost();
        (env.player.has_infinite_materials() || env.player.experience_level() >= cost) && cost > 0
    }

    fn on_take(&self, _slot: &Slot, env: &mut SlotEnv, _carried: &ItemStack) {
        let infinite = env.player.has_infinite_materials();
        if !infinite {
            env.player.give_experience_levels(self.state.cost().wrapping_neg());
        }
        let repair_count = self.state.repair_item_count_cost.get();
        let input = env.locals[self.input_local].as_mut();
        if repair_count > 0 {
            let mut addition = input.get_item(ADDITIONAL_SLOT).clone();
            if !addition.is_empty() && addition.count() > repair_count {
                addition.shrink(repair_count);
                input.set_item(ADDITIONAL_SLOT, addition);
            } else {
                input.set_item(ADDITIONAL_SLOT, ItemStack::empty());
            }
        } else if !self.state.only_renaming.get() {
            input.set_item(ADDITIONAL_SLOT, ItemStack::empty());
        }
        self.state.set_cost(0);
        let item_name = self.state.item_name.borrow().clone();
        if let Some(name) = item_name {
            if !is_blank(&name) && env.locals[self.input_local].get_item(INPUT_SLOT).get_hover_name().get_string() != name {
                env.player.process_stream_message(&name);
            }
        }
        env.locals[self.input_local].set_item(INPUT_SLOT, ItemStack::empty());
        if let Some(pos) = self.access.pos() {
            let random_roll = if !infinite {
                // `!hasInfiniteMaterials() && state.is(ANVIL) && random < 0.12F`: the random is drawn only when the block is an anvil
                let is_anvil = env.player.level_ref().is_some_and(|l| l.get_block_state(pos).is_tag("minecraft:anvil"));
                is_anvil && env.player.player_random_next_float() < 0.12
            } else {
                false
            };
            if let Some(level) = env.player.level() {
                if random_roll {
                    let state = level.get_block_state(pos);
                    match crate::world::level::block::impls::anvil_block::damage(state) {
                        None => {
                            level.remove_block(pos, false);
                            level.level_event(None, 1029, pos, 0);
                        }
                        Some(damaged) => {
                            level.set_block(pos, damaged, 2, 512);
                            level.level_event(None, 1030, pos, 0);
                        }
                    }
                } else {
                    level.level_event(None, 1030, pos, 0);
                }
            }
        }
    }
}

impl AnvilMenu {
    /// `AnvilMenu(containerId, inventory, access)`.
    pub fn new(container_id: i32, _player: &mut dyn MenuPlayer, access: ContainerLevelAccess) -> Self {
        let cost = Rc::new(RefCell::new(vec![0]));
        let state = Rc::new(AnvilState { cost: cost.clone(), repair_item_count_cost: Cell::new(0), item_name: RefCell::new(None), only_renaming: Cell::new(false) });
        let definition = ItemCombinerMenuSlotDefinition::create()
            .with_slot(0, 27, 47, |_| true)
            .with_slot(1, 76, 47, |_| true)
            .with_result_slot(2, 134, 47)
            .build();
        // the result container is local 1 of the core (inputs local 0)
        let result = AnvilResultSlot { state: state.clone(), access, input_local: 0 };
        let mut core = ItemCombinerMenuCore::new(Some(MenuType::ANVIL), container_id, access, definition, Box::new(result));
        debug_assert_eq!(core.input_local, 0);
        core.base.add_data_slot(DataSlot::shared(cost, 0));
        Self { core, state }
    }

    /// `getCost()`.
    pub fn get_cost(&self) -> i32 {
        self.state.cost()
    }

    fn input_stack(&self, slot: usize) -> ItemStack {
        self.core.base.locals[self.core.input_local].get_item(slot).clone()
    }

    fn set_result(&mut self, stack: ItemStack) {
        let local = self.core.result_local;
        self.core.base.locals[local].set_item(0, stack);
    }

    /// `validateName(name)`: filtered text, `None` when longer than [`MAX_NAME_LENGTH`] UTF-16 units.
    fn validate_name(name: &str) -> Option<String> {
        let filtered = filter_text(name);
        (filtered.encode_utf16().count() <= MAX_NAME_LENGTH).then_some(filtered)
    }

    /// `setItemName(name)`.
    pub fn set_item_name(&mut self, player: &mut dyn MenuPlayer, name: &str) -> bool {
        let Some(validated) = Self::validate_name(name) else { return false };
        if self.state.item_name.borrow().as_deref() == Some(validated.as_str()) {
            return false;
        }
        *self.state.item_name.borrow_mut() = Some(validated.clone());
        let result_local = self.core.result_local;
        if let Some(stack) = self.core.base.locals[result_local].get_item_mut(0) {
            if !stack.is_empty() {
                if is_blank(&validated) {
                    stack.remove(DataComponentType::CUSTOM_NAME);
                } else {
                    stack.set(CUSTOM_NAME, Component::literal(validated));
                }
            }
        }
        self.create_result(player);
        true
    }
}

impl ItemCombinerMenu for AnvilMenu {
    fn core(&self) -> &ItemCombinerMenuCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut ItemCombinerMenuCore {
        &mut self.core
    }

    fn is_valid_block(&self, state: BlockState) -> bool {
        state.is_tag("minecraft:anvil")
    }

    fn create_result(&mut self, player: &mut dyn MenuPlayer) {
        let Some(registries) = player.level_ref().map(|l| l.registries.clone()) else {
            // a menu without a level cannot evaluate enchantments (never happens on a server)
            self.set_result(ItemStack::empty());
            self.state.set_cost(0);
            return;
        };
        let infinite = player.has_infinite_materials();
        let input = self.input_stack(INPUT_SLOT);
        self.state.only_renaming.set(false);
        self.state.set_cost(1);
        let mut price: i32 = 0;
        let mut tax: i64 = 0;
        let mut naming_cost: i32 = 0;
        if !input.is_empty() && can_store_enchantments(&input) {
            let mut result = input.copy();
            let addition = self.input_stack(ADDITIONAL_SLOT);
            let mut enchantments = ItemEnchantmentsMutable::new(&get_enchantments_for_crafting(&result));
            tax += i64::from(input.get(&REPAIR_COST).copied().unwrap_or(0)) + i64::from(addition.get(&REPAIR_COST).copied().unwrap_or(0));
            self.state.repair_item_count_cost.set(0);
            if !addition.is_empty() {
                let using_book = addition.has(DataComponentType::STORED_ENCHANTMENTS);
                if result.is_damageable_item() && is_valid_repair_item(&input, &addition, &registries) {
                    let mut repair_amount = result.get_damage_value().min(result.get_max_damage() / 4);
                    if repair_amount <= 0 {
                        self.set_result(ItemStack::empty());
                        self.state.set_cost(0);
                        return;
                    }
                    let mut count = 0;
                    while repair_amount > 0 && count < addition.count() {
                        let result_damage = result.get_damage_value() - repair_amount;
                        result.set_damage_value(result_damage);
                        price += 1;
                        repair_amount = result.get_damage_value().min(result.get_max_damage() / 4);
                        count += 1;
                    }
                    self.state.repair_item_count_cost.set(count);
                } else {
                    if !using_book && (!result.is(addition.item()) || !result.is_damageable_item()) {
                        self.set_result(ItemStack::empty());
                        self.state.set_cost(0);
                        return;
                    }
                    if result.is_damageable_item() && !using_book {
                        let remaining1 = input.get_max_damage() - input.get_damage_value();
                        let remaining2 = addition.get_max_damage() - addition.get_damage_value();
                        let additional = remaining2 + result.get_max_damage().wrapping_mul(12) / 100;
                        let remaining = remaining1 + additional;
                        let mut result_damage = result.get_max_damage() - remaining;
                        if result_damage < 0 {
                            result_damage = 0;
                        }
                        if result_damage < result.get_damage_value() {
                            result.set_damage_value(result_damage);
                            price += 2;
                        }
                    }
                    let additional_enchantments = get_enchantments_for_crafting(&addition);
                    let mut any_compatible = false;
                    let mut any_not_compatible = false;
                    for (enchantment_holder, entry_level) in additional_enchantments.entries() {
                        let Some(key) = enchantment_holder.name() else { continue };
                        let Some(enchantment) = definition(&registries, key) else { continue };
                        let current = enchantments.get_level(enchantment_holder);
                        let mut level = *entry_level;
                        level = if current == level { level + 1 } else { level.max(current) };
                        let mut compatible = enchantment.can_enchant(&registries, &input);
                        if infinite || input.is(Item::ENCHANTED_BOOK) {
                            compatible = true;
                        }
                        let others: Vec<_> = enchantments.key_set().cloned().collect();
                        for other in &others {
                            let Some(other_key) = other.name() else { continue };
                            if other != enchantment_holder && !Enchantment::are_compatible(&registries, key, other_key) {
                                compatible = false;
                                price += 1;
                            }
                        }
                        if !compatible {
                            any_not_compatible = true;
                        } else {
                            any_compatible = true;
                            if level > enchantment.get_max_level() {
                                level = enchantment.get_max_level();
                            }
                            enchantments.set(enchantment_holder, level);
                            let mut fee = enchantment.get_anvil_cost();
                            if using_book {
                                fee = (fee / 2).max(1);
                            }
                            price = price.wrapping_add(fee.wrapping_mul(level));
                            if input.count() > 1 {
                                price = 40;
                            }
                        }
                    }
                    if any_not_compatible && !any_compatible {
                        self.set_result(ItemStack::empty());
                        self.state.set_cost(0);
                        return;
                    }
                }
            }

            let item_name = self.state.item_name.borrow().clone();
            match item_name.as_deref().filter(|n| !is_blank(n)) {
                Some(name) => {
                    if name != input.get_hover_name().get_string() {
                        naming_cost = 1;
                        price += naming_cost;
                        result.set(CUSTOM_NAME, Component::literal(name));
                    }
                }
                None if input.has(DataComponentType::CUSTOM_NAME) => {
                    naming_cost = 1;
                    price += naming_cost;
                    result.remove(DataComponentType::CUSTOM_NAME);
                }
                None => {}
            }

            let final_price = if price <= 0 { 0 } else { (tax + i64::from(price)).clamp(0, i64::from(i32::MAX)) as i32 };
            self.state.set_cost(final_price);
            if price <= 0 {
                result = ItemStack::empty();
            }
            if naming_cost == price && naming_cost > 0 {
                if self.state.cost() >= 40 {
                    self.state.set_cost(39);
                }
                self.state.only_renaming.set(true);
            }
            if self.state.cost() >= 40 && !infinite {
                result = ItemStack::empty();
            }
            if !result.is_empty() {
                let mut base_cost = result.get(&REPAIR_COST).copied().unwrap_or(0);
                let addition_cost = addition.get(&REPAIR_COST).copied().unwrap_or(0);
                if base_cost < addition_cost {
                    base_cost = addition_cost;
                }
                if naming_cost != price || naming_cost == 0 {
                    base_cost = calculate_increased_repair_cost(base_cost);
                }
                result.set(REPAIR_COST, base_cost);
                set_enchantments(&mut result, enchantments.to_immutable());
            }
            self.set_result(result);
            self.core.base.broadcast_changes(player);
        } else {
            self.set_result(ItemStack::empty());
            self.state.set_cost(0);
        }
    }
}

impl AbstractContainerMenu for AnvilMenu {
    fn base(&self) -> &MenuBase {
        &self.core.base
    }
    fn base_mut(&mut self) -> &mut MenuBase {
        &mut self.core.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        self.combiner_slots_changed(player, container);
    }
    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.combiner_removed(player);
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.combiner_still_valid(player)
    }
    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        self.combiner_quick_move_stack(player, slot_index)
    }
}
