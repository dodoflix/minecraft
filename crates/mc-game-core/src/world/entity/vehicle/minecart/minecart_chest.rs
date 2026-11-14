//! `net.minecraft.world.entity.vehicle.minecart.MinecartChest`.

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use super::abstract_minecart_container;
use crate::minecart_container_entity_methods;
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use crate::world::entity::vehicle::container_entity::{
    ChestVehicleData, ChestVehicleMenu, ContainerEntity,
};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, GameEvent, Item};
use mc_nbt::CompoundTag;

/// `MinecartChest.getContainerSize()`.
const CONTAINER_SIZE: usize = 27;

pub struct MinecartChest {
    base: EntityData,
    minecart: MinecartData,
    chest: ChestVehicleData,
}

impl MinecartChest {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 8, |_| {});
        Self {
            base,
            minecart,
            chest: ChestVehicleData::new_minecart(id, CONTAINER_SIZE, ChestVehicleMenu::ThreeRows),
        }
    }
}

impl AbstractMinecart for MinecartChest {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::CHEST.default_state()
    }
    fn get_default_display_offset(&self) -> i32 {
        8
    }
    fn apply_natural_slowdown(&mut self, movement: Vec3) -> Vec3 {
        abstract_minecart_container::apply_natural_slowdown(self, movement)
    }
}

impl VehicleEntity for MinecartChest {
    fn get_drop_item(&self) -> Item {
        Item::CHEST_MINECART
    }
    fn destroy(&mut self, level: &mut ServerLevel, source: &DamageSource) {
        abstract_minecart_container::destroy(self, level, source);
    }
}

impl ContainerEntity for MinecartChest {
    fn chest(&self) -> &ChestVehicleData {
        &self.chest
    }
    fn chest_mut(&mut self) -> &mut ChestVehicleData {
        &mut self.chest
    }
}

impl Entity for MinecartChest {
    minecart_entity_methods!();
    minecart_container_entity_methods!();

    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        self.open_chest_vehicle(level, player)
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::CHEST_MINECART))
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart_container::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        abstract_minecart_container::read_additional_save_data(self, level, input);
    }

    /// `MinecartChest.stopOpen(user)`: `CONTAINER_CLOSE` at the cart (the open event is posted by `interact`).
    fn container_open_close(&mut self, level: &mut ServerLevel, user: EntityId, open: bool) {
        if !open {
            level.game_event_from_entity(
                Some(user),
                GameEvent::CONTAINER_CLOSE,
                self.base.position(),
            );
        }
    }
}

/// The scenario dump fields of the container carts (the items are read without unpacking a pending loot table).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static MINECART_CONTAINER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 31,
    applies: |e| e.chest_vehicle().is_some() && e.as_abstract_minecart().is_some(),
    fields: |e, _level, _ctx| {
        super::abstract_minecart_container::dump_fields(e.chest_vehicle().expect("applies"))
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_CHEST_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::CHEST_MINECART,
    create: |ty, id, _registries| Box::new(MinecartChest::new(ty, id)),
};
