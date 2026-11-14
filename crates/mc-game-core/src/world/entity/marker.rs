//! `net.minecraft.world.entity.Marker`: an inert entity with a position and nothing else. It has no tracking range, so no
//! client is ever told about it.

use super::factory::{ENTITY_FACTORIES, EntityRegistration};
use super::{Entity, EntityData, EntityId, entity};
use crate::server::level::ServerLevel;
use linkme::distributed_slice;
use mc_data::EntityType;
use mc_data::enums::PushReaction;
use mc_nbt::CompoundTag;
use std::any::Any;

pub struct Marker {
    base: EntityData,
}

impl Marker {
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        base.no_physics = true;
        Self { base }
    }
}

impl Entity for Marker {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }

    /// Empty: a marker takes no `baseTick`.
    fn tick(&mut self, _level: &mut ServerLevel) {}

    /// Unreachable through the tracker: the type's client tracking range is 0, so a marker is never paired with a player.
    fn get_add_entity_packet(
        &self,
        _server_entity: &crate::server::level::ServerEntity,
    ) -> mc_protocol::packets::play::ClientboundAddEntityPacket {
        panic!("Markers should never be sent");
    }

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        false
    }

    fn could_accept_passenger(&self) -> bool {
        false
    }

    fn add_passenger(&mut self, _passenger: &dyn Entity, _first_passenger_is_player: bool) {
        panic!("Should never addPassenger without checking couldAcceptPassenger()");
    }

    fn get_piston_push_reaction(&self) -> PushReaction {
        PushReaction::IgnoreEntity
    }

    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    fn hurt_server(
        &mut self,
        _level: &mut ServerLevel,
        _source: &crate::world::damagesource::DamageSource,
        _amount: f32,
    ) -> bool {
        false
    }

    fn add_additional_save_data(&self, _out: &mut CompoundTag) {}

    fn read_additional_save_data(&mut self, _level: Option<&ServerLevel>, _input: &CompoundTag) {}
}

#[distributed_slice(ENTITY_FACTORIES)]
static MARKER_FACTORY: EntityRegistration = EntityRegistration {
    entity_type: EntityType::MARKER,
    create: |ty, id, _registries| Box::new(Marker::new(ty, id)),
};
