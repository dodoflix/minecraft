//! `CartographyTableMenu`: lock a map with a glass pane, extend it with paper, copy it with an empty map.

use super::item_combiner_menu::CombinerResultContainer;
use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, MenuBase, MenuPlayer,
    MenuSimpleContainer, Slot, SlotBehavior, SlotEnv,
};
use crate::world::item::{ItemStack, behaviour, map_item};
use mc_data::{Block, Item, MenuType, SoundEvent};
use mc_protocol::components::{MAP_ID, MAP_POST_PROCESSING, MapPostProcessing};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;
use std::cell::Cell;

pub const MAP_SLOT: usize = 0;
pub const ADDITIONAL_SLOT: usize = 1;
pub const RESULT_SLOT: usize = 2;
const INV_SLOT_START: usize = 3;
const INV_SLOT_END: usize = 30;
const USE_ROW_SLOT_START: usize = 30;
const USE_ROW_SLOT_END: usize = 39;

/// Menu-local containers.
const CONTAINER_LOCAL: usize = 0;
const RESULT_LOCAL: usize = 1;

/// The map slot: `mayPlace` = has a `map_id`.
struct MapSlot;

impl SlotBehavior for MapSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        stack.get(&MAP_ID).is_some()
    }
}

/// `itemStack.is(PAPER) || itemStack.is(MAP) || itemStack.is(GLASS_PANE)`.
fn is_additional_item(stack: &ItemStack) -> bool {
    stack.is(Item::PAPER) || stack.is(Item::MAP) || stack.is(Item::GLASS_PANE)
}

struct AdditionalSlot;

impl SlotBehavior for AdditionalSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        is_additional_item(stack)
    }
}

/// `result.getItem().onCraftedBy(result, player)`.
fn on_crafted_by(stack: &mut ItemStack, player: &mut dyn MenuPlayer) {
    behaviour(stack.item()).on_crafted_by(stack, player);
}

/// The result slot: taking consumes one of each input, runs the map post-processing and plays the sound once per tick.
struct CartographyResultSlot {
    access: ContainerLevelAccess,
    last_sound_time: Cell<i64>,
}

impl SlotBehavior for CartographyResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, carried: &mut ItemStack) {
        env.locals[CONTAINER_LOCAL].remove_item(MAP_SLOT, 1);
        env.locals[CONTAINER_LOCAL].remove_item(ADDITIONAL_SLOT, 1);
        on_crafted_by(carried, &mut *env.player);
        if let Some(pos) = self.access.pos()
            && let Some(level) = env.player.level()
        {
            let game_time = level.game_time();
            if self.last_sound_time.get() != game_time {
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::UI_CARTOGRAPHY_TABLE_TAKE_RESULT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                self.last_sound_time.set(game_time);
            }
        }
        slot.set_changed(env);
    }

    fn safe_clone(&self, slot: &Slot, env: &mut SlotEnv) -> ItemStack {
        let mut result = slot.base_safe_clone(env);
        on_crafted_by(&mut result, &mut *env.player);
        result
    }
}

pub struct CartographyTableMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
}

impl CartographyTableMenu {
    /// `CartographyTableMenu(containerId, inventory, access)`.
    pub fn new(
        container_id: i32,
        _player: &mut dyn MenuPlayer,
        access: ContainerLevelAccess,
    ) -> Self {
        let mut base = MenuBase::new(Some(MenuType::CARTOGRAPHY_TABLE), container_id);
        let container = base.add_local(Box::new(MenuSimpleContainer::new(2)));
        let result = base.add_local(Box::new(CombinerResultContainer::default()));
        debug_assert_eq!((container, result), (CONTAINER_LOCAL, RESULT_LOCAL));
        let inputs = ContainerKey::Local(CONTAINER_LOCAL);
        base.add_slot(Slot::with_behavior(
            inputs.clone(),
            0,
            15,
            15,
            Box::new(MapSlot),
        ));
        base.add_slot(Slot::with_behavior(
            inputs,
            1,
            15,
            52,
            Box::new(AdditionalSlot),
        ));
        base.add_slot(Slot::with_behavior(
            ContainerKey::Local(RESULT_LOCAL),
            2,
            145,
            39,
            Box::new(CartographyResultSlot {
                access,
                last_sound_time: Cell::new(0),
            }),
        ));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self { base, access }
    }

    fn input(&self, slot: usize) -> ItemStack {
        self.base.locals[CONTAINER_LOCAL].get_item(slot).clone()
    }

    /// `setupResultSlot(mapStack, additionalStack, resultStack)`.
    fn setup_result_slot(
        &mut self,
        player: &mut dyn MenuPlayer,
        map_stack: &ItemStack,
        additional_stack: &ItemStack,
        result_stack: &ItemStack,
    ) {
        if self.access.pos().is_none() {
            return;
        }
        // `MapItem.getSavedData` is null for a map the level does not know: nothing happens at all
        let Some((locked, scale)) = player
            .level()
            .and_then(|level| map_item::saved_data_info(map_stack, level))
        else {
            return;
        };
        let mut result;
        if additional_stack.is(Item::PAPER)
            && mc_data::tags::get::<Item>("extendable_maps")
                .is_some_and(|tag| tag.contains(map_stack.item()))
            && !locked
            && scale < 4
        {
            result = map_stack.copy_with_count(1);
            result.set(MAP_POST_PROCESSING, MapPostProcessing::Scale);
        } else if additional_stack.is(Item::GLASS_PANE) && !locked {
            result = map_stack.copy_with_count(1);
            result.set(MAP_POST_PROCESSING, MapPostProcessing::Lock);
        } else {
            if !additional_stack.is(Item::MAP) {
                self.base.locals[RESULT_LOCAL].remove_item_no_update(RESULT_SLOT);
                self.broadcast_changes(player);
                return;
            }
            result = map_stack.copy_with_count(2);
        }
        self.broadcast_changes(player);
        if !ItemStack::matches(&result, result_stack) {
            self.base.locals[RESULT_LOCAL].set_item(RESULT_SLOT, result);
            self.broadcast_changes(player);
        }
    }
}

impl AbstractContainerMenu for CartographyTableMenu {
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

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        super::still_valid(&self.access, player, Block::CARTOGRAPHY_TABLE)
    }

    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, _container: &ContainerKey) {
        let map_stack = self.input(MAP_SLOT);
        let additional_stack = self.input(ADDITIONAL_SLOT);
        let result_stack = self.base.locals[RESULT_LOCAL].get_item(RESULT_SLOT).clone();
        if result_stack.is_empty() || !map_stack.is_empty() && !additional_stack.is_empty() {
            if !map_stack.is_empty() && !additional_stack.is_empty() {
                self.setup_result_slot(player, &map_stack, &additional_stack, &result_stack);
            }
        } else {
            self.base.locals[RESULT_LOCAL].remove_item_no_update(RESULT_SLOT);
        }
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, target: &Slot) -> bool {
        !matches!(target.container, ContainerKey::Local(i) if i == RESULT_LOCAL)
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        if !self
            .base
            .with_slot(player, slot_index, |s, e| s.has_item(e))
        {
            return ItemStack::empty();
        }
        let mut stack = self
            .base
            .with_slot(player, slot_index, |s, e| s.get_item(e));
        let clicked = stack.copy();
        if slot_index == RESULT_SLOT {
            // vanilla rewrites the slot's own stack: it keeps the new map even when nothing fits
            on_crafted_by(&mut stack, player);
            let rewritten = stack.clone();
            self.base.with_slot(player, slot_index, |s, e| {
                s.with_item_mut(e, |t| *t = rewritten)
            });
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true)
            {
                return ItemStack::empty();
            }
            self.base.with_slot(player, slot_index, |s, e| {
                s.on_quick_craft(e, &stack, &clicked)
            });
        } else if slot_index != ADDITIONAL_SLOT && slot_index != MAP_SLOT {
            if stack.get(&MAP_ID).is_some() {
                if !self.move_item_stack_to(player, &mut stack, MAP_SLOT, MAP_SLOT + 1, false) {
                    return ItemStack::empty();
                }
            } else if !is_additional_item(&stack) {
                if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                    if !self.move_item_stack_to(
                        player,
                        &mut stack,
                        USE_ROW_SLOT_START,
                        USE_ROW_SLOT_END,
                        false,
                    ) {
                        return ItemStack::empty();
                    }
                } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index)
                    && !self.move_item_stack_to(
                        player,
                        &mut stack,
                        INV_SLOT_START,
                        INV_SLOT_END,
                        false,
                    )
                {
                    return ItemStack::empty();
                }
            } else if !self.move_item_stack_to(
                player,
                &mut stack,
                ADDITIONAL_SLOT,
                RESULT_SLOT,
                false,
            ) {
                return ItemStack::empty();
            }
        } else if !self.move_item_stack_to(
            player,
            &mut stack,
            INV_SLOT_START,
            USE_ROW_SLOT_END,
            false,
        ) {
            return ItemStack::empty();
        }
        // unlike the other menus the slot is marked changed whether or not it was emptied
        if stack.is_empty() {
            self.base.with_slot(player, slot_index, |s, e| {
                s.set_by_player_with(e, ItemStack::empty(), &clicked)
            });
        } else {
            let rest = stack.clone();
            self.base
                .with_slot(player, slot_index, |s, e| s.with_item_mut(e, |t| *t = rest));
        }
        self.base
            .with_slot(player, slot_index, |s, e| s.set_changed(e));
        if stack.count() == clicked.count() {
            return ItemStack::empty();
        }
        self.base
            .with_slot(player, slot_index, |s, e| s.on_take(e, &mut stack));
        self.flush_container_events(player);
        self.broadcast_changes(player);
        clicked
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        self.base.locals[RESULT_LOCAL].remove_item_no_update(RESULT_SLOT);
        if self.access.pos().is_some() {
            self.clear_container(player, CONTAINER_LOCAL);
        }
    }
}
