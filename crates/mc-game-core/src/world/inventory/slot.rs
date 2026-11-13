//! `Slot` and its overridable behaviour.
//!
//! A vanilla `Slot` subclass becomes a [`SlotBehavior`] implementor; every overridable `Slot` method has a hook with
//! the base implementation as default. Overrides that call `super.x()` call `slot.base_x(..)`. Hooks take `&self`
//! (use `Cell` for mutable slot state such as `ResultSlot.removeCount`).

use super::MenuPlayer;
use crate::world::block_entity::ContainerUser;
use crate::world::container::{Container, SimpleContainer};
use crate::world::entity::EntityId;
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use mc_core::pos::BlockPos;
use std::cell::RefCell;
use std::rc::Rc;

/// A container shared between a menu and its owner (block entity, player ender chest, entity inventory).
pub type SharedContainer = Rc<RefCell<dyn Container>>;

/// Which container a slot reads (`Slot.container`). Identity comparison: [`ContainerKey::same_container`].
#[derive(Clone)]
pub enum ContainerKey {
    /// The menu owner's `Inventory`.
    PlayerInventory,
    /// `MenuBase::locals[i]` (crafting grids, result containers, anvil inputs, ...).
    Local(usize),
    Shared(SharedContainer),
    /// The `Container` view of the block entity at `pos`, resolved through the player's level on every access.
    BlockEntity(BlockPos),
    /// The `Container` of an entity (`Entity::as_container`): horse inventory, chest minecart, ...
    Entity(EntityId),
    /// The owner's `PlayerEnderChestContainer`.
    PlayerEnderChest,
    /// `mob.createEquipmentSlotContainer(slot)`: one equipment slot of a mount (via `Entity::as_mount_inventory`).
    EntityEquipment(EntityId, crate::world::entity::equipment::EquipmentSlot),
}

impl ContainerKey {
    /// Reference identity of two keys of the same menu pair (`slot.container == container`). `Local` keys only compare
    /// within one menu.
    pub fn same_container(&self, other: &ContainerKey) -> bool {
        match (self, other) {
            (Self::PlayerInventory, Self::PlayerInventory)
            | (Self::PlayerEnderChest, Self::PlayerEnderChest) => true,
            (Self::Local(a), Self::Local(b)) => a == b,
            (Self::Shared(a), Self::Shared(b)) => Rc::ptr_eq(a, b),
            (Self::BlockEntity(a), Self::BlockEntity(b)) => a == b,
            (Self::Entity(a), Self::Entity(b)) => a == b,
            (Self::EntityEquipment(a, sa), Self::EntityEquipment(b, sb)) => a == b && sa == sb,
            _ => false,
        }
    }

    /// `container.stillValid(player)` for the container behind this key (a container that cannot be resolved any more
    /// is not valid; `Local`/`PlayerInventory` containers are always valid like `SimpleContainer`/`Inventory`).
    pub fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        match self {
            Self::PlayerInventory | Self::Local(_) => true,
            Self::Shared(c) => c.borrow().still_valid(player),
            Self::BlockEntity(pos) => player
                .level_ref()
                .and_then(|l| l.get_block_entity(*pos))
                .is_some_and(|be| be.menu_container().is_some_and(|c| c.still_valid(player))),
            Self::Entity(id) => player
                .level_ref()
                .and_then(|l| l.get_entity(*id))
                .is_some_and(|e| e.as_container().is_some_and(|c| c.still_valid(player))),
            Self::PlayerEnderChest => player.ender_chest().is_none_or(|c| c.still_valid(player)),
            // `player.getVehicle() == mob || within range`: the mount menus check the range themselves
            Self::EntityEquipment(..) => true,
        }
    }
}

/// Everything a slot operation can touch: the player (and through it the player's inventory) and the menu's local
/// containers.
pub struct SlotEnv<'a> {
    pub player: &'a mut dyn MenuPlayer,
    pub locals: &'a mut Vec<Box<dyn Container>>,
    /// Set by item code that calls `player.containerMenu.slotsChanged(player.getInventory())` while the menu is
    /// running a click (bundles); the menu replays it right after the item hook returns.
    pub slots_changed_requested: &'a mut bool,
}

impl SlotEnv<'_> {
    pub fn with_container<R>(
        &mut self,
        key: &ContainerKey,
        f: impl FnOnce(&mut dyn Container) -> R,
    ) -> R {
        match key {
            ContainerKey::Local(i) => f(self.locals[*i].as_mut()),
            _ => with_player_container(self.player, key, f),
        }
    }
}

/// Resolves every `ContainerKey` except `Local` against the player. A container that no longer exists (block entity
/// removed, entity gone) is replaced by a scratch container: reads are empty and writes are dropped; the menu closes
/// itself on the next `still_valid` check.
pub fn with_player_container<R>(
    player: &mut dyn MenuPlayer,
    key: &ContainerKey,
    f: impl FnOnce(&mut dyn Container) -> R,
) -> R {
    match key {
        ContainerKey::PlayerInventory => f(player.inventory_mut()),
        ContainerKey::Local(_) => unreachable!("menu-local containers are resolved by the menu"),
        ContainerKey::Shared(c) => f(&mut *c.borrow_mut()),
        ContainerKey::BlockEntity(pos) => {
            let mut f = Some(f);
            let resolved = player.level().and_then(|level| {
                let be = level.get_block_entity_immediate(*pos)?;
                let container = be.menu_container_mut()?;
                let result = (f.take().expect("called once"))(container);
                let pending = be.container_pending();
                Some((result, pending))
            });
            match resolved {
                Some((result, pending)) => {
                    if pending && let Some(level) = player.level() {
                        level.with_block_entity(*pos, |level, be| be.container_accessed(level));
                    }
                    result
                }
                None => (f.take().expect("not consumed"))(&mut SimpleContainer::new(0)),
            }
        }
        ContainerKey::Entity(id) => {
            let mut f = Some(f);
            let resolved = player
                .level()
                .and_then(|l| {
                    l.with_entity(*id, |_, e| {
                        e.as_container_mut()
                            .map(|c| (f.take().expect("called once"))(c))
                    })
                })
                .flatten();
            match resolved {
                Some(r) => r,
                None => (f.take().expect("not consumed"))(&mut SimpleContainer::new(0)),
            }
        }
        ContainerKey::EntityEquipment(id, equipment) => {
            let mut f = Some(f);
            let resolved = player.level().and_then(|l| {
                l.with_entity(*id, |level, e| {
                    let mount = e.as_mount_inventory_mut()?;
                    let mut container = super::mount_menu::EquipmentSlotContainer {
                        stack: mount.get_equipment_slot_item(*equipment),
                        dirty: false,
                    };
                    let result = (f.take().expect("called once"))(&mut container);
                    if container.dirty {
                        mount.set_equipment_slot_item(level, *equipment, container.stack);
                    }
                    Some(result)
                })
            });
            match resolved.flatten() {
                Some(r) => r,
                None => (f.take().expect("not consumed"))(&mut SimpleContainer::new(1)),
            }
        }
        ContainerKey::PlayerEnderChest => match player.ender_chest_mut() {
            Some(c) => f(c),
            None => f(&mut SimpleContainer::new(27)),
        },
    }
}

impl ContainerKey {
    /// `container.startOpen(player)`.
    pub fn start_open(&self, player: &mut dyn MenuPlayer) {
        self.open_or_close(player, true);
    }

    /// `container.stopOpen(player)`.
    pub fn stop_open(&self, player: &mut dyn MenuPlayer) {
        self.open_or_close(player, false);
    }

    fn open_or_close(&self, player: &mut dyn MenuPlayer, open: bool) {
        let user = player.entity_id();
        let container_user = ContainerUser {
            id: user,
            spectator: player.is_spectator(),
            interaction_range: player.container_interaction_range(),
        };
        let notify_be = |player: &mut dyn MenuPlayer, pos: BlockPos| {
            if let Some(level) = player.level() {
                level.with_block_entity(pos, |level, be| {
                    if open {
                        be.start_open(level, &container_user)
                    } else {
                        be.stop_open(level, &container_user)
                    }
                });
            }
        };
        match self {
            ContainerKey::PlayerInventory
            | ContainerKey::Local(_)
            | ContainerKey::EntityEquipment(..) => {}
            ContainerKey::Shared(c) => {
                if open {
                    c.borrow_mut().start_open(user)
                } else {
                    c.borrow_mut().stop_open(user)
                }
            }
            ContainerKey::BlockEntity(pos) => notify_be(player, *pos),
            ContainerKey::Entity(id) => {
                if let Some(level) = player.level() {
                    level.with_entity(*id, |level, e| e.container_open_close(level, user, open));
                }
            }
            ContainerKey::PlayerEnderChest => {
                let active = player.ender_chest().and_then(|c| c.active_chest());
                if let Some(pos) = active {
                    notify_be(player, pos);
                }
                if !open && let Some(c) = player.ender_chest_mut() {
                    c.clear_active_chest();
                }
            }
        }
    }
}

/// Overridable `Slot` methods. Defaults are the base `Slot` behavior.
pub trait SlotBehavior {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        true
    }
    fn may_pickup(&self, _slot: &Slot, _env: &mut SlotEnv) -> bool {
        true
    }
    fn allow_modification(&self, slot: &Slot, env: &mut SlotEnv) -> bool {
        slot.base_allow_modification(env)
    }
    fn get_max_stack_size(&self, slot: &Slot, env: &mut SlotEnv) -> i32 {
        slot.base_get_max_stack_size(env)
    }
    fn remove(&self, slot: &Slot, env: &mut SlotEnv, amount: i32) -> ItemStack {
        slot.base_remove(env, amount)
    }
    /// `onTake(player, carried)`: `carried` is the taken stack itself, so `onCraftedBy` rewrites it in place.
    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, _carried: &mut ItemStack) {
        slot.set_changed(env);
    }
    fn set_by_player(
        &self,
        slot: &Slot,
        env: &mut SlotEnv,
        stack: ItemStack,
        _previous: &ItemStack,
    ) {
        slot.set(env, stack);
    }
    fn on_quick_craft(&self, _slot: &Slot, _env: &mut SlotEnv, _picked: &ItemStack, _count: i32) {}
    fn on_swap_craft(&self, _slot: &Slot, _count: i32) {}
    fn safe_clone(&self, slot: &Slot, env: &mut SlotEnv) -> ItemStack {
        slot.base_safe_clone(env)
    }
    /// `isFake()`.
    fn is_fake(&self) -> bool {
        false
    }
}

struct PlainSlot;
impl SlotBehavior for PlainSlot {}

pub struct Slot {
    pub container: ContainerKey,
    slot: usize,
    /// Index in the owning menu (set by `addSlot`).
    pub index: usize,
    pub x: i32,
    pub y: i32,
    behavior: Box<dyn SlotBehavior>,
}

impl Slot {
    pub fn new(container: ContainerKey, slot: usize, x: i32, y: i32) -> Self {
        Self::with_behavior(container, slot, x, y, Box::new(PlainSlot))
    }

    pub fn with_behavior(
        container: ContainerKey,
        slot: usize,
        x: i32,
        y: i32,
        behavior: Box<dyn SlotBehavior>,
    ) -> Self {
        Self {
            container,
            slot,
            index: 0,
            x,
            y,
            behavior,
        }
    }

    /// `getContainerSlot()`.
    pub fn get_container_slot(&self) -> usize {
        self.slot
    }

    pub fn is_fake(&self) -> bool {
        self.behavior.is_fake()
    }

    // ---- non-virtual ----

    /// `getItem()` (a copy of the stored stack; use [`Slot::with_item_mut`] to mutate it in place).
    pub fn get_item(&self, env: &mut SlotEnv) -> ItemStack {
        env.with_container(&self.container, |c| c.get_item(self.slot).clone())
    }

    pub fn has_item(&self, env: &mut SlotEnv) -> bool {
        env.with_container(&self.container, |c| !c.get_item(self.slot).is_empty())
    }

    /// In-place access to the stored stack (mutating the Java object returned by `getItem()`).
    pub fn with_item_mut<R>(&self, env: &mut SlotEnv, f: impl FnOnce(&mut ItemStack) -> R) -> R {
        env.with_container(&self.container, |c| match c.get_item_mut(self.slot) {
            Some(s) => f(s),
            None => f(&mut ItemStack::empty()),
        })
    }

    pub fn set(&self, env: &mut SlotEnv, stack: ItemStack) {
        env.with_container(&self.container, |c| c.set_item(self.slot, stack));
        self.set_changed(env);
    }

    pub fn set_changed(&self, env: &mut SlotEnv) {
        env.with_container(&self.container, |c| c.set_changed());
    }

    /// `getMaxStackSize(stack)`.
    pub fn get_max_stack_size_for(&self, env: &mut SlotEnv, stack: &ItemStack) -> i32 {
        self.get_max_stack_size(env).min(stack.get_max_stack_size())
    }

    pub fn on_quick_craft(&self, env: &mut SlotEnv, picked: &ItemStack, original: &ItemStack) {
        let count = original.count() - picked.count();
        if count > 0 {
            self.behavior.on_quick_craft(self, env, original, count);
        }
    }

    pub fn try_remove(&self, env: &mut SlotEnv, amount: i32, max_amount: i32) -> Option<ItemStack> {
        if !self.may_pickup(env)
            || (!self.allow_modification(env) && max_amount < self.get_item(env).count())
        {
            None
        } else {
            let amount = amount.min(max_amount);
            let result = self.remove(env, amount);
            if result.is_empty() {
                None
            } else {
                if self.get_item(env).is_empty() {
                    self.set_by_player_with(env, ItemStack::empty(), &result);
                }
                Some(result)
            }
        }
    }

    pub fn safe_take(&self, env: &mut SlotEnv, amount: i32, max_amount: i32) -> ItemStack {
        match self.try_remove(env, amount, max_amount) {
            Some(mut item) => {
                self.on_take(env, &mut item);
                item
            }
            None => ItemStack::empty(),
        }
    }

    pub fn safe_insert(&self, env: &mut SlotEnv, stack: ItemStack) -> ItemStack {
        let amount = stack.count();
        self.safe_insert_amount(env, stack, amount)
    }

    pub fn safe_insert_amount(
        &self,
        env: &mut SlotEnv,
        mut input: ItemStack,
        input_amount: i32,
    ) -> ItemStack {
        if !input.is_empty() && self.may_place(env, &input) {
            let slot_stack = self.get_item(env);
            let transferable = input_amount
                .min(input.count())
                .min(self.get_max_stack_size_for(env, &input) - slot_stack.count());
            if transferable <= 0 {
                return input;
            }
            if slot_stack.is_empty() {
                let part = input.split(transferable);
                self.set_by_player(env, part);
            } else if ItemStack::is_same_item_same_components(&slot_stack, &input) {
                input.shrink(transferable);
                // Java grows the stored object in place, so `previous` seen by `setByPlayer` is already grown.
                self.with_item_mut(env, |s| s.grow(transferable));
                let grown = self.get_item(env);
                self.set_by_player(env, grown);
            }
        }
        input
    }

    pub fn set_by_player(&self, env: &mut SlotEnv, stack: ItemStack) {
        let previous = self.get_item(env);
        self.set_by_player_with(env, stack, &previous);
    }

    // ---- virtual (dispatch) ----

    pub fn may_place(&self, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        self.behavior.may_place(self, env, stack)
    }
    pub fn may_pickup(&self, env: &mut SlotEnv) -> bool {
        self.behavior.may_pickup(self, env)
    }
    pub fn allow_modification(&self, env: &mut SlotEnv) -> bool {
        self.behavior.allow_modification(self, env)
    }
    pub fn get_max_stack_size(&self, env: &mut SlotEnv) -> i32 {
        self.behavior.get_max_stack_size(self, env)
    }
    pub fn remove(&self, env: &mut SlotEnv, amount: i32) -> ItemStack {
        self.behavior.remove(self, env, amount)
    }
    pub fn on_take(&self, env: &mut SlotEnv, carried: &mut ItemStack) {
        self.behavior.on_take(self, env, carried)
    }
    pub fn set_by_player_with(&self, env: &mut SlotEnv, stack: ItemStack, previous: &ItemStack) {
        self.behavior.set_by_player(self, env, stack, previous)
    }
    pub fn on_swap_craft(&self, count: i32) {
        self.behavior.on_swap_craft(self, count)
    }
    pub fn safe_clone(&self, env: &mut SlotEnv) -> ItemStack {
        self.behavior.safe_clone(self, env)
    }

    // ---- base implementations (vanilla `Slot`) ----

    pub fn base_allow_modification(&self, env: &mut SlotEnv) -> bool {
        let item = self.get_item(env);
        self.may_pickup(env) && self.may_place(env, &item)
    }
    pub fn base_get_max_stack_size(&self, env: &mut SlotEnv) -> i32 {
        env.with_container(&self.container, |c| c.get_max_stack_size())
    }
    pub fn base_remove(&self, env: &mut SlotEnv, amount: i32) -> ItemStack {
        env.with_container(&self.container, |c| c.remove_item(self.slot, amount))
    }
    pub fn base_safe_clone(&self, env: &mut SlotEnv) -> ItemStack {
        let item = self.get_item(env);
        item.copy_with_count(item.get_max_stack_size())
    }
}
