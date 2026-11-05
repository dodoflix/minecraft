//! `EnchantmentMenu` (the enchanting table): seeded 3-option costs/clues, lapis + experience level payment.
//!
//! Also hosts `EnchantingTableBlock.BOOKSHELF_OFFSETS` / `isValidBookShelf` (the power providers around the table).

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, DataSlot, MenuBase, MenuPlayer, MenuSimpleContainer, Slot, SlotBehavior, SlotEnv,
    finish_quick_move,
};
use crate::registry::RegistryAccess;
use crate::registry::keys::ENCHANTMENT as ENCHANTMENT_REGISTRY;
use crate::world::container::Container;
use crate::world::item::ItemStack;
use crate::world::item::enchantment::enchantment_instance::EnchantmentInstance;
use crate::world::item::enchantment::enchantment_tags;
use crate::world::item::enchantment::item_enchantments::enchant;
use crate::world::item::enchantment_helper::{get_enchantment_cost, select_enchantment};
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::{Block, Item, MenuType, SoundEvent};
use mc_protocol::components::{ENCHANTABLE, ENCHANTMENTS};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

/// `EnchantingTableBlock.BOOKSHELF_OFFSETS`: `betweenClosedStream(-2, 0, -2, 2, 1, 2)` restricted to the outer ring.
pub static BOOKSHELF_OFFSETS: LazyLock<Vec<BlockPos>> = LazyLock::new(|| {
    BlockPos::between_closed_xyz(-2, 0, -2, 2, 1, 2).filter(|p| p.x().abs() == 2 || p.z().abs() == 2).collect()
});

/// `EnchantingTableBlock.isValidBookShelf(level, pos, offset)`.
pub fn is_valid_book_shelf(level: &dyn BlockGetter, pos: BlockPos, offset: BlockPos) -> bool {
    level.get_block_state(pos.offset(offset.x(), offset.y(), offset.z())).is_tag("minecraft:enchantment_power_provider")
        && level.get_block_state(pos.offset(offset.x() / 2, offset.y(), offset.z() / 2)).is_tag("minecraft:enchantment_power_transmitter")
}

/// `ItemStack.isEnchantable()`: has `enchantable` and an empty `enchantments` component.
pub fn is_enchantable(stack: &ItemStack) -> bool {
    stack.get(&ENCHANTABLE).is_some() && stack.get(&ENCHANTMENTS).is_some_and(|e| e.is_empty())
}

const ENCHANT_LOCAL: usize = 0;

/// Slot 0 of the table holds one item.
struct EnchantItemSlot;
impl SlotBehavior for EnchantItemSlot {
    fn get_max_stack_size(&self, _slot: &Slot, _env: &mut SlotEnv) -> i32 {
        1
    }
}

/// Slot 1 takes lapis lazuli only.
struct LapisSlot;
impl SlotBehavior for LapisSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        stack.is(Item::LAPIS_LAZULI)
    }
}

pub struct EnchantmentMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
    random: LegacyRandomSource,
    enchantment_seed: Rc<RefCell<Vec<i32>>>,
    pub costs: Rc<RefCell<Vec<i32>>>,
    pub enchant_clue: Rc<RefCell<Vec<i32>>>,
    pub level_clue: Rc<RefCell<Vec<i32>>>,
}

impl EnchantmentMenu {
    /// `EnchantmentMenu(containerId, inventory, access)`.
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, access: ContainerLevelAccess) -> Self {
        let mut base = MenuBase::new(Some(MenuType::ENCHANTMENT), container_id);
        let local = base.add_local(Box::new(MenuSimpleContainer::new(2)));
        debug_assert_eq!(local, ENCHANT_LOCAL);
        base.add_slot(Slot::with_behavior(ContainerKey::Local(ENCHANT_LOCAL), 0, 15, 47, Box::new(EnchantItemSlot)));
        base.add_slot(Slot::with_behavior(ContainerKey::Local(ENCHANT_LOCAL), 1, 35, 47, Box::new(LapisSlot)));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        let costs = Rc::new(RefCell::new(vec![0; 3]));
        let enchant_clue = Rc::new(RefCell::new(vec![-1; 3]));
        let level_clue = Rc::new(RefCell::new(vec![-1; 3]));
        let enchantment_seed = Rc::new(RefCell::new(vec![player.enchantment_seed()]));
        for i in 0..3 {
            base.add_data_slot(DataSlot::shared(costs.clone(), i));
        }
        base.add_data_slot(DataSlot::shared(enchantment_seed.clone(), 0));
        for i in 0..3 {
            base.add_data_slot(DataSlot::shared(enchant_clue.clone(), i));
        }
        for i in 0..3 {
            base.add_data_slot(DataSlot::shared(level_clue.clone(), i));
        }
        Self { base, access, random: LegacyRandomSource::new(0), enchantment_seed, costs, enchant_clue, level_clue }
    }

    fn seed(&self) -> i32 {
        self.enchantment_seed.borrow()[0]
    }

    /// `getEnchantmentList(access, itemStack, slot, enchantmentCost)`.
    fn get_enchantment_list(&mut self, registries: &RegistryAccess, item: &ItemStack, slot: i32, enchantment_cost: i32) -> Vec<EnchantmentInstance> {
        self.random.set_seed(i64::from(self.seed().wrapping_add(slot)));
        let Some(registry) = registries.registry(ENCHANTMENT_REGISTRY) else { return Vec::new() };
        let tag = enchantment_tags::id(enchantment_tags::IN_ENCHANTING_TABLE);
        let Some(ids) = registry.tag(&tag) else { return Vec::new() };
        let source: Vec<_> = ids.iter().filter_map(|id| registry.name_of(*id)).collect();
        let mut list = select_enchantment(&mut self.random, item, enchantment_cost, registries, &source);
        if item.is(Item::BOOK) && list.len() > 1 {
            let at = self.random.next_int_bounded(list.len() as i32) as usize;
            list.remove(at);
        }
        list
    }

    /// `getGoldCount()`.
    pub fn get_gold_count(&self) -> i32 {
        let stack = self.base.locals[ENCHANT_LOCAL].get_item(1);
        if stack.is_empty() { 0 } else { stack.count() }
    }

    /// `getEnchantmentSeed()`.
    pub fn get_enchantment_seed(&self) -> i32 {
        self.seed()
    }

    fn reset_options(&self) {
        for i in 0..3 {
            self.costs.borrow_mut()[i] = 0;
            self.enchant_clue.borrow_mut()[i] = -1;
            self.level_clue.borrow_mut()[i] = -1;
        }
    }
}

impl AbstractContainerMenu for EnchantmentMenu {
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

    /// `slotsChanged(container)`: only the table's own container recomputes (no `super.slotsChanged`).
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        if !matches!(container, ContainerKey::Local(ENCHANT_LOCAL)) {
            return;
        }
        let item = self.base.locals[ENCHANT_LOCAL].get_item(0).clone();
        if !item.is_empty() && is_enchantable(&item) {
            let Some(pos) = self.access.pos() else { return };
            let Some(level) = player.level_ref() else { return };
            let registries = level.registries.clone();
            let mut bookcases = 0;
            for offset in BOOKSHELF_OFFSETS.iter() {
                if is_valid_book_shelf(level, pos, *offset) {
                    bookcases += 1;
                }
            }
            self.random.set_seed(i64::from(self.seed()));
            for i in 0..3usize {
                let cost = get_enchantment_cost(&mut self.random, i as i32, bookcases, &item);
                self.costs.borrow_mut()[i] = cost;
                self.enchant_clue.borrow_mut()[i] = -1;
                self.level_clue.borrow_mut()[i] = -1;
                if cost < i as i32 + 1 {
                    self.costs.borrow_mut()[i] = 0;
                }
            }
            for i in 0..3usize {
                let cost = self.costs.borrow()[i];
                if cost > 0 {
                    let list = self.get_enchantment_list(&registries, &item, i as i32, cost);
                    if !list.is_empty() {
                        let ench = list[self.random.next_int_bounded(list.len() as i32) as usize].clone();
                        let id = registries.registry(ENCHANTMENT_REGISTRY).and_then(|r| r.id_of(&ench.enchantment)).map_or(-1, |id| id as i32);
                        self.enchant_clue.borrow_mut()[i] = id;
                        self.level_clue.borrow_mut()[i] = ench.level;
                    }
                }
            }
            self.base.broadcast_changes(player);
        } else {
            self.reset_options();
        }
    }

    fn click_menu_button(&mut self, player: &mut dyn MenuPlayer, button_id: i32) -> bool {
        if !(0..3).contains(&button_id) {
            // `Util.logAndPauseIfInIde(name + " pressed invalid button id: " + buttonId)`
            return false;
        }
        let button = button_id as usize;
        let item = self.base.locals[ENCHANT_LOCAL].get_item(0).clone();
        let currency = self.base.locals[ENCHANT_LOCAL].get_item(1).clone();
        let enchantment_cost = button_id + 1;
        let infinite = player.has_infinite_materials();
        if (currency.is_empty() || currency.count() < enchantment_cost) && !infinite {
            return false;
        }
        let cost = self.costs.borrow()[button];
        let level = player.experience_level();
        if cost <= 0 || item.is_empty() || ((level < enchantment_cost || level < cost) && !infinite) {
            return false;
        }
        let Some(pos) = self.access.pos() else { return true };
        let Some(registries) = player.level_ref().map(|l| l.registries.clone()) else { return true };
        let new_enchantment = self.get_enchantment_list(&registries, &item, button_id, cost);
        if !new_enchantment.is_empty() {
            player.on_enchantment_performed(&item, enchantment_cost);
            let mut enchantment_item = item.clone();
            if item.is(Item::BOOK) {
                enchantment_item = ItemStack::with_patch(Item::ENCHANTED_BOOK, item.count(), &item.get_components_patch());
                self.base.locals[ENCHANT_LOCAL].set_item(0, enchantment_item.clone());
                self.flush_container_events(player);
            }
            for enchantment in &new_enchantment {
                enchant(&mut enchantment_item, &enchantment.enchantment, enchantment.level);
            }
            // `enchantmentItem` is the live stack of slot 0 (the original stack, or the book that replaced it)
            *self.base.locals[ENCHANT_LOCAL].get_item_mut(0).expect("slot 0") = enchantment_item.clone();
            let mut currency = currency;
            if !infinite {
                currency.shrink(enchantment_cost);
            }
            *self.base.locals[ENCHANT_LOCAL].get_item_mut(1).expect("slot 1") = currency.clone();
            if currency.is_empty() {
                self.base.locals[ENCHANT_LOCAL].set_item(1, ItemStack::empty());
                self.flush_container_events(player);
            }
            player.on_item_enchanted(&enchantment_item, enchantment_cost);
            self.base.locals[ENCHANT_LOCAL].set_changed();
            self.flush_container_events(player);
            self.enchantment_seed.borrow_mut()[0] = player.enchantment_seed();
            self.slots_changed(player, &ContainerKey::Local(ENCHANT_LOCAL));
            if let Some(level) = player.level() {
                let pitch = level.random().next_float() * 0.1 + 0.9;
                level.play_sound(None, pos, SoundEvent::BLOCK_ENCHANTMENT_TABLE_USE, SoundSource::Blocks, 1.0, pitch);
            }
        }
        true
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if self.access.pos().is_some() {
            self.clear_container(player, ENCHANT_LOCAL);
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        super::still_valid(&self.access, player, Block::ENCHANTING_TABLE)
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        if !self.base.with_slot(player, slot_index, |s, e| s.has_item(e)) {
            return ItemStack::empty();
        }
        let mut stack = self.base.with_slot(player, slot_index, |s, e| s.get_item(e));
        let clicked = stack.copy();
        if slot_index == 0 || slot_index == 1 {
            if !self.move_item_stack_to(player, &mut stack, 2, 38, true) {
                return ItemStack::empty();
            }
        } else if stack.is(Item::LAPIS_LAZULI) {
            if !self.move_item_stack_to(player, &mut stack, 1, 2, true) {
                return ItemStack::empty();
            }
        } else {
            let (has, may_place) = self.base.with_slot(player, 0, |s, e| (s.has_item(e), s.may_place(e, &stack)));
            if has || !may_place {
                return ItemStack::empty();
            }
            let single_item = stack.copy_with_count(1);
            stack.shrink(1);
            self.base.with_slot(player, 0, |s, e| s.set_by_player(e, single_item));
        }
        finish_quick_move(self, player, slot_index, stack, &clicked)
    }
}
