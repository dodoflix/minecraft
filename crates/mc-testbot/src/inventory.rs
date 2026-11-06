//! Container menus as the client sees them: slot contents, state id, carried (cursor) stack, and the
//! client-side prediction of clicks (`AbstractContainerMenu.clicked`) so that `container_click` carries the
//! same changed-slot hashes as the real client.
use std::collections::BTreeMap;

use mc_protocol::components::hashed::component_hash;
use mc_protocol::components::{HashedStack, ItemStack};
use mc_protocol::packets::play::{ContainerInput, ServerboundContainerClickPacket};

/// Menu id of the player's own inventory menu.
pub const PLAYER_MENU: i32 = 0;
/// Slot number used by clicks outside the window (drop).
pub const OUTSIDE: i16 = -999;

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub container_id: i32,
    pub state_id: i32,
    pub slots: Vec<ItemStack>,
    pub cursor: ItemStack,
    /// Window title (plain text) for opened containers.
    pub title: Option<String>,
    /// Registry name of the menu type for opened containers (`minecraft:generic_9x3`).
    pub menu_type: Option<String>,
    /// `container_set_data` properties (furnace progress, enchant costs, …).
    pub data: BTreeMap<i16, i16>,
}

impl Menu {
    pub fn new(container_id: i32, slot_count: usize) -> Menu {
        Menu {
            container_id,
            state_id: 0,
            slots: vec![ItemStack::empty(); slot_count],
            cursor: ItemStack::empty(),
            title: None,
            menu_type: None,
            data: BTreeMap::new(),
        }
    }

    /// `InventoryMenu`: result, 2x2 crafting, armor (head..feet), 27 main, 9 hotbar, offhand.
    pub fn player_inventory() -> Menu {
        Menu::new(PLAYER_MENU, 46)
    }

    /// First slot of the hotbar inside this menu (the player inventory is always the tail of a menu).
    pub fn hotbar_start(&self) -> usize {
        if self.container_id == PLAYER_MENU {
            36
        } else {
            self.slots.len() - 9
        }
    }

    /// First slot of the 27 main inventory slots inside this menu.
    pub fn main_start(&self) -> usize {
        self.hotbar_start() - 27
    }

    /// Menu slot of a `Inventory` index (0-8 hotbar, 9-35 main, 36-39 feet/legs/chest/head, 40 offhand).
    pub fn slot_of_inventory(&self, index: i32) -> Option<usize> {
        let i = usize::try_from(index).ok()?;
        match i {
            0..=8 => Some(self.hotbar_start() + i),
            9..=35 => Some(self.main_start() + i - 9),
            36..=39 if self.container_id == PLAYER_MENU => Some(8 - (i - 36)),
            40 if self.container_id == PLAYER_MENU => Some(45),
            _ => None,
        }
    }

    pub fn hotbar(&self, index: usize) -> &ItemStack {
        &self.slots[self.hotbar_start() + index]
    }

    /// Prediction of one click; returns the packet the real client would send (state id of this menu).
    pub fn click(
        &mut self,
        slot: i16,
        button: i8,
        input: ContainerInput,
    ) -> ServerboundContainerClickPacket {
        let before = self.slots.clone();
        match input {
            ContainerInput::Pickup => self.click_pickup(slot, button),
            ContainerInput::Swap => self.click_swap(slot, button),
            ContainerInput::Throw => self.click_throw(slot, button),
            // other modes (quick move, clone, quick craft, pickup all) are resolved by the server; the real client
            // predicts them too but a wrong prediction only makes the server resend the menu.
            _ => {}
        }
        let changed_slots = self
            .slots
            .iter()
            .enumerate()
            .filter(|(i, s)| !ItemStack::matches(s, &before[*i]))
            .map(|(i, s)| (i as i16, HashedStack::create(s, &component_hash)))
            .collect();
        ServerboundContainerClickPacket {
            container_id: self.container_id,
            state_id: self.state_id,
            slot_num: slot,
            button_num: button,
            container_input: input,
            changed_slots,
            carried_item: HashedStack::create(&self.cursor, &component_hash),
        }
    }

    fn is_result_slot(&self, slot: usize) -> bool {
        self.container_id == PLAYER_MENU && slot == 0
    }

    fn click_pickup(&mut self, slot: i16, button: i8) {
        if slot == OUTSIDE {
            if !self.cursor.is_empty() {
                if button == 0 {
                    self.cursor = ItemStack::empty();
                } else {
                    self.cursor.shrink(1);
                }
            }
            return;
        }
        let Some(idx) = usize::try_from(slot).ok().filter(|i| *i < self.slots.len()) else {
            return;
        };
        if self.is_result_slot(idx) {
            return;
        }
        let (slot_stack, cursor) = (self.slots[idx].clone(), self.cursor.clone());
        if slot_stack.is_empty() {
            if !cursor.is_empty() {
                let amount = if button == 0 { cursor.count() } else { 1 };
                let mut put = cursor.copy_with_count(amount);
                put.limit_size(put.get_max_stack_size());
                let n = put.count();
                self.slots[idx] = put;
                self.cursor.shrink(n);
            }
        } else if cursor.is_empty() {
            let amount = if button == 0 {
                slot_stack.count()
            } else {
                (slot_stack.count() + 1) / 2
            };
            let mut taken = slot_stack.clone();
            let part = taken.split(amount);
            self.slots[idx] = taken;
            self.cursor = part;
        } else if ItemStack::is_same_item_same_components(&slot_stack, &cursor) {
            let amount = if button == 0 { cursor.count() } else { 1 };
            let room = (slot_stack.get_max_stack_size() - slot_stack.count()).max(0);
            let n = amount.min(room);
            self.slots[idx].grow(n);
            self.cursor.shrink(n);
        } else if cursor.count() <= slot_stack.get_max_stack_size() {
            self.slots[idx] = cursor;
            self.cursor = slot_stack;
        }
    }

    fn click_swap(&mut self, slot: i16, button: i8) {
        let Some(idx) = usize::try_from(slot).ok().filter(|i| *i < self.slots.len()) else {
            return;
        };
        if self.is_result_slot(idx) {
            return;
        }
        let other = match button {
            0..=8 => self.hotbar_start() + button as usize,
            40 if self.container_id == PLAYER_MENU => 45,
            _ => return,
        };
        if other != idx {
            self.slots.swap(idx, other);
        }
    }

    fn click_throw(&mut self, slot: i16, button: i8) {
        let Some(idx) = usize::try_from(slot).ok().filter(|i| *i < self.slots.len()) else {
            return;
        };
        if self.is_result_slot(idx) || !self.cursor.is_empty() {
            return;
        }
        if button == 0 {
            self.slots[idx].shrink(1);
        } else {
            self.slots[idx] = ItemStack::empty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_data::Item;

    fn stack(item: Item, n: i32) -> ItemStack {
        ItemStack::with_count(item, n)
    }

    #[test]
    fn pickup_place_split_merge_swap() {
        let mut m = Menu::player_inventory();
        m.slots[9] = stack(Item::STONE, 40);
        // left click picks the whole stack, the packet reports slot 9 emptied and the carried stack
        let p = m.click(9, 0, ContainerInput::Pickup);
        assert_eq!(p.changed_slots, vec![(9, HashedStack::Empty)]);
        assert!(matches!(
            p.carried_item,
            HashedStack::ActualItem { count: 40, .. }
        ));
        // right click on an empty slot puts one down
        m.click(10, 1, ContainerInput::Pickup);
        assert_eq!((m.slots[10].count(), m.cursor.count()), (1, 39));
        // left click on the same item merges (limited by the max stack size 64)
        m.slots[11] = stack(Item::STONE, 60);
        m.click(11, 0, ContainerInput::Pickup);
        assert_eq!((m.slots[11].count(), m.cursor.count()), (64, 35));
        // different item swaps with the cursor
        m.slots[12] = stack(Item::DIRT, 3);
        m.click(12, 0, ContainerInput::Pickup);
        assert!(
            m.slots[12].is(Item::STONE) && m.slots[12].count() == 35 && m.cursor.is(Item::DIRT)
        );
        // right click on a stack takes half (rounded up) only with an empty cursor
        m.cursor = ItemStack::empty();
        m.click(11, 1, ContainerInput::Pickup);
        assert_eq!((m.slots[11].count(), m.cursor.count()), (32, 32));
        // outside the window drops the cursor; the result slot is not clickable
        m.click(OUTSIDE, 0, ContainerInput::Pickup);
        assert!(m.cursor.is_empty());
        m.slots[0] = stack(Item::STONE, 1);
        let p = m.click(0, 0, ContainerInput::Pickup);
        assert!(p.changed_slots.is_empty());
    }

    #[test]
    fn swap_with_hotbar_and_throw() {
        let mut m = Menu::player_inventory();
        m.slots[9] = stack(Item::STONE, 5);
        m.slots[36 + 2] = stack(Item::DIRT, 7);
        let p = m.click(9, 2, ContainerInput::Swap);
        assert_eq!(p.changed_slots.len(), 2);
        assert!(m.slots[9].is(Item::DIRT) && m.slots[38].is(Item::STONE));
        m.click(38, 0, ContainerInput::Throw);
        assert_eq!(m.slots[38].count(), 4);
        m.click(38, 1, ContainerInput::Throw);
        assert!(m.slots[38].is_empty());
    }

    #[test]
    fn inventory_index_mapping() {
        let m = Menu::player_inventory();
        assert_eq!(
            [
                m.slot_of_inventory(0),
                m.slot_of_inventory(9),
                m.slot_of_inventory(36),
                m.slot_of_inventory(39),
                m.slot_of_inventory(40)
            ],
            [Some(36), Some(9), Some(8), Some(5), Some(45)]
        );
        let chest = Menu::new(3, 27 + 36);
        assert_eq!(
            [
                chest.slot_of_inventory(0),
                chest.slot_of_inventory(9),
                chest.slot_of_inventory(40)
            ],
            [Some(54), Some(27), None]
        );
    }
}
