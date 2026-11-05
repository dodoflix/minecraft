//! `net.minecraft.world.level.block.EndGatewayBlock` (a `Portal`: teleports within the End to the gateway's exit).
use super::prelude::*;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::players::dimension_change::{PostTeleportTransition, TeleportTransition};
use crate::world::block_entity::TheEndGatewayBlockEntity;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::InsideBlockEffectApplier;
use crate::world::entity::portal::Portal;
use mc_core::phys::Vec3;
use mc_protocol::packets::play::Relatives;

/// The instance entities register as their `Portal` (non-zero-sized: portals are compared by address).
pub struct EndGatewayBlock {
    _unique: u8,
}

pub static END_GATEWAY_BLOCK: EndGatewayBlock = EndGatewayBlock { _unique: 1 };

impl BlockBehaviour for EndGatewayBlock {
    fn entity_inside(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, _effect_applier: &mut dyn InsideBlockEffectApplier, _is_precise: bool) {
        if !entity.can_use_portal(false) {
            return;
        }
        let triggered = level
            .with_block_entity(pos, |level, be| match be.as_any_mut().downcast_mut::<TheEndGatewayBlockEntity>() {
                Some(gateway) if !gateway.is_cooling_down() => {
                    gateway.trigger_cooldown(level);
                    true
                }
                _ => false,
            })
            .unwrap_or(false);
        if triggered {
            entity.set_as_inside_portal(level, &END_GATEWAY_BLOCK, pos);
        }
    }

    /// `BaseEntityBlock.triggerEvent` -> `TheEndGatewayBlockEntity.triggerEvent`.
    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, _b1: i32) -> bool {
        if b0 != 1 {
            return false;
        }
        level
            .with_block_entity(pos, |_, be| {
                be.as_any_mut().downcast_mut::<TheEndGatewayBlockEntity>().map(|g| g.teleport_cooldown = 40).is_some()
            })
            .unwrap_or(false)
    }

    fn as_portal(&self) -> Option<&'static dyn Portal> {
        Some(&END_GATEWAY_BLOCK)
    }

    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> crate::world::item::ItemStack {
        crate::world::item::ItemStack::empty()
    }

    fn can_be_replaced_by_fluid(&self, _state: BlockState, _fluid: mc_data::fluid::Fluid) -> bool {
        false
    }
}

impl Portal for EndGatewayBlock {
    fn get_portal_destination(&self, _server: &mut MinecraftServer, current_level: &mut ServerLevel, entity: &mut dyn Entity, portal_entry_pos: BlockPos) -> Option<TeleportTransition> {
        let teleport_position = current_level.with_block_entity(portal_entry_pos, |level, be| {
            be.as_any_mut().downcast_mut::<TheEndGatewayBlockEntity>().map(|g| g.get_portal_position(level, portal_entry_pos))
        })??;
        let teleport_position: Vec3 = teleport_position?;
        let relatives = if entity.get_type() == mc_data::EntityType::ENDER_PEARL { Relatives::NONE } else { Relatives::DELTA.union(Relatives::ROTATION) };
        Some(TeleportTransition::new(current_level.dimension.clone(), teleport_position, Vec3::ZERO, 0.0, 0.0, PostTeleportTransition::PLACE_PORTAL_TICKET).with_relatives(relatives))
    }
}

register_block_class!("EndGatewayBlock", |_| Box::new(EndGatewayBlock { _unique: 1 }));
