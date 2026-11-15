//! `net.minecraft.world.entity.vehicle.ContainerEntity`: the chest part of chest boats and chest/hopper minecarts.
//!
//! [`ChestVehicleData`] holds `itemStacks` / `lootTable` / `lootTableSeed` and implements the `ContainerEntity` default methods
//! plus `Container` (what `ContainerKey::Entity` resolves through [`Entity::as_container_mut`]). [`ContainerEntity`] is the
//! trait of the entities embedding it; its defaults are the interface's `default` methods that need the entity itself.
//!
//! The vanilla getters unpack a pending loot table lazily (`unpackChestVehicleLootTable(null)` on every `getItem`...). A `&self` Rust
//! container cannot do that without the level, so the paths that have it unpack explicitly: opening the menu
//! ([`ChestVehicleMenuProvider`], with the player), dropping the contents on removal/destruction, a hopper reading or filling the
//! container, a comparator reading it through a detector rail, `clearContent`, `/item` before it reads or writes
//! `container.N` (`getChestVehicleSlot`, answered by `Entity::get_slot` through `chest_vehicle`), and a `slots` predicate, which
//! holds the vehicle by shared reference and unpacks through [`ChestVehicleData::unpack_shared`].

use crate::advancements::triggers::CriteriaTriggers;
use crate::server::level::ServerLevel;
use crate::server::level::block_break::anger_nearby_piglins;
use crate::world::container::{Container, empty_stack, helper};
use crate::world::containers::drop_item_stack;
use crate::world::damagesource::DamageSource;
use crate::world::damagesource::display_name::entity_display_name;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::inventory::{
    AbstractContainerMenu, ChestMenu, ContainerRef, HopperMenu, MenuPlayer, MenuProvider,
};
use crate::world::item::{InteractionResult, ItemStack};
use crate::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::GameEvent;
use mc_nbt::CompoundTag;
use mc_text::Component;
use std::any::Any;
use std::sync::OnceLock;

/// The menu a chest vehicle opens (`createMenu`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestVehicleMenu {
    /// `ChestMenu.threeRows` (chest boats, chest minecarts).
    ThreeRows,
    /// `HopperMenu` (hopper minecarts).
    Hopper,
}

/// Slots of the item list a minecart container starts with (`AbstractMinecartContainer.itemStacks`), whatever its container size.
const MINECART_INITIAL_STACKS: usize = 36;

/// `ContainerEntity.getItemStacks()` & the loot table fields of the implementing entity.
///
/// A `slots` predicate unpacks the loot table of a vehicle it only holds by shared reference ([`Self::unpack_shared`]): the
/// rolled stacks wait in `unpacked`, which every reader consults first and every `&mut` method settles into `item_stacks`.
pub struct ChestVehicleData {
    entity_id: EntityId,
    item_stacks: Vec<ItemStack>,
    container_size: usize,
    menu: ChestVehicleMenu,
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
    unpacked: OnceLock<Vec<ItemStack>>,
}

impl ChestVehicleData {
    /// `NonNullList.withSize(size, ItemStack.EMPTY)`; `entity_id` identifies the owner for `stillValid`.
    pub fn new(entity_id: EntityId, size: usize) -> Self {
        Self {
            entity_id,
            item_stacks: vec![ItemStack::empty(); size],
            container_size: size,
            menu: ChestVehicleMenu::ThreeRows,
            loot_table: None,
            loot_table_seed: 0,
            unpacked: OnceLock::new(),
        }
    }

    /// The state of an `AbstractMinecartContainer`: the item list starts with 36 slots (the list is only resized to
    /// `getContainerSize()` when the cart is loaded or cleared).
    pub fn new_minecart(
        entity_id: EntityId,
        container_size: usize,
        menu: ChestVehicleMenu,
    ) -> Self {
        Self {
            item_stacks: vec![ItemStack::empty(); MINECART_INITIAL_STACKS],
            container_size,
            menu,
            ..Self::new(entity_id, 0)
        }
    }

    /// Whether a loot table is waiting to be unpacked.
    pub fn has_loot_table(&self) -> bool {
        self.get_container_loot_table().is_some()
    }

    pub fn get_container_loot_table(&self) -> Option<&Identifier> {
        match self.unpacked.get() {
            Some(_) => None,
            None => self.loot_table.as_ref(),
        }
    }

    pub fn set_container_loot_table(&mut self, loot_table: Option<Identifier>) {
        self.settle();
        self.loot_table = loot_table;
    }

    pub fn get_container_loot_table_seed(&self) -> i64 {
        self.loot_table_seed
    }

    pub fn set_container_loot_table_seed(&mut self, seed: i64) {
        self.loot_table_seed = seed;
    }

    pub fn get_item_stacks(&self) -> &[ItemStack] {
        self.unpacked.get().unwrap_or(&self.item_stacks)
    }

    /// Moves the stacks a shared unpack rolled into the item list.
    fn settle(&mut self) {
        if let Some(stacks) = self.unpacked.take() {
            self.item_stacks = stacks;
            self.loot_table = None;
        }
    }

    /// `clearItemStacks()`: a fresh list of empty stacks of the container size.
    pub fn clear_item_stacks(&mut self) {
        self.settle();
        self.item_stacks = vec![ItemStack::empty(); self.container_size];
    }

    /// `addChestVehicleSaveData(output)`: the loot table (and its seed when non-zero) replaces the items.
    pub fn add_chest_vehicle_save_data(&self, out: &mut CompoundTag) {
        if let Some(table) = self.get_container_loot_table() {
            out.put_string("LootTable", table.to_string());
            if self.loot_table_seed != 0 {
                out.put_long("LootTableSeed", self.loot_table_seed);
            }
        } else if let Some(items) = helper::save_all_items(self.get_item_stacks(), true) {
            out.put("Items", mc_nbt::Tag::List(items));
        }
    }

    /// `readChestVehicleSaveData(input)`.
    pub fn read_chest_vehicle_save_data(&mut self, input: &CompoundTag) {
        self.clear_item_stacks();
        self.loot_table = input
            .get_string("LootTable")
            .and_then(|s| Identifier::read(s).ok());
        self.loot_table_seed = input.get_long_or("LootTableSeed", 0);
        if self.loot_table.is_none() {
            helper::load_all_items(input, &mut self.item_stacks);
        }
    }

    /// `unpackChestVehicleLootTable(player)` at `origin` (the entity's position): `GENERATE_LOOT`, then the fill with the
    /// player's luck.
    pub fn unpack_chest_vehicle_loot_table(
        &mut self,
        level: &mut ServerLevel,
        origin: Vec3,
        player: &mut ServerPlayer,
    ) {
        self.settle();
        if let Some(table) = &self.loot_table {
            CriteriaTriggers::GENERATE_LOOT.trigger(level, player, table);
        }
        self.fill_from_loot_table(level, origin, Some(&*player));
    }

    /// `unpackChestVehicleLootTable(null)`: the lazy unpack of the container getters.
    pub fn unpack_for_access(&mut self, level: &ServerLevel, origin: Vec3) {
        self.settle();
        self.fill_from_loot_table(level, origin, None);
    }

    /// `unpackChestVehicleLootTable(null)` for a vehicle held by shared reference (a `slots` predicate reading `container.N`):
    /// the roll happens now, on a copy of the item list, and its result becomes the vehicle's state at its next `&mut` access.
    pub fn unpack_shared(&self, level: &ServerLevel, origin: Vec3) {
        if !self.has_loot_table() {
            return;
        }
        let mut rolled = Self {
            item_stacks: self.item_stacks.clone(),
            loot_table: self.loot_table.clone(),
            unpacked: OnceLock::new(),
            ..Self::new(self.entity_id, 0)
        };
        rolled.container_size = self.container_size;
        rolled.menu = self.menu;
        rolled.loot_table_seed = self.loot_table_seed;
        rolled.fill_from_loot_table(level, origin, None);
        let _ = self.unpacked.set(rolled.item_stacks);
    }

    fn fill_from_loot_table(
        &mut self,
        level: &ServerLevel,
        origin: Vec3,
        player: Option<&dyn Entity>,
    ) {
        let Some(table_id) = self.loot_table.take() else {
            return;
        };
        let table = loading::get_loot_table(&level.registries, &table_id);
        let seed = self.loot_table_seed;
        let mut builder = LootParams::builder(level).origin(origin);
        if let Some(p) = player {
            let luck = p.living().map_or(0.0, |l| {
                l.get_attribute_value(mc_data::Attribute::LUCK) as f32
            });
            builder = builder.luck(luck).this_entity(p);
        }
        let params = builder.create(&LootContextParamSets::CHEST);
        table.fill(self, &params, seed);
    }

    /// `Containers.dropContents(level, entity, container)` with `getItem` unpacking first: one drop per slot of the container
    /// size; the stacks are consumed like `dropItemStack` splits them.
    pub fn drop_contents(&mut self, level: &mut ServerLevel, position: Vec3) {
        self.unpack_for_access(level, position);
        for stack in self.item_stacks.iter_mut().take(self.container_size) {
            drop_item_stack(
                level,
                position.x,
                position.y,
                position.z,
                std::mem::take(stack),
            );
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
        self.container_size
    }
    /// `isChestVehicleEmpty()`.
    fn is_empty(&self) -> bool {
        self.get_item_stacks().iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.get_item_stacks()
            .get(slot)
            .unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.settle();
        self.item_stacks.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        self.settle();
        helper::remove_item(&mut self.item_stacks, slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.settle();
        helper::take_item(&mut self.item_stacks, slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        self.settle();
        stack.limit_size(self.get_max_stack_size_for(&stack));
        if let Some(s) = self.item_stacks.get_mut(slot) {
            *s = stack;
        }
    }
    /// `clearChestVehicleContent()`.
    fn clear_content(&mut self) {
        self.settle();
        self.loot_table = None;
        self.item_stacks
            .iter_mut()
            .for_each(|s| *s = ItemStack::empty());
    }
    fn set_changed(&mut self) {}
    /// `isChestVehicleStillValid(player)`: the vehicle is alive and within interaction range (buffer 4).
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        player
            .level_ref()
            .and_then(|l| l.get_entity(self.entity_id))
            .is_some_and(|e| {
                !e.is_removed()
                    && player.is_within_entity_interaction_range(&e.get_bounding_box(), 4.0)
            })
    }
}

/// The menu a chest vehicle opens (`MenuProvider.createMenu`: unpack the loot table for the player, then the chest or hopper menu).
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

    fn create_menu(
        &self,
        container_id: i32,
        player: &mut dyn MenuPlayer,
    ) -> Option<Box<dyn AbstractContainerMenu>> {
        let id = self.entity_id;
        let chest = |player: &dyn MenuPlayer| {
            player
                .level_ref()
                .and_then(|l| l.get_entity(id))
                .and_then(|e| e.chest_vehicle())
                .map(|c| (c.has_loot_table(), c.container_size, c.menu))
        };
        let (loot_pending, size, menu) = chest(&*player)?;
        if loot_pending && player.is_spectator() {
            return None;
        }
        player.with_server_player(&mut |level, server_player| {
            level.with_entity(id, |level, e| {
                let position = e.position();
                if let Some(chest) = e.chest_vehicle_mut() {
                    chest.unpack_chest_vehicle_loot_table(level, position, server_player);
                }
            });
        });
        let container = ContainerRef::entity(id, size);
        Some(match menu {
            ChestVehicleMenu::ThreeRows => {
                Box::new(ChestMenu::three_rows(container_id, player, container))
            }
            ChestVehicleMenu::Hopper => Box::new(HopperMenu::new(container_id, player, container)),
        })
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
            if let Some(player) = source
                .get_direct_entity()
                .filter(|_| source.direct_is_player())
            {
                level.with_entity(player, |level, player| {
                    anger_nearby_piglins(level, &*player, true);
                });
            }
        }
    }

    /// What chest boats and chest minecarts do when the player uses them: `interactWithContainerVehicle`, then on success the
    /// `CONTAINER_OPEN` game event at the vehicle and the anger of the piglins that see the player.
    fn open_chest_vehicle(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let result = self.interact_with_container_vehicle(level, player);
        if result.consumes_action() {
            level.game_event_from_entity_ref(
                &*player,
                GameEvent::CONTAINER_OPEN,
                self.base().position(),
            );
            anger_nearby_piglins(level, &*player, true);
        }
        result
    }

    /// `interactWithContainerVehicle(player)`: `player.openMenu(this)`. The vehicle is out of the level while it is being
    /// interacted with, so it is lent for the menu, which addresses it through the level.
    fn interact_with_container_vehicle(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
    ) -> InteractionResult {
        let id = self.id();
        let title = entity_display_name(level, self.as_dyn());
        level.lend(self.as_dyn_mut(), |level| {
            player.open_menu(level, &ChestVehicleMenuProvider::new(id, title));
        });
        InteractionResult::SUCCESS
    }
}
