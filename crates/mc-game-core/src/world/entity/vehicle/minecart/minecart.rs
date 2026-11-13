//! `net.minecraft.world.entity.vehicle.minecart.Minecart` (the rideable cart).

use super::abstract_minecart::{self, AbstractMinecart, MinecartData};
use crate::minecart_entity_methods;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::vehicle::vehicle_entity::{self, VehicleEntity};
use crate::world::entity::{Entity, EntityData, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_data::{EntityType, Item};
use mc_nbt::CompoundTag;

pub struct Minecart {
    base: EntityData,
    minecart: MinecartData,
}

impl Minecart {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let (base, minecart) = abstract_minecart::new_base(ty, id, 6, |_| {});
        Self { base, minecart }
    }
}

impl AbstractMinecart for Minecart {
    fn minecart(&self) -> &MinecartData {
        &self.minecart
    }
    fn minecart_mut(&mut self) -> &mut MinecartData {
        &mut self.minecart
    }
    fn is_rideable(&self) -> bool {
        true
    }
    /// `activateMinecart`: a powered activator rail ejects the passenger and shakes the cart.
    fn activate_minecart(
        &mut self,
        level: &mut ServerLevel,
        _x: i32,
        _y: i32,
        _z: i32,
        state: bool,
    ) {
        if state {
            if self.base.is_vehicle() {
                level.eject_passengers(self);
            }
            if vehicle_entity::get_hurt_time(&self.base) == 0 {
                let dir = vehicle_entity::get_hurt_dir(&self.base);
                vehicle_entity::set_hurt_dir(&mut self.base, -dir);
                vehicle_entity::set_hurt_time(&mut self.base, 10);
                vehicle_entity::set_damage(&mut self.base, 50.0);
                self.mark_hurt();
            }
        }
    }
}

impl VehicleEntity for Minecart {
    fn get_drop_item(&self) -> Item {
        Item::MINECART
    }
}

impl Entity for Minecart {
    minecart_entity_methods!();

    fn interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _location: Vec3,
    ) -> InteractionResult {
        if !player.is_shift_key_down() && !self.base.is_vehicle() {
            if level.start_riding_held(player, self, false, true) {
                InteractionResult::SUCCESS_SERVER
            } else {
                InteractionResult::CONSUME
            }
        } else {
            InteractionResult::PASS
        }
    }

    fn get_pick_result(&self) -> Option<ItemStack> {
        Some(ItemStack::new(Item::MINECART))
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        abstract_minecart::tick(self, level);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        vehicle_entity::hurt_server(self, level, source, amount)
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        abstract_minecart::add_additional_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        abstract_minecart::read_additional_save_data(self, input);
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static MINECART_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::MINECART,
    create: |ty, id, _registries| Box::new(Minecart::new(ty, id)),
};
