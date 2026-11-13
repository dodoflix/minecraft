//! `net.minecraft.world.entity.vehicle.minecart.MinecartSpawner`: a cart that runs a [`BaseSpawner`] at its block position.

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::base_spawner::{BaseSpawner, BaseSpawnerHost};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

/// The anonymous `BaseSpawner` subclass of `MinecartSpawner`: events go to the cart's watchers.
struct CartSpawnerHost(EntityId);

impl BaseSpawnerHost for CartSpawnerHost {
    fn broadcast_event(&self, level: &mut ServerLevel, _pos: BlockPos, id: i32) {
        level.broadcast_entity_event(self.0, id as i8);
    }
}

pub struct MinecartSpawner {
    base: EntityData,
    minecart: MinecartData,
    /// Out of the cart only while its tick runs the spawner (the cart is exposed to the spawner's queries meanwhile).
    spawner: Option<BaseSpawner>,
}

impl MinecartSpawner {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 6, |_| {});
        Self {
            base,
            minecart,
            spawner: Some(BaseSpawner::new(Box::new(CartSpawnerHost(id)))),
        }
    }

    /// `getSpawner()`.
    pub fn get_spawner(&mut self) -> &mut BaseSpawner {
        self.spawner
            .as_mut()
            .expect("the spawner is back after the tick")
    }
}

impl AbstractMinecart for MinecartSpawner {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn get_default_display_block_state(&self) -> BlockState {
        Block::SPAWNER.default_state()
    }
}

impl VehicleEntity for MinecartSpawner {
    fn get_drop_item(&self) -> Item {
        Item::MINECART
    }
}

impl Entity for MinecartSpawner {
    minecart_entity_methods!();

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
        let pos = self.base.block_position();
        let mut spawner = self
            .spawner
            .take()
            .expect("the spawner is back after the tick");
        // vanilla's spawn collision and nearby-entity queries see the cart itself
        level.expose(self, |level| spawner.server_tick(level, pos));
        self.spawner = Some(spawner);
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::MINECART))
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart::add_additional_save_data(self, out);
        self.spawner
            .as_ref()
            .expect("the spawner is back after the tick")
            .save(out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_minecart::read_additional_save_data(self, input);
        self.get_spawner().load(input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_SPAWNER_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::SPAWNER_MINECART,
    create: |ty, id, _registries| Box::new(MinecartSpawner::new(ty, id)),
};
