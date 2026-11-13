//! Entity accessors the predicates need (vanilla's `Entity`/`LivingEntity`/`Player` methods that have no counterpart on
//! our traits) and the data types of the `Entity` hooks.

use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::movement;
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::inventory::InventoryMenu;
use crate::world::item::ItemStack;
use mc_protocol::components::{CUSTOM_DATA, CUSTOM_NAME, DataComponentMap};

/// `LightningBolt` as seen by `LightningBoltPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct LightningBoltState {
    /// `getBlocksSetOnFire()`.
    pub blocks_set_on_fire: i32,
    /// `getHitEntities()`.
    pub hit_entities: Vec<EntityId>,
}

/// `AdvancementProgress` as seen by `PlayerPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancementProgressSnapshot {
    /// `isDone()`.
    pub done: bool,
    /// Every criterion of the advancement with `CriterionProgress.isDone()`.
    pub criteria: Vec<(String, bool)>,
}

/// `Entity.get(type)` for the components every entity has: `CUSTOM_NAME` and `CUSTOM_DATA` (entity types have no
/// default components).
pub fn base_entity_components(base: &EntityData) -> DataComponentMap {
    let mut builder = DataComponentMap::builder();
    if let Some(name) = base.get_custom_name() {
        builder = builder.set(CUSTOM_NAME, name);
    }
    builder
        .set(
            CUSTOM_DATA,
            mc_protocol::components::CustomData::of(&base.custom_data),
        )
        .build()
}

/// `getEquipmentSlot(slot)` of `LivingEntity.getSlot`.
pub(crate) fn equipment_slot_of(slot: i32) -> Option<EquipmentSlot> {
    [
        (100 + EquipmentSlot::Head.get_index(), EquipmentSlot::Head),
        (100 + EquipmentSlot::Chest.get_index(), EquipmentSlot::Chest),
        (100 + EquipmentSlot::Legs.get_index(), EquipmentSlot::Legs),
        (100 + EquipmentSlot::Feet.get_index(), EquipmentSlot::Feet),
        (98, EquipmentSlot::MainHand),
        (99, EquipmentSlot::OffHand),
        (105, EquipmentSlot::Body),
        (106, EquipmentSlot::Saddle),
    ]
    .into_iter()
    .find(|(id, _)| *id == slot)
    .map(|(_, s)| s)
}

/// `Player.getSlot(slot).get()` / `LivingEntity.getSlot(slot).get()` / `ContainerEntity.getChestVehicleSlot(slot).get()` (chest
/// boats and carts, `container.N`); `Entity.getSlot` is `null`. A chest vehicle's loot table is unpacked by the callers that can.
pub fn get_slot(entity: &dyn Entity, slot: i32) -> Option<ItemStack> {
    if let Some(chest) = entity.chest_vehicle() {
        return usize::try_from(slot)
            .ok()
            .filter(|&s| s < chest.get_container_size())
            .map(|s| chest.get_item(s).clone());
    }
    if let Some(player) = entity.player() {
        if slot == 499 {
            return entity
                .as_any()
                .downcast_ref::<ServerPlayer>()
                .map(|p| p.player.menus.current().base().get_carried().clone());
        }
        let craft_slot = slot - 500;
        if (0..4).contains(&craft_slot) {
            return player
                .menus
                .inventory_menu()
                .as_any()
                .downcast_ref::<InventoryMenu>()
                .map(|m| m.get_craft_slots().get_item(craft_slot as usize).clone());
        }
        if let Some(stack) = usize::try_from(slot)
            .ok()
            .and_then(|s| player.inventory.get_non_equipment_items().get(s))
        {
            return Some(stack.clone());
        }
        if let Some(stack) = usize::try_from(slot - 200)
            .ok()
            .and_then(|s| player.ender_chest.get_items().get(s))
        {
            return Some(stack.clone());
        }
    }
    if entity.living().is_some() {
        return equipment_slot_of(slot).map(|s| entity.get_item_by_slot(s));
    }
    None
}

/// `SlotAccess.set`'s validator of the living entities' equipment slots (`LivingEntity.createEquipmentSlotAccess`): armor slots
/// below the head and the body/saddle slots only take items that go there (or nothing).
pub fn can_set_equipment_slot(entity: &dyn Entity, slot: i32, stack: &ItemStack) -> bool {
    if entity.living().is_none() {
        return true;
    }
    match equipment_slot_of(slot) {
        Some(equipment)
            if !matches!(
                equipment,
                EquipmentSlot::Head | EquipmentSlot::MainHand | EquipmentSlot::OffHand
            ) =>
        {
            // `getEquipmentSlotForItem` (the equippable's slot when the entity can use it, else the main hand)
            stack.is_empty() || entity.get_equipment_slot_for_item(stack) == equipment
        }
        _ => true,
    }
}

/// `getSlot(slot).set(stack)` of the entities that have no slots of their own beyond `get_slot`'s: `ServerPlayer` (carried item,
/// crafting grid, inventory, ender chest, equipment), other living entities (equipment) and chest vehicles (`container.N`).
pub fn set_slot(level: &mut ServerLevel, entity: &mut dyn Entity, slot: i32, stack: ItemStack) {
    if let Some(chest) = entity.chest_vehicle_mut() {
        // `setChestVehicleItem(slot, stack)` (the slot was checked against the container size by `getChestVehicleSlot`)
        if let Ok(slot) = usize::try_from(slot)
            && slot < chest.get_container_size()
        {
            chest.set_item(slot, stack);
        }
        return;
    }
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
fn set_player_slot(
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    slot: i32,
    stack: &ItemStack,
) -> bool {
    if slot == 499 {
        player
            .player
            .menus
            .current_mut()
            .base_mut()
            .set_carried(stack.clone());
        return true;
    }
    let craft_slot = slot - 500;
    if (0..4).contains(&craft_slot) {
        let menu = player
            .player
            .menus
            .inventory_menu_mut()
            .as_any_mut()
            .downcast_mut::<InventoryMenu>()
            .expect("inventory menu");
        menu.get_craft_slots_mut()
            .set_item(craft_slot as usize, stack.clone());
        player.inventory_menu_slots_changed(level);
        return true;
    }
    if slot >= 0 && (slot as usize) < player.player.inventory.get_non_equipment_items().len() {
        player
            .player
            .inventory
            .set_item(slot as usize, stack.clone());
        return true;
    }
    let ender_slot = slot - 200;
    if ender_slot >= 0 && (ender_slot as usize) < player.player.ender_chest.get_container_size() {
        player
            .player
            .ender_chest
            .set_item(ender_slot as usize, stack.clone());
        return true;
    }
    false
}

/// `LivingEntity.isFallFlying()`; `false` for non-living entities.
pub fn is_fall_flying(entity: &dyn Entity) -> bool {
    entity.living().is_some_and(|l| l.is_fall_flying())
}

/// `Entity.getOnPos()`.
pub fn get_on_pos(level: &ServerLevel, entity: &dyn Entity) -> mc_core::pos::BlockPos {
    movement::get_on_pos(entity, level, 1.0E-5)
}

/// `Entity.getBlockPosBelowThatAffectsMyMovement()`.
pub fn get_block_pos_below_that_affects_my_movement(
    level: &ServerLevel,
    entity: &dyn Entity,
) -> mc_core::pos::BlockPos {
    movement::get_block_pos_below_that_affects_my_movement(entity, level)
}

/// `Entity.getTeam().getName()`: the scoreboard team of the entity's scoreboard name.
pub fn get_team_name(level: &ServerLevel, entity: &dyn Entity) -> Option<String> {
    level
        .scoreboard()
        .get_players_team(&entity.get_scoreboard_name())
        .map(|t| t.get_name().to_owned())
}
