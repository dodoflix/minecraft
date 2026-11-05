//! Slot access of the command targets: `SlotProvider`s over an entity (`Entity.getSlot`) or a container block entity
//! (`Container.getSlot`) that record writes instead of performing them, and the functions applying the recorded writes.
//!
//! A `SlotProvider`'s slots are `Rc<dyn SlotAccess>` with `set(&self)`; the entity or block entity they read is only
//! borrowed shared (it may be the `THIS_ENTITY` of the same loot context), so `set` validates like vanilla
//! (`SlotAccess.forEquipmentSlot`'s validator), remembers the stack, and later reads see it. The caller applies the writes
//! with `apply_entity_writes` / `apply_container_writes` once the loot context is gone.
use crate::advancements::predicates::entity::support::equipment_slot_of;
use crate::commands::arguments::selector::EntityRef;
use crate::commands::CommandSourceStack;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::container::Container;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::inventory::{equippable_slot, InventoryMenu};
use crate::world::item::slot::{SlotAccess, SlotProvider};
use crate::world::level::storage::loot::context::{LootParams, LootParamsBuilder};
use crate::world::level::storage::loot::LootContextParamSets;
use mc_core::pos::BlockPos;
use mc_protocol::components::ItemStack;
use std::cell::RefCell;
use std::rc::Rc;

/// `LootParams` of `ItemCommands.getSlotsFromProvider`: `ORIGIN`, `CONTAINER`, optional `THIS_ENTITY`, set `COMMAND_SLOT_SOURCE`.
pub fn slot_params<'a>(level: &'a ServerLevel, source: &CommandSourceStack, container: &'a dyn SlotProvider) -> LootParams<'a> {
    let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
    let builder: LootParamsBuilder<'a> = LootParams::builder(level).origin(source.get_position()).container(container).optional_this_entity(this_entity);
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
        Self { entity, writes: RefCell::new(Vec::new()) }
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

    /// The validator of `LivingEntity.createEquipmentSlotAccess`: armor slots below the head and the body/saddle slots only
    /// take items that go there (or nothing).
    fn accepts(&self, slot: i32, stack: &ItemStack) -> bool {
        if self.entity.living().is_none() {
            return true;
        }
        match equipment_slot_of(slot) {
            Some(equipment) if !matches!(equipment, EquipmentSlot::Head | EquipmentSlot::MainHand | EquipmentSlot::OffHand) => {
                // `getEquipmentSlotForItem`: the equippable's slot (`canUseSlot` is always true for the entities modelled), else the main hand
                stack.is_empty() || equippable_slot(stack).unwrap_or(EquipmentSlot::MainHand) == equipment
            }
            _ => true,
        }
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
        if !self.owner.accepts(self.slot, &stack) {
            return false;
        }
        self.owner.writes.borrow_mut().push((self.slot, stack));
        true
    }
}

impl SlotProvider for EntitySlots<'_> {
    fn get_slot(&self, slot: i32) -> Option<Rc<dyn SlotAccess + '_>> {
        self.entity.get_slot(slot).map(|_| Rc::new(EntitySlot { owner: self, slot }) as Rc<dyn SlotAccess + '_>)
    }
}

/// Performs `getSlot(slot).set(stack)` for each write on the entity (`ServerPlayer`: carried item, crafting grid, inventory,
/// ender chest, equipment; other living entities: equipment).
pub fn apply_entity_writes(level: &mut ServerLevel, id: EntityId, writes: Vec<(i32, ItemStack)>) {
    if writes.is_empty() {
        return;
    }
    level.with_entity(id, |level, entity| {
        for (slot, stack) in writes {
            set_entity_slot(level, entity, slot, stack);
        }
    });
}

fn set_entity_slot(level: &mut ServerLevel, entity: &mut dyn Entity, slot: i32, stack: ItemStack) {
    if let Some(player) = entity.as_any_mut().downcast_mut::<ServerPlayer>() {
        if set_player_slot(level, player, slot, &stack) {
            return;
        }
        if let Some(equipment) = equipment_slot_of(slot) {
            player.set_item_slot(level, equipment, stack);
        }
        return;
    }
    if let Some(equipment) = equipment_slot_of(slot)
        && let Some(living) = entity.living_mut()
    {
        let old = living.equipment.set(equipment, stack.clone());
        entity.on_equip_item(level, equipment, &old, &stack);
    }
}

/// `Player.getSlot(slot)` before the `LivingEntity` fallback; `false` when `slot` is not one of the player's own.
fn set_player_slot(level: &mut ServerLevel, player: &mut ServerPlayer, slot: i32, stack: &ItemStack) -> bool {
    if slot == 499 {
        player.player.menus.current_mut().base_mut().set_carried(stack.clone());
        return true;
    }
    let craft_slot = slot - 500;
    if (0..4).contains(&craft_slot) {
        let menu = player.player.menus.inventory_menu_mut().as_any_mut().downcast_mut::<InventoryMenu>().expect("inventory menu");
        menu.get_craft_slots_mut().set_item(craft_slot as usize, stack.clone());
        player.inventory_menu_slots_changed(level);
        return true;
    }
    if slot >= 0 && (slot as usize) < player.player.inventory.get_non_equipment_items().len() {
        player.player.inventory.set_item(slot as usize, stack.clone());
        return true;
    }
    let ender_slot = slot - 200;
    if ender_slot >= 0 && (ender_slot as usize) < player.player.ender_chest.get_container_size() {
        player.player.ender_chest.set_item(ender_slot as usize, stack.clone());
        return true;
    }
    false
}

// ---- containers ------------------------------------------------------------------------------------------------------------

/// `Container.getSlot(slot)` with a write overlay.
pub struct ContainerSlots<'a> {
    container: &'a dyn Container,
    writes: RefCell<Vec<(usize, ItemStack)>>,
}

impl<'a> ContainerSlots<'a> {
    pub fn new(container: &'a dyn Container) -> Self {
        Self { container, writes: RefCell::new(Vec::new()) }
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
        (slot >= 0 && (slot as usize) < self.container.get_container_size()).then(|| Rc::new(ContainerSlot { owner: self, slot: slot as usize }) as Rc<dyn SlotAccess + '_>)
    }
}

/// `Container.setItem(slot, stack)` for each write on the container block entity at `pos`.
pub fn apply_container_writes(level: &mut ServerLevel, pos: BlockPos, writes: Vec<(usize, ItemStack)>) {
    if writes.is_empty() {
        return;
    }
    level.with_block_entity(pos, |_, block_entity| {
        if let Some(container) = block_entity.as_container_mut() {
            for (slot, stack) in writes {
                container.set_item(slot, stack);
            }
        }
    });
}

/// `source.getLevel().getBlockEntity(pos) instanceof Container`.
pub fn container_at<'a>(level: &'a ServerLevel, pos: BlockPos) -> Option<&'a dyn Container> {
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
            let mut carried = std::mem::take(player.player.menus.current_mut().base_mut().get_carried_mut());
            let count = {
                let inventory = &mut player.player.inventory;
                let menu = player.player.menus.inventory_menu_mut().as_any_mut().downcast_mut::<InventoryMenu>().expect("inventory menu");
                inventory.clear_or_count_matching_items(predicate, counting_only, max_count, menu.get_craft_slots_mut(), &mut carried)
            };
            *player.player.menus.current_mut().base_mut().get_carried_mut() = carried;
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
    level.with_player(player, |_, player| {
        let infinite = player.player.abilities.instabuild;
        player.player.inventory.add(&mut stack, infinite)
    })
    .unwrap_or(false)
}

/// Resolves a command entity reference against its level.
pub fn entity_in<'a>(server: &'a crate::server::MinecraftServer, entity: &EntityRef) -> Option<&'a dyn Entity> {
    server.level(&entity.dimension)?.get_entity(entity.id)
}
