//! Pieces the in-process click goldens (`menu_golden`, `container_menu_golden`) share: the state-line text of a stack,
//! stacks built from the script's `mods` word, the recording synchronizer and container listener.

use mc_core::ident::Identifier;
use mc_data::Item;
use mc_game::world::inventory::{
    ContainerInput, ContainerListener, ContainerSynchronizer, RemoteSlot, Slot,
};
use mc_game::world::item::ItemStack;
use mc_protocol::components::{
    BUNDLE_CONTENTS, CUSTOM_NAME, ENCHANTMENTS, Holder, ItemEnchantments, MAX_STACK_SIZE,
};
use mc_text::component::Component;
use std::cell::RefCell;
use std::rc::Rc;

/// Components that make stacks differ besides item, count and damage: custom name, enchantments, a max stack size
/// override.
fn marks(s: &ItemStack) -> String {
    let mut out = String::new();
    if let Some(name) = s.get(&CUSTOM_NAME) {
        out += &format!("+n({})", name.get_string());
    }
    if let Some(e) = s.get(&ENCHANTMENTS).filter(|e| !e.is_empty()) {
        out += "+e(";
        for (holder, level) in e.entries() {
            let id = holder.name().expect("registered enchantment");
            out += &format!("{}:{level},", id.path());
        }
        out += ")";
    }
    let max = s.get_max_stack_size();
    if max != ItemStack::new(s.item()).get_max_stack_size() {
        out += &format!("+m({max})");
    }
    out
}

/// The text of a stack in the state lines (`MenuGolden.st`).
pub fn st(s: &ItemStack) -> String {
    if s.is_empty() {
        return "-".into();
    }
    let n = s.item().name().trim_start_matches("minecraft:");
    let mut out = format!(
        "{n}x{}{}",
        s.count(),
        if s.is_damaged() {
            format!("d{}", s.get_damage_value())
        } else {
            String::new()
        }
    );
    if let Some(bc) = s.get(&BUNDLE_CONTENTS) {
        out.push('{');
        for t in bc.items() {
            out += &format!(
                "{}x{},",
                t.item.name().trim_start_matches("minecraft:"),
                t.count
            );
        }
        out += &format!(";{}}}", bc.selected_item_index());
    }
    out + &marks(s)
}

/// `MenuGolden.mk(name, count, dmg, mods)`; `mods` is `-` or a comma list of `bind` (binding curse), `prot`
/// (protection 3), `name:X` and `max:N`.
pub fn mk(name: &str, count: i32, dmg: i32, mods: &str) -> ItemStack {
    let mut s = ItemStack::with_count(
        Item::by_name(name).unwrap_or_else(|| panic!("item {name}")),
        count,
    );
    if dmg > 0 {
        s.set_damage_value(dmg);
    }
    if mods != "-" {
        for m in mods.split(',') {
            let enchant = |path: &str, level| {
                ItemEnchantments::new(vec![(
                    Holder::Reference(Identifier::parse(&format!("minecraft:{path}")).unwrap()),
                    level,
                )])
            };
            if m == "bind" {
                s.set(ENCHANTMENTS, enchant("binding_curse", 1));
            } else if m == "prot" {
                s.set(ENCHANTMENTS, enchant("protection", 3));
            } else if let Some(name) = m.strip_prefix("name:") {
                s.set(CUSTOM_NAME, Component::literal(name));
            } else if let Some(max) = m.strip_prefix("max:") {
                s.set(MAX_STACK_SIZE, max.parse().expect("max stack size"));
            } else {
                panic!("unknown mod {m}");
            }
        }
    }
    s
}

pub fn input(name: &str) -> ContainerInput {
    match name {
        "PICKUP" => ContainerInput::Pickup,
        "QUICK_MOVE" => ContainerInput::QuickMove,
        "SWAP" => ContainerInput::Swap,
        "CLONE" => ContainerInput::Clone,
        "THROW" => ContainerInput::Throw,
        "QUICK_CRAFT" => ContainerInput::QuickCraft,
        "PICKUP_ALL" => ContainerInput::PickupAll,
        _ => panic!("input {name}"),
    }
}

/// The recording `ContainerSynchronizer` of the Java generators.
pub struct Sync(pub Rc<RefCell<Vec<String>>>);

impl ContainerSynchronizer for Sync {
    fn send_initial_data(
        &mut self,
        _id: i32,
        state_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
        _data: &[i32],
    ) {
        let mut b = format!("SYNC init {state_id} {}", st(&carried));
        for (i, s) in items.iter().enumerate() {
            if !s.is_empty() {
                b += &format!(" {i}:{}", st(s));
            }
        }
        self.0.borrow_mut().push(b);
    }
    fn send_slot_change(&mut self, _id: i32, state_id: i32, slot: usize, stack: ItemStack) {
        self.0
            .borrow_mut()
            .push(format!("SYNC slot {state_id} {slot} {}", st(&stack)));
    }
    fn send_carried_change(&mut self, stack: ItemStack) {
        self.0
            .borrow_mut()
            .push(format!("SYNC carried {}", st(&stack)));
    }
    fn send_data_change(&mut self, _id: i32, id_: usize, v: i32) {
        self.0.borrow_mut().push(format!("SYNC data {id_} {v}"));
    }
    fn create_slot(&self) -> RemoteSlot {
        RemoteSlot::new_synchronized()
    }
}

/// The events of one script (`MenuGolden.events`), shared by the player stand-in and the listeners so that their lines
/// keep the order they happened in.
#[derive(Clone, Default)]
pub struct Events(Rc<RefCell<Vec<String>>>);

impl Events {
    pub fn push(&self, event: String) {
        self.0.borrow_mut().push(event);
    }

    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

/// `MenuGolden.LogListener`: the `listen` op's listener; two instances with the same id are the same Java object.
pub struct LogListener {
    pub id: usize,
    pub log: Events,
}

impl ContainerListener for LogListener {
    fn identity(&self) -> Option<usize> {
        Some(self.id)
    }
    fn slot_changed(&mut self, _slot: &Slot, slot_index: usize, stack: &ItemStack) {
        self.log
            .push(format!("LISTEN {} {slot_index} {}", self.id, st(stack)));
    }
    fn data_changed(&mut self, id: usize, value: i32) {
        self.log
            .push(format!("LISTENDATA {} {id} {value}", self.id));
    }
}
