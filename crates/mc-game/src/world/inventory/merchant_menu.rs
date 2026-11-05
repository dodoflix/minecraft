//! `MerchantMenu`, `MerchantContainer`, `MerchantResultSlot`: the trading screen. The merchant is an entity reached
//! through [`Merchant`] (see `world::item::trading::merchant`); offers are addressed by index into its offer list.
//!
//! `MerchantContainer.updateSellItem()` needs the merchant, which a plain `Container` cannot reach: the container counts
//! the vanilla call sites (`setItem` of a payment slot, `removeItem` that removed something from a payment slot,
//! `setChanged`) and the menu replays them as `update_sell_item` after each slot operation (`flush_container_events`).

use super::{AbstractContainerMenu, ContainerKey, MenuBase, MenuPlayer, Prediction, Slot, SlotBehavior, SlotEnv, place_item_back_in_inventory};
use crate::stats::{Stat, Stats};
use crate::world::container::{Container, helper};
use crate::world::entity::EntityId;
use crate::world::item::trading::{ItemCost, MerchantOffers, with_merchant};
use crate::world::item::{ItemStack, behaviour};
use mc_data::MenuType;
use std::any::Any;
use std::cell::Cell;

const PAYMENT1_SLOT: usize = 0;
const PAYMENT2_SLOT: usize = 1;
const RESULT_SLOT: usize = 2;
const INV_SLOT_START: usize = 3;
const INV_SLOT_END: usize = 30;
const USE_ROW_SLOT_START: usize = 30;
const USE_ROW_SLOT_END: usize = 39;
const TRADE_LOCAL: usize = 0;

/// `MerchantContainer` (3 slots: two payments and the result) minus the merchant, which the menu supplies.
pub struct MerchantContainer {
    items: [ItemStack; 3],
    /// `activeOffer` as an index into the merchant's offers.
    active_offer: Option<usize>,
    selection_hint: i32,
    future_xp: i32,
    pending_updates: u32,
}

impl MerchantContainer {
    fn new() -> Self {
        Self { items: [ItemStack::empty(), ItemStack::empty(), ItemStack::empty()], active_offer: None, selection_hint: 0, future_xp: 0, pending_updates: 0 }
    }

    fn is_payment_slot(slot: usize) -> bool {
        slot == 0 || slot == 1
    }

    /// `getActiveOffer()` (index).
    pub fn get_active_offer(&self) -> Option<usize> {
        self.active_offer
    }

    /// `getFutureXp()`.
    pub fn get_future_xp(&self) -> i32 {
        self.future_xp
    }
}

impl Container for MerchantContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        3
    }
    fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.items.get(slot).unwrap_or_else(|| crate::world::container::empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.items.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        if slot == RESULT_SLOT && !self.items[slot].is_empty() {
            let all = self.items[slot].count();
            return helper::remove_item(&mut self.items, slot, all);
        }
        let result = helper::remove_item(&mut self.items, slot, count);
        if !result.is_empty() && Self::is_payment_slot(slot) {
            self.pending_updates += 1;
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.items, slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        self.items[slot] = stack;
        if Self::is_payment_slot(slot) {
            self.pending_updates += 1;
        }
    }
    fn clear_content(&mut self) {
        self.items = [ItemStack::empty(), ItemStack::empty(), ItemStack::empty()];
    }
    fn set_changed(&mut self) {
        self.pending_updates += 1;
    }
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn take_menu_notifications(&mut self) -> u32 {
        std::mem::take(&mut self.pending_updates)
    }
}

fn trade_container<'a>(locals: &'a mut [Box<dyn Container>]) -> &'a mut MerchantContainer {
    locals[TRADE_LOCAL].as_any_mut().downcast_mut::<MerchantContainer>().expect("merchant container")
}

/// `MerchantContainer.getRecipeFor` with the index of the offer.
fn recipe_index_for(offers: &MerchantOffers, buy_a: &ItemStack, buy_b: &ItemStack, selection_hint: i32) -> Option<usize> {
    if selection_hint > 0 && (selection_hint as usize) < offers.len() {
        return offers[selection_hint as usize].satisfied_by(buy_a, buy_b).then_some(selection_hint as usize);
    }
    offers.iter().position(|o| o.satisfied_by(buy_a, buy_b))
}

/// `MerchantResultSlot`.
struct MerchantResultSlot {
    merchant: EntityId,
    remove_count: Cell<i32>,
}

impl MerchantResultSlot {
    /// `checkTakeAchievements(carried)`: `carried.onCraftedBy(player, removeCount)`.
    fn check_take_achievements(&self, env: &mut SlotEnv, carried: &ItemStack) {
        let count = self.remove_count.get();
        let item = carried.item();
        env.player.with_server_player(&mut |level, p| p.award_stat(&*level, Stat::Crafted(item), count));
        let mut copy = carried.clone();
        behaviour(item).on_crafted_by(&mut copy, &mut *env.player);
        self.remove_count.set(0);
    }
}

impl SlotBehavior for MerchantResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn remove(&self, slot: &Slot, env: &mut SlotEnv, amount: i32) -> ItemStack {
        let current = slot.get_item(env);
        if !current.is_empty() {
            self.remove_count.set(self.remove_count.get() + amount.min(current.count()));
        }
        slot.base_remove(env, amount)
    }

    fn on_quick_craft(&self, _slot: &Slot, env: &mut SlotEnv, picked: &ItemStack, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
        self.check_take_achievements(env, picked);
    }

    fn may_pickup(&self, _slot: &Slot, env: &mut SlotEnv) -> bool {
        let container = trade_container(env.locals);
        let Some(index) = container.active_offer else { return false };
        let (buy_a, buy_b) = (container.items[0].clone(), container.items[1].clone());
        with_merchant(&mut *env.player, self.merchant, |level, m| {
            m.get_offers(level).get(index).is_some_and(|o| o.satisfied_by(&buy_a, &buy_b) || o.satisfied_by(&buy_b, &buy_a))
        })
        .unwrap_or(false)
    }

    fn on_take(&self, _slot: &Slot, env: &mut SlotEnv, carried: &ItemStack) {
        self.check_take_achievements(env, carried);
        let container = trade_container(env.locals);
        let Some(index) = container.active_offer else { return };
        let (mut buy_a, mut buy_b) = (container.items[0].clone(), container.items[1].clone());
        let taken = with_merchant(&mut *env.player, self.merchant, |level, m| {
            let offer = m.get_offers(level).get(index)?;
            let taken = offer.take(&mut buy_a, &mut buy_b) || offer.take(&mut buy_b, &mut buy_a);
            let xp = offer.get_xp();
            if taken {
                m.notify_trade(level, index);
            }
            Some((taken, xp))
        })
        .flatten();
        let Some((taken, xp)) = taken else { return };
        if taken {
            env.player.with_server_player(&mut |level, p| p.award_stat_once(&*level, Stats::TRADED_WITH_VILLAGER));
            let container = trade_container(env.locals);
            container.set_item(0, buy_a);
            container.set_item(1, buy_b);
        }
        with_merchant(&mut *env.player, self.merchant, |_, m| {
            let total = m.get_villager_xp() + xp;
            m.override_xp(total);
        });
    }
}

pub struct MerchantMenu {
    base: MenuBase,
    trader: EntityId,
    merchant_level: i32,
    show_progress_bar: bool,
    can_restock: bool,
}

impl MerchantMenu {
    /// `MerchantMenu(containerId, inventory, merchant)`.
    pub fn new(container_id: i32, merchant: EntityId) -> Self {
        let mut base = MenuBase::new(Some(MenuType::MERCHANT), container_id);
        let local = base.add_local(Box::new(MerchantContainer::new()));
        debug_assert_eq!(local, TRADE_LOCAL);
        base.add_slot(Slot::new(ContainerKey::Local(TRADE_LOCAL), PAYMENT1_SLOT, 136, 37));
        base.add_slot(Slot::new(ContainerKey::Local(TRADE_LOCAL), PAYMENT2_SLOT, 162, 37));
        base.add_slot(Slot::with_behavior(ContainerKey::Local(TRADE_LOCAL), RESULT_SLOT, 220, 37, Box::new(MerchantResultSlot { merchant, remove_count: Cell::new(0) })));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 108, 84);
        Self { base, trader: merchant, merchant_level: 0, show_progress_bar: false, can_restock: false }
    }

    pub fn get_trader(&self) -> EntityId {
        self.trader
    }

    fn container(&mut self) -> &mut MerchantContainer {
        trade_container(&mut self.base.locals)
    }

    /// `MerchantContainer.updateSellItem()`.
    pub fn update_sell_item(&mut self, player: &mut dyn MenuPlayer) {
        let trader = self.trader;
        let container = self.container();
        container.active_offer = None;
        let (buy_a, buy_b) = if container.items[0].is_empty() {
            (container.items[1].clone(), ItemStack::empty())
        } else {
            (container.items[0].clone(), container.items[1].clone())
        };
        if buy_a.is_empty() {
            container.set_item(RESULT_SLOT, ItemStack::empty());
            container.future_xp = 0;
            return;
        }
        let hint = container.selection_hint;
        let outcome = with_merchant(&mut *player, trader, |level, m| {
            let offers = m.get_offers(level);
            let mut active = None;
            let mut result = None;
            if !offers.is_empty() {
                let mut found = recipe_index_for(offers, &buy_a, &buy_b, hint);
                if found.is_none_or(|i| offers[i].is_out_of_stock()) {
                    active = found;
                    found = recipe_index_for(offers, &buy_b, &buy_a, hint);
                }
                match found {
                    Some(i) if !offers[i].is_out_of_stock() => {
                        active = Some(i);
                        result = Some((offers[i].assemble(), offers[i].get_xp()));
                    }
                    _ => {}
                }
            }
            (offers.is_empty(), active, result)
        });
        let Some((offers_empty, active, result)) = outcome else { return };
        let container = self.container();
        if !offers_empty {
            container.active_offer = active;
            match result {
                Some((stack, xp)) => {
                    container.set_item(RESULT_SLOT, stack);
                    container.future_xp = xp;
                }
                None => {
                    container.set_item(RESULT_SLOT, ItemStack::empty());
                    container.future_xp = 0;
                }
            }
        }
        let result_item = container.items[RESULT_SLOT].clone();
        with_merchant(&mut *player, trader, |level, m| m.notify_trade_updated(level, &result_item));
    }

    /// `setSelectionHint(hint)`.
    pub fn set_selection_hint(&mut self, player: &mut dyn MenuPlayer, hint: i32) {
        self.container().selection_hint = hint;
        self.update_sell_item(player);
        self.flush_container_events(player);
    }

    pub fn get_future_trader_xp(&mut self) -> i32 {
        self.container().future_xp
    }

    pub fn set_show_progress_bar(&mut self, show: bool) {
        self.show_progress_bar = show;
    }

    pub fn show_progress_bar(&self) -> bool {
        self.show_progress_bar
    }

    pub fn get_trader_level(&self) -> i32 {
        self.merchant_level
    }

    pub fn set_merchant_level(&mut self, level: i32) {
        self.merchant_level = level;
    }

    pub fn set_can_restock(&mut self, can_restock: bool) {
        self.can_restock = can_restock;
    }

    pub fn can_restock(&self) -> bool {
        self.can_restock
    }

    /// `getTraderXp()`.
    pub fn get_trader_xp(&self, player: &mut dyn MenuPlayer) -> i32 {
        with_merchant(player, self.trader, |_, m| m.get_villager_xp()).unwrap_or(0)
    }

    /// `tryMoveItems(newTradeIndex)` (`ServerboundSelectTradePacket`).
    pub fn try_move_items(&mut self, player: &mut dyn MenuPlayer, new_trade_index: i32) {
        let trader = self.trader;
        let offer = with_merchant(&mut *player, trader, |level, m| {
            let offers = m.get_offers(level);
            (new_trade_index >= 0 && offers.len() > new_trade_index as usize).then(|| offers[new_trade_index as usize].clone())
        })
        .flatten();
        let Some(offer) = offer else { return };
        for payment in [PAYMENT1_SLOT, PAYMENT2_SLOT] {
            let mut old_cost = self.container().items[payment].clone();
            if !old_cost.is_empty() {
                if !self.move_item_stack_to(player, &mut old_cost, INV_SLOT_START, USE_ROW_SLOT_END, true) {
                    return;
                }
                self.container().set_item(payment, old_cost);
                self.flush_container_events(player);
            }
        }
        let both_empty = {
            let c = self.container();
            c.items[0].is_empty() && c.items[1].is_empty()
        };
        if both_empty {
            self.move_from_inventory_to_payment_slot(player, 0, offer.get_item_cost_a());
            if let Some(cost_b) = offer.get_item_cost_b() {
                self.move_from_inventory_to_payment_slot(player, 1, cost_b);
            }
        }
    }

    fn move_from_inventory_to_payment_slot(&mut self, player: &mut dyn MenuPlayer, payment_slot: usize, cost: &ItemCost) {
        for i in INV_SLOT_START..USE_ROW_SLOT_END {
            let inventory_item = self.base.with_slot(player, i, |s, e| s.get_item(e));
            if inventory_item.is_empty() || !cost.test(&inventory_item) {
                continue;
            }
            let current = self.container().items[payment_slot].clone();
            if current.is_empty() || ItemStack::is_same_item_same_components(&inventory_item, &current) {
                let max_stack_size = inventory_item.get_max_stack_size();
                let move_count = (max_stack_size - current.count()).min(inventory_item.count());
                let new_payment = inventory_item.copy_with_count(current.count() + move_count);
                self.base.with_slot(player, i, |s, e| s.with_item_mut(e, |st| st.shrink(move_count)));
                let full = new_payment.count() >= max_stack_size;
                self.container().set_item(payment_slot, new_payment);
                self.flush_container_events(player);
                if full {
                    break;
                }
            }
        }
    }
}

impl AbstractContainerMenu for MerchantMenu {
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

    /// The container notifications are `MerchantContainer.updateSellItem()` calls; a `slotsChanged` of anything else
    /// (`MerchantMenu.slotsChanged`) additionally broadcasts.
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        self.update_sell_item(player);
        if !matches!(container, ContainerKey::Local(_)) {
            self.broadcast_changes(player);
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        use crate::world::entity::Entity;
        let me = player.entity_id();
        player.level_ref().and_then(|l| l.get_entity(self.trader)).and_then(|e| e.as_merchant()).is_some_and(|m| m.still_valid(me))
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, _target: &Slot) -> bool {
        false
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        with_merchant(&mut *player, self.trader, |_, m| m.set_trading_player(None));
        if player.is_server_player() {
            let dead = !player.is_alive() || player.has_disconnected();
            for slot in [PAYMENT1_SLOT, PAYMENT2_SLOT] {
                let mut stack = self.container().remove_item_no_update(slot);
                if dead {
                    if !stack.is_empty() {
                        player.drop(stack, false, Prediction::ServerOnly);
                    }
                } else {
                    place_item_back_in_inventory(player, &mut stack, true, Prediction::ServerOnly);
                }
            }
        }
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let mut clicked = ItemStack::empty();
        if !self.base.with_slot(player, slot_index, |s, e| s.has_item(e)) {
            return clicked;
        }
        let mut stack = self.base.with_slot(player, slot_index, |s, e| s.get_item(e));
        clicked = stack.copy();
        let moved = if slot_index == RESULT_SLOT {
            let moved = self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true);
            if moved {
                self.base.with_slot(player, RESULT_SLOT, |s, e| s.on_quick_craft(e, &stack, &clicked));
                // `playTradeSound`: `Level.playLocalSound` is a no-op on the server.
            }
            moved
        } else if slot_index != PAYMENT1_SLOT && slot_index != PAYMENT2_SLOT {
            if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, USE_ROW_SLOT_START, USE_ROW_SLOT_END, false)
            } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, INV_SLOT_START, INV_SLOT_END, false)
            } else {
                true
            }
        } else {
            self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, false)
        };
        if !moved {
            return ItemStack::empty();
        }
        if stack.is_empty() {
            self.base.with_slot(player, slot_index, |s, e| s.set_by_player_with(e, ItemStack::empty(), &clicked));
        } else {
            let rest = stack.clone();
            self.base.with_slot(player, slot_index, |s, e| {
                s.with_item_mut(e, |t| *t = rest);
                s.set_changed(e);
            });
        }
        if stack.count() == clicked.count() {
            return ItemStack::empty();
        }
        self.base.with_slot(player, slot_index, |s, e| s.on_take(e, &stack));
        self.flush_container_events(player);
        clicked
    }
}
