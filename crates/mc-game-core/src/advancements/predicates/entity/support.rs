//! Entity accessors the predicates need (vanilla's `Entity`/`LivingEntity`/`Player` methods that have no counterpart on
//! our traits) and the data types of the `Entity` hooks.

use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::movement;
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::inventory::InventoryMenu;
use crate::world::item::ItemStack;
use mc_core::phys::Vec3;
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

/// `Player.getSlot(slot).get()` / `LivingEntity.getSlot(slot).get()`; `Entity.getSlot` is `null`.
pub fn get_slot(entity: &dyn Entity, slot: i32) -> Option<ItemStack> {
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

/// `LivingEntity.isFallFlying()`; `false` for non-living entities.
pub fn is_fall_flying(entity: &dyn Entity) -> bool {
    entity.living().is_some_and(|l| l.is_fall_flying())
}

/// The entity with `id`: one of the `lent` entities (running entities are not in the level's storage) or the level's.
pub fn resolve<'a>(
    level: &'a ServerLevel,
    lent: &[&'a dyn Entity],
    id: EntityId,
) -> Option<&'a dyn Entity> {
    lent.iter()
        .copied()
        .find(|e| e.id() == id)
        .or_else(|| level.get_entity(id))
}

/// `Entity.getVehicle()`.
pub fn get_vehicle<'a>(
    level: &'a ServerLevel,
    lent: &[&'a dyn Entity],
    entity: &dyn Entity,
) -> Option<&'a dyn Entity> {
    entity
        .base()
        .vehicle
        .and_then(|id| resolve(level, lent, id))
}

/// `Entity.getKnownMovement()` (`ServerPlayer` overrides it with the last client movement).
// shortcut: a vehicle that is the entity currently being ticked (taken out of the level) is not found; upgrade when the
// level can hand out the running entity.
pub fn get_known_movement(level: &ServerLevel, entity: &dyn Entity) -> Vec3 {
    if let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() {
        return match get_vehicle(level, &[], entity) {
            Some(vehicle) if vehicle.get_controlling_passenger() != Some(entity.id()) => {
                get_known_movement(level, vehicle)
            }
            _ => player.last_known_client_movement,
        };
    }
    entity.get_known_movement(level)
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
