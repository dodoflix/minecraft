//! Slot access of the command targets: `SlotProvider`s over an entity (`Entity.getSlot`) or a container block entity
//! (`Container.getSlot`) that record writes instead of performing them, and the functions applying the recorded writes.
//!
//! A `SlotProvider`'s slots are `Rc<dyn SlotAccess>` with `set(&self)`; the entity or block entity they read is only
//! borrowed shared (it may be the `THIS_ENTITY` of the same loot context), so `set` validates like vanilla
//! (`SlotAccess.forEquipmentSlot`'s validator), remembers the stack, and later reads see it. The caller applies the writes
//! with `apply_entity_writes` / `apply_container_writes` once the loot context is gone.
use crate::commands::CommandSourceStack;
use crate::commands::arguments::selector::EntityRef;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::container::Container;
use crate::world::entity::{Entity, EntityId};
use crate::world::inventory::InventoryMenu;
use crate::world::item::slot::{SlotAccess, SlotProvider};
use crate::world::level::storage::loot::LootContextParamSets;
use crate::world::level::storage::loot::context::{LootParams, LootParamsBuilder};
use mc_core::pos::BlockPos;
use mc_protocol::components::ItemStack;
use std::cell::RefCell;
use std::rc::Rc;

/// `LootParams` of `ItemCommands.getSlotsFromProvider`: `ORIGIN`, `CONTAINER`, optional `THIS_ENTITY`, set `COMMAND_SLOT_SOURCE`.
pub fn slot_params<'a>(
    level: &'a ServerLevel,
    source: &CommandSourceStack,
    container: &'a dyn SlotProvider,
) -> LootParams<'a> {
    let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
    let builder: LootParamsBuilder<'a> = LootParams::builder(level)
        .origin(source.get_position())
        .container(container)
        .optional_this_entity(this_entity);
    builder.create(&LootContextParamSets::COMMAND_SLOT_SOURCE)
}

// ---- entities --------------------------------------------------------------------------------------------------------------

/// `Entity.getSlot(slot)` of `entity` with a write overlay.
pub struct EntitySlots<'a> {
    entity: &'a dyn Entity,
    writes: RefCell<Vec<(i32, ItemStack)>>,
}

impl<'a> EntitySlots<'a> {
    pub fn new(entity: &'a dyn Entity) -> Self {
        Self {
            entity,
            writes: RefCell::new(Vec::new()),
        }
    }

    /// The writes `set` accepted, in order.
    pub fn into_writes(self) -> Vec<(i32, ItemStack)> {
        self.writes.into_inner()
    }

    fn current(&self, slot: i32) -> Option<ItemStack> {
        if let Some((_, stack)) = self.writes.borrow().iter().rev().find(|(s, _)| *s == slot) {
            return Some(stack.clone());
        }
        self.entity.get_slot(slot)
    }
}

struct EntitySlot<'p, 'a> {
    owner: &'p EntitySlots<'a>,
    slot: i32,
}

impl SlotAccess for EntitySlot<'_, '_> {
    fn get(&self) -> ItemStack {
        self.owner.current(self.slot).unwrap_or_default()
    }

    fn set(&self, stack: ItemStack) -> bool {
        if !self.owner.entity.can_set_slot(self.slot, &stack) {
            return false;
        }
        self.owner.writes.borrow_mut().push((self.slot, stack));
        true
    }
}

impl SlotProvider for EntitySlots<'_> {
    fn get_slot(&self, slot: i32) -> Option<Rc<dyn SlotAccess + '_>> {
        self.entity
            .get_slot(slot)
            .map(|_| Rc::new(EntitySlot { owner: self, slot }) as Rc<dyn SlotAccess + '_>)
    }
}

/// `unpackChestVehicleLootTable(null)` of the chest vehicle `id`, which every `getChestVehicleSlot(slot).get()/.set()` does first
/// (`/item` calls this for each target before it reads or writes `container.N`).
pub fn unpack_chest_vehicle_loot(level: &mut ServerLevel, id: EntityId) {
    level.with_entity(id, |level, entity| {
        let position = entity.position();
        if let Some(chest) = entity.chest_vehicle_mut() {
            chest.unpack_for_access(level, position);
        }
    });
}

/// Performs `getSlot(slot).set(stack)` for each write on the entity (`Entity::set_slot`).
pub fn apply_entity_writes(level: &mut ServerLevel, id: EntityId, writes: Vec<(i32, ItemStack)>) {
    if writes.is_empty() {
        return;
    }
    level.with_entity(id, |level, entity| {
        for (slot, stack) in writes {
            entity.set_slot(level, slot, stack);
        }
    });
}

// ---- containers ------------------------------------------------------------------------------------------------------------

/// `Container.getSlot(slot)` with a write overlay.
pub struct ContainerSlots<'a> {
    container: &'a dyn Container,
    writes: RefCell<Vec<(usize, ItemStack)>>,
}

impl<'a> ContainerSlots<'a> {
    pub fn new(container: &'a dyn Container) -> Self {
        Self {
            container,
            writes: RefCell::new(Vec::new()),
        }
    }

    pub fn into_writes(self) -> Vec<(usize, ItemStack)> {
        self.writes.into_inner()
    }

    fn current(&self, slot: usize) -> ItemStack {
        if let Some((_, stack)) = self.writes.borrow().iter().rev().find(|(s, _)| *s == slot) {
            return stack.clone();
        }
        self.container.get_item(slot).clone()
    }
}

struct ContainerSlot<'p, 'a> {
    owner: &'p ContainerSlots<'a>,
    slot: usize,
}

impl SlotAccess for ContainerSlot<'_, '_> {
    fn get(&self) -> ItemStack {
        self.owner.current(self.slot)
    }

    fn set(&self, stack: ItemStack) -> bool {
        self.owner.writes.borrow_mut().push((self.slot, stack));
        true
    }
}

impl SlotProvider for ContainerSlots<'_> {
    fn get_slot(&self, slot: i32) -> Option<Rc<dyn SlotAccess + '_>> {
        (slot >= 0 && (slot as usize) < self.container.get_container_size()).then(|| {
            Rc::new(ContainerSlot {
                owner: self,
                slot: slot as usize,
            }) as Rc<dyn SlotAccess + '_>
        })
    }
}

/// `Container.setItem(slot, stack)` for each write on the container block entity at `pos`.
pub fn apply_container_writes(
    level: &mut ServerLevel,
    pos: BlockPos,
    writes: Vec<(usize, ItemStack)>,
) {
    if writes.is_empty() {
        return;
    }
    level.with_block_entity(pos, |level, block_entity| {
        if let Some(container) = block_entity.as_container_mut() {
            for (slot, stack) in writes {
                container.set_item(slot, stack);
            }
        }
        if block_entity.container_pending() {
            block_entity.container_accessed(level);
        }
    });
}

/// `source.getLevel().getBlockEntity(pos) instanceof Container`.
pub fn container_at(level: &ServerLevel, pos: BlockPos) -> Option<&dyn Container> {
    use crate::world::level::BlockGetter;
    let block_entity: &dyn BlockEntity = level.get_block_entity(pos)?;
    block_entity.as_container()
}

// ---- players ---------------------------------------------------------------------------------------------------------------

/// `Inventory.clearOrCountMatchingItems(predicate, countingOnly, maxCount, inventoryMenu.getCraftSlots())`, followed by
/// the `/clear` menu updates (`containerMenu.broadcastChanges()`, `inventoryMenu.slotsChanged(inventory)`) unless only counting.
pub fn clear_or_count_matching_items(
    level: &mut ServerLevel,
    player: EntityId,
    predicate: &dyn Fn(&ItemStack) -> bool,
    counting_only: bool,
    max_count: i32,
) -> i32 {
    level
        .with_player(player, |level, player| {
            // the cursor stack lives in the current menu, the crafting grid in the inventory menu (which may be the same object)
            let mut carried = std::mem::take(
                player
                    .player
                    .menus
                    .current_mut()
                    .base_mut()
                    .get_carried_mut(),
            );
            let count = {
                let inventory = &mut player.player.inventory;
                let menu = player
                    .player
                    .menus
                    .inventory_menu_mut()
                    .as_any_mut()
                    .downcast_mut::<InventoryMenu>()
                    .expect("inventory menu");
                inventory.clear_or_count_matching_items(
                    predicate,
                    counting_only,
                    max_count,
                    menu.get_craft_slots_mut(),
                    &mut carried,
                )
            };
            *player
                .player
                .menus
                .current_mut()
                .base_mut()
                .get_carried_mut() = carried;
            if !counting_only {
                player.broadcast_menu_changes(level);
                player.inventory_menu_slots_changed(level);
            }
            count
        })
        .unwrap_or(0)
}

/// `player.getInventory().add(stack)` (`/loot give`): whether anything was added (the stack is a copy the caller does not reuse).
pub fn give_to_inventory(level: &mut ServerLevel, player: EntityId, mut stack: ItemStack) -> bool {
    level
        .with_player(player, |_, player| {
            let infinite = player.player.abilities.instabuild;
            player.player.inventory.add(&mut stack, infinite)
        })
        .unwrap_or(false)
}

/// Resolves a command entity reference against its level.
pub fn entity_in<'a>(
    server: &'a crate::server::MinecraftServer,
    entity: &EntityRef,
) -> Option<&'a dyn Entity> {
    server.level(&entity.dimension)?.get_entity(entity.id)
}
