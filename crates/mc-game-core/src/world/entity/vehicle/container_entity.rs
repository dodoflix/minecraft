//! `net.minecraft.world.entity.vehicle.ContainerEntity`: the chest part of chest boats and chest/hopper minecarts.
//!
//! [`ChestVehicleData`] holds `itemStacks` / `lootTable` / `lootTableSeed` and implements the `ContainerEntity` default methods
//! plus `Container` (what `ContainerKey::Entity` resolves through [`Entity::as_container_mut`]). [`ContainerEntity`] is the
//! trait of the entities embedding it; its defaults are the interface's `default` methods that need the entity itself.
//!
//! The vanilla getters unpack a pending loot table lazily (`unpackChestVehicleLootTable(null)` on every `getItem`...). A `&self`
//! container cannot do that without the level, so the paths that have it unpack explicitly: opening the menu
//! ([`ChestVehicleMenuProvider`], with the player), dropping the contents on removal/destruction, `clearContent`.

use crate::server::level::ServerLevel;
use crate::world::container::{Container, empty_stack, helper};
use crate::world::damagesource::DamageSource;
use crate::world::damagesource::display_name::entity_display_name;
use crate::world::containers::drop_item_stack;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::inventory::{AbstractContainerMenu, ChestMenu, ContainerRef, MenuPlayer, MenuProvider};
use crate::world::item::{InteractionResult, ItemStack};
use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_nbt::CompoundTag;
use mc_text::Component;
use std::any::Any;

/// `ContainerEntity.getItemStacks()` & the loot table fields of the implementing entity.
pub struct ChestVehicleData {
    entity_id: EntityId,
    item_stacks: Vec<ItemStack>,
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
}

impl ChestVehicleData {
    /// `NonNullList.withSize(size, ItemStack.EMPTY)`; `entity_id` identifies the owner for `stillValid`.
    pub fn new(entity_id: EntityId, size: usize) -> Self {
        Self { entity_id, item_stacks: vec![ItemStack::empty(); size], loot_table: None, loot_table_seed: 0 }
    }

    pub fn get_container_loot_table(&self) -> Option<&Identifier> {
        self.loot_table.as_ref()
    }

    pub fn set_container_loot_table(&mut self, loot_table: Option<Identifier>) {
        self.loot_table = loot_table;
    }

    pub fn get_container_loot_table_seed(&self) -> i64 {
        self.loot_table_seed
    }

    pub fn set_container_loot_table_seed(&mut self, seed: i64) {
        self.loot_table_seed = seed;
    }

    pub fn get_item_stacks(&self) -> &[ItemStack] {
        &self.item_stacks
    }

    /// `clearItemStacks()`: a fresh list of empty stacks of the same size.
    pub fn clear_item_stacks(&mut self) {
        let size = self.item_stacks.len();
        self.item_stacks = vec![ItemStack::empty(); size];
    }

    /// `addChestVehicleSaveData(output)`: the loot table (and its seed when non-zero) replaces the items.
    pub fn add_chest_vehicle_save_data(&self, out: &mut CompoundTag) {
        if let Some(table) = &self.loot_table {
            out.put_string("LootTable", table.to_string());
            if self.loot_table_seed != 0 {
                out.put_long("LootTableSeed", self.loot_table_seed);
            }
        } else if let Some(items) = helper::save_all_items(&self.item_stacks, true) {
            out.put("Items", mc_nbt::Tag::List(items));
        }
    }

    /// `readChestVehicleSaveData(input)`.
    pub fn read_chest_vehicle_save_data(&mut self, input: &CompoundTag) {
        self.clear_item_stacks();
        self.loot_table = input.get_string("LootTable").and_then(|s| Identifier::read(s).ok());
        self.loot_table_seed = input.get_long_or("LootTableSeed", 0);
        if self.loot_table.is_none() {
            helper::load_all_items(input, &mut self.item_stacks);
        }
    }

    /// `unpackChestVehicleLootTable(player)` at `origin` (the entity's position).
    pub fn unpack_chest_vehicle_loot_table(&mut self, level: &ServerLevel, origin: Vec3, player: Option<&dyn Entity>) {
        let Some(table_id) = self.loot_table.take() else { return };
        let table = loading::get_loot_table(&level.registries, &table_id);
        // SEAM(ImAdvancements): `CriteriaTriggers.GENERATE_LOOT.trigger(player, lootTable)` when a player opened it
        let seed = self.loot_table_seed;
        let mut builder = LootParams::builder(level).origin(origin);
        if let Some(p) = player {
            let luck = p.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::LUCK) as f32);
            builder = builder.luck(luck).this_entity(p);
        }
        let params = builder.create(&LootContextParamSets::CHEST);
        table.fill(self, &params, seed);
    }

    /// `Containers.dropContents(level, entity, container)` with `getItem` unpacking first; the stacks are consumed like
    /// `dropItemStack` splits them.
    pub fn drop_contents(&mut self, level: &mut ServerLevel, position: Vec3) {
        self.unpack_chest_vehicle_loot_table(level, position, None);
        for stack in &mut self.item_stacks {
            drop_item_stack(level, position.x, position.y, position.z, std::mem::take(stack));
        }
    }
}

impl Container for ChestVehicleData {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.item_stacks.len()
    }
    /// `isChestVehicleEmpty()`.
    fn is_empty(&self) -> bool {
        self.item_stacks.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.item_stacks.get(slot).unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.item_stacks.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        helper::remove_item(&mut self.item_stacks, slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.item_stacks, slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        stack.limit_size(self.get_max_stack_size_for(&stack));
        if let Some(s) = self.item_stacks.get_mut(slot) {
            *s = stack;
        }
    }
    /// `clearChestVehicleContent()`.
    fn clear_content(&mut self) {
        self.loot_table = None;
        self.item_stacks.iter_mut().for_each(|s| *s = ItemStack::empty());
    }
    fn set_changed(&mut self) {}
    /// `isChestVehicleStillValid(player)`: the vehicle is alive and within interaction range (buffer 4).
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        player
            .level_ref()
            .and_then(|l| l.get_entity(self.entity_id))
            .is_some_and(|e| !e.is_removed() && player.is_within_entity_interaction_range(&e.get_bounding_box(), 4.0))
    }
}

/// The menu a chest vehicle opens (`MenuProvider.createMenu`: unpack the loot table for the player, `ChestMenu.threeRows`).
pub struct ChestVehicleMenuProvider {
    entity_id: EntityId,
    title: Component,
}

impl ChestVehicleMenuProvider {
    pub fn new(entity_id: EntityId, title: Component) -> Self {
        Self { entity_id, title }
    }
}

impl MenuProvider for ChestVehicleMenuProvider {
    fn get_display_name(&self) -> Component {
        self.title.clone()
    }

    fn create_menu(&self, container_id: i32, player: &mut dyn MenuPlayer) -> Option<Box<dyn AbstractContainerMenu>> {
        let id = self.entity_id;
        let loot_pending = player.level_ref().and_then(|l| l.get_entity(id)).and_then(|e| e.chest_vehicle()).is_some_and(|c| c.loot_table.is_some());
        if loot_pending && player.is_spectator() {
            return None;
        }
        player.with_server_player(&mut |level, server_player| {
            level.with_entity(id, |level, e| {
                let position = e.position();
                if let Some(chest) = e.chest_vehicle_mut() {
                    chest.unpack_chest_vehicle_loot_table(level, position, Some(&*server_player));
                }
            });
        });
        let size = player.level_ref().and_then(|l| l.get_entity(id)).and_then(|e| e.chest_vehicle()).map_or(0, |c| c.get_container_size());
        Some(Box::new(ChestMenu::three_rows(container_id, player, ContainerRef::entity(id, size))))
    }
}

/// `ContainerEntity` for an entity that embeds a [`ChestVehicleData`].
pub trait ContainerEntity: Entity {
    fn chest(&self) -> &ChestVehicleData;
    fn chest_mut(&mut self) -> &mut ChestVehicleData;

    /// `Containers.dropContents(level, this, this)`.
    fn drop_chest_contents(&mut self, level: &mut ServerLevel) {
        let position = self.base().position();
        self.chest_mut().drop_contents(level, position);
    }

    /// `chestVehicleDestroyed(source, level, entity)`.
    fn chest_vehicle_destroyed(&mut self, source: &DamageSource, level: &mut ServerLevel) {
        if level.entity_drops() {
            self.drop_chest_contents(level);
            if source.direct_is_player() {
                // SEAM(MobsScope): `PiglinAi.angerNearbyPiglins(level, player, true)`
            }
        }
    }

    /// `interactWithContainerVehicle(player)`: `player.openMenu(this)`. The vehicle is out of the level while it is being
    /// interacted with, so the menu (which addresses it through the level) opens when the player is put back.
    fn interact_with_container_vehicle(&mut self, level: &mut ServerLevel, player: &ServerPlayer) -> InteractionResult {
        open_chest_vehicle_menu(level, player.id(), self.id());
        InteractionResult::SUCCESS
    }
}

/// `player.openMenu(vehicle)`: queued for `player_id` (runs at once when the player is in the level).
pub fn open_chest_vehicle_menu(level: &mut ServerLevel, player_id: EntityId, vehicle_id: EntityId) {
    level.with_entity_or_defer(player_id, move |level, player| {
        let Some(title) = level.get_entity(vehicle_id).map(|v| entity_display_name(level, v)) else { return };
        if let Some(player) = player.as_any_mut().downcast_mut::<ServerPlayer>() {
            player.open_menu(level, &ChestVehicleMenuProvider::new(vehicle_id, title));
        }
    });
}
