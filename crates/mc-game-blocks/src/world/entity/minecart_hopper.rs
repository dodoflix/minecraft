//! `net.minecraft.world.entity.vehicle.minecart.MinecartHopper`: a cart that sucks items in like a hopper block (from the
//! container or item entities above it, and from item entities around it) while it is enabled.

use crate::world::block_entity::hopper_block_entity::{self, Hopper};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, Item};
use mc_game_core::minecart_container_entity_methods;
use mc_game_core::minecart_entity_methods;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::Container;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::vehicle::container_entity::{
    ChestVehicleData, ChestVehicleMenu, ContainerEntity,
};
use mc_game_core::world::entity::vehicle::minecart::abstract_minecart::{
    self, AbstractMinecart, MinecartData,
};
use mc_game_core::world::entity::vehicle::minecart::abstract_minecart_container;
use mc_game_core::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use mc_game_core::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use mc_game_core::world::inventory::MenuPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_nbt::CompoundTag;
use std::any::Any;

/// `MinecartHopper.getContainerSize()`.
const CONTAINER_SIZE: usize = 5;

pub struct MinecartHopper {
    base: EntityData,
    minecart: MinecartData,
    chest: ChestVehicleData,
    enabled: bool,
    consumed_item_this_frame: bool,
}

impl MinecartHopper {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 1, |_| {});
        Self {
            base,
            minecart,
            chest: ChestVehicleData::new_minecart(id, CONTAINER_SIZE, ChestVehicleMenu::Hopper),
            enabled: true,
            consumed_item_this_frame: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// `tryConsumeItems()`.
    fn try_consume_items(&mut self, level: &mut ServerLevel) {
        if self.is_alive()
            && self.enabled
            && !self.consumed_item_this_frame
            && self.suck_in_items(level)
        {
            self.consumed_item_this_frame = true;
        }
    }

    /// `suckInItems()`: the hopper rules, then the item entities touching the cart.
    pub fn suck_in_items(&mut self, level: &mut ServerLevel) -> bool {
        if hopper_block_entity::suck_in_items(level, self) {
            return true;
        }
        let aabb = self.base.bounding_box().inflate(0.25, 0.0, 0.25);
        for id in
            level.get_entities_of_class(&aabb, |e| e.as_any().is::<ItemEntity>(), |e| e.is_alive())
        {
            if hopper_block_entity::absorb_item_entity(level, self, id) {
                return true;
            }
        }
        false
    }
}

impl Container for MinecartHopper {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.chest.get_container_size()
    }
    fn is_empty(&self) -> bool {
        self.chest.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.chest.get_item(slot)
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.chest.get_item_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        self.chest.remove_item(slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.chest.remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.chest.set_item(slot, stack);
    }
    fn clear_content(&mut self) {
        self.chest.clear_content();
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.chest.still_valid(player)
    }
}

impl Hopper for MinecartHopper {
    fn get_level_x(&self) -> f64 {
        self.base.x()
    }
    fn get_level_y(&self) -> f64 {
        self.base.y() + 0.5
    }
    fn get_level_z(&self) -> f64 {
        self.base.z()
    }
    fn is_grid_aligned(&self) -> bool {
        false
    }
    fn unpack_pending_loot(&mut self, level: &ServerLevel) {
        let position = self.base.position();
        self.chest.unpack_for_access(level, position);
    }
}

impl AbstractMinecart for MinecartHopper {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::HOPPER.default_state()
    }
    fn get_default_display_offset(&self) -> i32 {
        1
    }
    /// `activateMinecart`: a powered activator rail disables the hopper.
    fn activate_minecart(
        &mut self,
        _level: &mut ServerLevel,
        _x: i32,
        _y: i32,
        _z: i32,
        state: bool,
    ) {
        self.set_enabled(!state);
    }
    fn apply_natural_slowdown(&mut self, movement: Vec3) -> Vec3 {
        abstract_minecart_container::apply_natural_slowdown(self, movement)
    }
}

impl VehicleEntity for MinecartHopper {
    fn get_drop_item(&self) -> Item {
        Item::HOPPER_MINECART
    }
    fn destroy(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        abstract_minecart_container::destroy(self, level, source);
    }
}

impl ContainerEntity for MinecartHopper {
    fn chest(&self) -> &ChestVehicleData {
        &self.chest
    }
    fn chest_mut(&mut self) -> &mut ChestVehicleData {
        &mut self.chest
    }
}

impl Entity for MinecartHopper {
    minecart_entity_methods!();
    minecart_container_entity_methods!();

    /// `AbstractMinecartContainer.interact`: opens the menu, no game event, no piglin anger.
    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        self.interact_with_container_vehicle(level, player)
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::HOPPER_MINECART))
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.consumed_item_this_frame = false;
        abstract_minecart::tick(self, level);
        self.try_consume_items(level);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart_container::add_additional_save_data(self, out);
        out.put_boolean("Enabled", self.enabled);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        abstract_minecart_container::read_additional_save_data(self, level, input);
        self.enabled = input.get_boolean_or("Enabled", true);
    }
}

/// The scenario dump field of the hopper cart: `enabled`.
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static MINECART_HOPPER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 32,
    applies: |e| e.as_any().is::<MinecartHopper>(),
    fields: |e, _level, _ctx| {
        let hopper = e
            .as_any()
            .downcast_ref::<MinecartHopper>()
            .expect("applies");
        vec![("enabled".into(), u8::from(hopper.enabled).to_string())]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_HOPPER_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::HOPPER_MINECART,
    create: |ty, id, _registries| Box::new(MinecartHopper::new(ty, id)),
};
