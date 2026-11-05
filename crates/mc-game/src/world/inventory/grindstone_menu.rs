//! `GrindstoneMenu`: disenchant (keeping curses), merge two items, refund experience.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, MenuBase, MenuPlayer, MenuSimpleContainer, ResultContainer, Slot, SlotBehavior, SlotEnv,
    finish_quick_move,
};
use crate::registry::RegistryAccess;
use crate::registry::keys::ENCHANTMENT as ENCHANTMENT_REGISTRY;
use crate::world::container::Container;
use crate::world::entity::experience_orb::ExperienceOrb;
use super::anvil_menu::calculate_increased_repair_cost;
use crate::world::item::ItemStack;
use crate::world::item::enchantment::enchantment_tags;
use crate::world::item::enchantment_helper::{definition, get_enchantments_for_crafting, has_any_enchantments, update_enchantments};
use mc_core::phys::Vec3;
use mc_data::{Block, Item, MenuType};
use mc_protocol::components::{MAX_DAMAGE, REPAIR_COST};
use std::any::Any;

pub const MAX_NAME_LENGTH: usize = 35;
pub const INPUT_SLOT: usize = 0;
pub const ADDITIONAL_SLOT: usize = 1;
pub const RESULT_SLOT: usize = 2;
const INV_SLOT_START: usize = 3;
const INV_SLOT_END: usize = 30;
const USE_ROW_SLOT_START: usize = 30;
const USE_ROW_SLOT_END: usize = 39;

/// Menu-local containers.
const RESULT_LOCAL: usize = 0;
const REPAIR_LOCAL: usize = 1;

pub struct GrindstoneMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
}

/// The two input slots: `mayPlace` = damageable or enchanted.
struct GrindstoneInputSlot;

impl SlotBehavior for GrindstoneInputSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        stack.is_damageable_item() || has_any_enchantments(stack)
    }
}

/// The result slot: `onTake` awards the experience and clears the inputs.
struct GrindstoneResultSlot {
    access: ContainerLevelAccess,
}

/// `getExperienceFromItem(item)`: the minimum cost of every non-curse enchantment.
fn get_experience_from_item(registries: &RegistryAccess, item: &ItemStack) -> i32 {
    let mut amount = 0;
    for (enchant, lvl) in get_enchantments_for_crafting(item).entries() {
        let Some(key) = enchant.name() else { continue };
        if !registries.is_in_tag(ENCHANTMENT_REGISTRY, key, &enchantment_tags::id(enchantment_tags::CURSE)) {
            if let Some(e) = definition(registries, key) {
                amount += e.get_min_cost(*lvl);
            }
        }
    }
    amount
}

impl SlotBehavior for GrindstoneResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn on_take(&self, _slot: &Slot, env: &mut SlotEnv, _carried: &ItemStack) {
        if let Some(pos) = self.access.pos() {
            // the amount is computed while the awarded orbs are being created: it reads the inputs and draws from the level's random
            let items = [env.locals[REPAIR_LOCAL].get_item(0).clone(), env.locals[REPAIR_LOCAL].get_item(1).clone()];
            if let Some(level) = env.player.level() {
                let registries = level.registries.clone();
                use crate::world::level::LevelAccessor;
                let mut amount = get_experience_from_item(&registries, &items[0]) + get_experience_from_item(&registries, &items[1]);
                if amount > 0 {
                    let half_amount = (f64::from(amount) / 2.0).ceil() as i32;
                    amount = half_amount + level.random().next_int_bounded(half_amount);
                } else {
                    amount = 0;
                }
                ExperienceOrb::award(level, &Vec3::at_center_of(&pos), amount);
                level.level_event(None, 1042, pos, 0);
            }
        }
        env.locals[REPAIR_LOCAL].set_item(0, ItemStack::empty());
        env.locals[REPAIR_LOCAL].set_item(1, ItemStack::empty());
    }
}

/// `ItemStack.transmuteCopy(newItem)`.
fn transmute_copy(stack: &ItemStack, new_item: Item) -> ItemStack {
    if stack.is_empty() { ItemStack::empty() } else { ItemStack::with_patch(new_item, stack.count(), &stack.get_components_patch()) }
}

impl GrindstoneMenu {
    /// `GrindstoneMenu(containerId, inventory, access)`.
    pub fn new(container_id: i32, _player: &mut dyn MenuPlayer, access: ContainerLevelAccess) -> Self {
        let mut base = MenuBase::new(Some(MenuType::GRINDSTONE), container_id);
        let result = base.add_local(Box::new(ResultContainer::new()));
        let repair = base.add_local(Box::new(MenuSimpleContainer::new(2)));
        debug_assert_eq!((result, repair), (RESULT_LOCAL, REPAIR_LOCAL));
        base.add_slot(Slot::with_behavior(ContainerKey::Local(REPAIR_LOCAL), 0, 49, 19, Box::new(GrindstoneInputSlot)));
        base.add_slot(Slot::with_behavior(ContainerKey::Local(REPAIR_LOCAL), 1, 49, 40, Box::new(GrindstoneInputSlot)));
        base.add_slot(Slot::with_behavior(ContainerKey::Local(RESULT_LOCAL), 2, 129, 34, Box::new(GrindstoneResultSlot { access })));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self { base, access }
    }

    fn repair_stack(&self, slot: usize) -> ItemStack {
        self.base.locals[REPAIR_LOCAL].get_item(slot).clone()
    }

    /// `createResult()`.
    fn create_result(&mut self, player: &mut dyn MenuPlayer) {
        let registries = player.registries();
        let result = self.compute_result(&registries, &self.repair_stack(0), &self.repair_stack(1));
        self.base.locals[RESULT_LOCAL].set_item(0, result);
        self.broadcast_changes(player);
    }

    fn compute_result(&self, registries: &RegistryAccess, input: &ItemStack, additional: &ItemStack) -> ItemStack {
        let has_an_item = !input.is_empty() || !additional.is_empty();
        if !has_an_item {
            ItemStack::empty()
        } else if input.count() <= 1 && additional.count() <= 1 {
            let has_both_items = !input.is_empty() && !additional.is_empty();
            if !has_both_items {
                let item = if !input.is_empty() { input } else { additional };
                if !has_any_enchantments(item) { ItemStack::empty() } else { Self::remove_non_curses_from(registries, item.copy()) }
            } else {
                Self::merge_items(registries, input, additional)
            }
        } else {
            ItemStack::empty()
        }
    }

    fn merge_items(registries: &RegistryAccess, input: &ItemStack, additional: &ItemStack) -> ItemStack {
        if !input.is(additional.item()) {
            return ItemStack::empty();
        }
        let durability = input.get_max_damage().max(additional.get_max_damage());
        let remaining1 = input.get_max_damage() - input.get_damage_value();
        let remaining2 = additional.get_max_damage() - additional.get_damage_value();
        let remaining = remaining1 + remaining2 + durability * 5 / 100;
        let mut count = 1;
        if !input.is_damageable_item() {
            if input.get_max_stack_size() < 2 || !ItemStack::matches(input, additional) {
                return ItemStack::empty();
            }
            count = 2;
        }
        let mut new_item = input.copy_with_count(count);
        if new_item.is_damageable_item() {
            new_item.set(MAX_DAMAGE, durability);
            new_item.set_damage_value((durability - remaining).max(0));
        }
        Self::merge_enchants_from(registries, &mut new_item, additional);
        Self::remove_non_curses_from(registries, new_item)
    }

    fn merge_enchants_from(registries: &RegistryAccess, target: &mut ItemStack, source: &ItemStack) {
        let curse = enchantment_tags::id(enchantment_tags::CURSE);
        update_enchantments(target, |new_enchantments| {
            for (enchant, level) in get_enchantments_for_crafting(source).entries() {
                let is_curse = enchant.name().is_some_and(|k| registries.is_in_tag(ENCHANTMENT_REGISTRY, k, &curse));
                if !is_curse || new_enchantments.get_level(enchant) == 0 {
                    new_enchantments.upgrade(enchant, *level);
                }
            }
        });
    }

    fn remove_non_curses_from(registries: &RegistryAccess, mut item: ItemStack) -> ItemStack {
        let curse = enchantment_tags::id(enchantment_tags::CURSE);
        let new_enchantments = update_enchantments(&mut item, |e| {
            e.remove_if(|enchantment| !enchantment.name().is_some_and(|k| registries.is_in_tag(ENCHANTMENT_REGISTRY, k, &curse)));
        });
        if item.is(Item::ENCHANTED_BOOK) && new_enchantments.is_empty() {
            item = transmute_copy(&item, Item::BOOK);
        }
        let mut repair_cost = 0;
        for _ in 0..new_enchantments.len() {
            repair_cost = calculate_increased_repair_cost(repair_cost);
        }
        item.set(REPAIR_COST, repair_cost);
        item
    }
}

impl AbstractContainerMenu for GrindstoneMenu {
    fn base(&self) -> &MenuBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MenuBase {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        self.broadcast_changes(player);
        if matches!(container, ContainerKey::Local(REPAIR_LOCAL)) {
            self.create_result(player);
        }
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if self.access.pos().is_some() {
            self.clear_container(player, REPAIR_LOCAL);
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        super::still_valid(&self.access, player, Block::GRINDSTONE)
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        if !self.base.with_slot(player, slot_index, |s, e| s.has_item(e)) {
            return ItemStack::empty();
        }
        let mut item = self.base.with_slot(player, slot_index, |s, e| s.get_item(e));
        let clicked = item.copy();
        let input = self.repair_stack(0);
        let additional = self.repair_stack(1);
        if slot_index == RESULT_SLOT {
            if !self.move_item_stack_to(player, &mut item, INV_SLOT_START, USE_ROW_SLOT_END, true) {
                return ItemStack::empty();
            }
            self.base.with_slot(player, slot_index, |s, e| s.on_quick_craft(e, &item, &clicked));
        } else if slot_index != 0 && slot_index != 1 {
            if !input.is_empty() && !additional.is_empty() {
                if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                    if !self.move_item_stack_to(player, &mut item, USE_ROW_SLOT_START, USE_ROW_SLOT_END, false) {
                        return ItemStack::empty();
                    }
                } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index) && !self.move_item_stack_to(player, &mut item, INV_SLOT_START, INV_SLOT_END, false) {
                    return ItemStack::empty();
                }
            } else if !self.move_item_stack_to(player, &mut item, 0, 2, false) {
                return ItemStack::empty();
            }
        } else if !self.move_item_stack_to(player, &mut item, INV_SLOT_START, USE_ROW_SLOT_END, false) {
            return ItemStack::empty();
        }
        finish_quick_move(self, player, slot_index, item, &clicked)
    }
}
