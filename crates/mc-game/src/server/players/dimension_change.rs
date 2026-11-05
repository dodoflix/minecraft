//! `Entity.teleport(TeleportTransition)` and `ServerPlayer.teleport(TeleportTransition)`: same-dimension moves and
//! the cross-dimension hand-over between `ServerLevel`s (`ClientboundRespawn`, level info resend, tracker/chunk-map
//! re-registration through the level's entity events).
//!
//! Cross-level moves need both levels, so they are free functions over `&mut MinecraftServer` that take the entity
//! out of its level exactly like [`super::respawn::respawn`] does.

use super::join::{abilities_packet, create_common_spawn_info, send_level_info};
use super::server_join_host::{ServerJoinHost, level_join_info};
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::world::entity::entity::{Entity, position_move_rotation_of};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{EntityId, RemovalReason, ServerPlayer};
use crate::world::level::DimensionKey;
use mc_core::phys::Vec3;
use mc_data::enums::EntitySpawnReason;
use mc_protocol::packets::play::{
    ClientboundChangeDifficultyPacket, ClientboundGameEventPacket, ClientboundRespawnPacket, ClientboundSetHeldSlotPacket, ClientboundTeleportEntityPacket,
    PositionMoveRotation, Relative, Relatives,
};

/// `TeleportTransition` (the post-teleport callback is the caller's: run it on the returned entity).
#[derive(Clone, Debug)]
pub struct TeleportTransition {
    pub level: DimensionKey,
    pub position: Vec3,
    pub delta_movement: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub missing_respawn_block: bool,
    pub as_passenger: bool,
    pub relatives: Relatives,
}

impl TeleportTransition {
    pub fn new(level: DimensionKey, position: Vec3, delta_movement: Vec3, y_rot: f32, x_rot: f32) -> Self {
        Self { level, position, delta_movement, y_rot, x_rot, missing_respawn_block: false, as_passenger: false, relatives: Relatives::NONE }
    }

    pub fn with_relatives(mut self, relatives: Relatives) -> Self {
        self.relatives = relatives;
        self
    }

    /// `PositionMoveRotation.of(transition)`.
    pub fn position_move_rotation(&self) -> PositionMoveRotation {
        PositionMoveRotation { position: self.position, delta_movement: self.delta_movement, y_rot: self.y_rot, x_rot: self.x_rot }
    }

    fn for_passenger(&self, vehicle: &dyn Entity, passenger: &dyn Entity) -> TeleportTransition {
        let rel = |r| self.relatives.contains(r);
        let y_rot = self.y_rot + if rel(Relative::YRot) { 0.0 } else { passenger.base().y_rot() - vehicle.base().y_rot() };
        let x_rot = self.x_rot + if rel(Relative::XRot) { 0.0 } else { passenger.base().x_rot() - vehicle.base().x_rot() };
        let offset = passenger.base().position().subtract(vehicle.base().position());
        let position = self.position.add_xyz(if rel(Relative::X) { 0.0 } else { offset.x }, if rel(Relative::Y) { 0.0 } else { offset.y }, if rel(Relative::Z) { 0.0 } else { offset.z });
        TeleportTransition { position, y_rot, x_rot, as_passenger: true, ..self.clone() }
    }
}

/// `Entity.teleportSameDimension` for an entity that is out of its level's storage (non-player entities; players use
/// [`teleport_player`]). Passengers move with it.
pub fn teleport_same_dimension(level: &mut ServerLevel, entity: &mut dyn Entity, t: &TeleportTransition) {
    for pid in entity.base().passengers.clone() {
        let pt = match level.entity_manager.get_any(pid) {
            Some(p) => t.for_passenger(entity, p),
            None => continue,
        };
        level.with_entity(pid, |level, p| teleport_same_dimension(level, p, &pt));
    }
    entity.teleport_set_position(t.position_move_rotation(), t.relatives);
    if !t.as_passenger {
        send_teleport_transition_to_players(level, entity, t);
    }
}

/// `Entity.sendTeleportTransitionToPlayers`.
fn send_teleport_transition_to_players(level: &mut ServerLevel, entity: &dyn Entity, t: &TeleportTransition) {
    let controller = entity.get_controlling_passenger();
    let mut passengers = Vec::new();
    let mut stack = entity.base().passengers.clone();
    while let Some(id) = stack.pop() {
        passengers.push(id);
        if let Some(p) = level.entity_manager.get_any(id) {
            stack.extend(p.base().passengers.iter().copied());
        }
    }
    let on_ground = entity.base().on_ground();
    for &pid in &passengers {
        if let Some(player) = level.entity_manager.get_any(pid).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()) {
            if controller == Some(pid) {
                player.send(&ClientboundTeleportEntityPacket { id: entity.id(), change: t.position_move_rotation(), relatives: t.relatives, on_ground });
            } else {
                player.send(&ClientboundTeleportEntityPacket { id: entity.id(), change: position_move_rotation_of(entity), relatives: Relatives::NONE, on_ground });
            }
        }
    }
    let ty = entity.get_type();
    if ty.data().update_interval == i32::MAX || entity.move_simulation_type_is_server_and_client() {
        let payload = encode_payload(&ClientboundTeleportEntityPacket { id: entity.id(), change: position_move_rotation_of(entity), relatives: Relatives::NONE, on_ground });
        level.entity_tracker.send_to_tracking_players_filtered(entity.id(), &payload, |pid| !passengers.contains(&pid));
    }
}

/// `Entity.teleport(transition)` for a non-player entity `id` of `old_dimension`; returns the id of the entity that
/// exists afterwards (a new one for a different dimension). Passengers are ejected first and follow.
pub fn teleport_entity(server: &mut MinecraftServer, old_dimension: &DimensionKey, id: EntityId, t: &TeleportTransition) -> Option<EntityId> {
    if t.level == *old_dimension {
        return server
            .with_level(old_dimension, |_, level| {
                level.with_entity(id, |level, e| {
                    if e.is_removed() {
                        return None;
                    }
                    if !t.as_passenger {
                        level.stop_riding(e);
                    }
                    teleport_same_dimension(level, e, t);
                    Some(id)
                })
            })
            .flatten()
            .flatten();
    }
    // cross dimension: `restoreFrom` = save + load into a fresh entity of the same type in the new level
    let (tag, passengers, ty) = server
        .with_level(old_dimension, |_, level| {
            level.with_entity(id, |level, e| {
                if e.is_removed() {
                    return None;
                }
                if !t.as_passenger {
                    level.stop_riding(e);
                }
                let passengers = e.base().passengers.clone();
                level.eject_passengers(e);
                let mut tag = mc_nbt::CompoundTag::new();
                let none = |_| None;
                e.save_without_id(&mut tag, &none);
                Some((tag, passengers, e.get_type()))
            })
        })
        .flatten()
        .flatten()?;
    let mut new_passengers = Vec::new();
    for pid in &passengers {
        let pt = server.with_level(old_dimension, |_, level| level.entity_manager.get_any(*pid).map(|p| TeleportTransition { as_passenger: true, ..t.clone() }.position_for(p, t)));
        if let Some(Some(pt)) = pt {
            if let Some(n) = teleport_entity(server, old_dimension, *pid, &pt) {
                new_passengers.push((n, pt.level.clone()));
            }
        }
    }
    let new_id = server
        .with_level(&t.level, |_, level| {
            let mut new = factory::create(ty, level, EntitySpawnRequest { reason: EntitySpawnReason::DimensionTravel, ignore_checks: true })?;
            let mut tag = tag;
            tag.remove("Dimension");
            tag.remove("Passengers");
            new.load(&tag).ok()?;
            new.teleport_set_position(PositionMoveRotation { position: t.position, delta_movement: t.delta_movement, y_rot: t.y_rot, x_rot: t.x_rot }, t.relatives);
            let new_id = new.id();
            level.add_fresh_entity(new);
            for (pid, _) in &new_passengers {
                level.with_entity(*pid, |level, p| {
                    level.start_riding(p, new_id, true, false);
                });
            }
            Some(new_id)
        })
        .flatten()?;
    server.with_level(old_dimension, |_, level| level.remove_entity(id, RemovalReason::ChangedDimension));
    Some(new_id)
}

impl TeleportTransition {
    fn position_for(&self, passenger: &dyn Entity, base: &TeleportTransition) -> TeleportTransition {
        TeleportTransition { position: base.position, y_rot: passenger.base().y_rot(), x_rot: passenger.base().x_rot(), as_passenger: true, ..base.clone() }
    }
}

/// `ServerPlayer.teleport(transition)`: same dimension = a client teleport; other dimension = respawn packet, level
/// info, re-add to the new level. The player must be in its level (not taken out). Returns whether it happened.
pub fn teleport_player(server: &mut MinecraftServer, old_dimension: &DimensionKey, id: EntityId, t: &TeleportTransition) -> bool {
    // take the player (with its connection) out of the old level; a dummy with the same id is removed in its place
    let taken = server
        .with_level(old_dimension, |_, level| {
            level.with_player(id, |level, player| {
                if player.is_removed() {
                    return None;
                }
                if t.missing_respawn_block {
                    player.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::NO_RESPAWN_BLOCK_AVAILABLE, param: 0.0 });
                }
                if !t.as_passenger {
                    level.stop_riding(player);
                }
                if t.level == *old_dimension {
                    player.connection_teleport_pmr(t.position_move_rotation(), t.relatives);
                    player.reset_position_after_teleport();
                    return Some(None);
                }
                player.close_menus_on_removal(level);
                let profile = player.player.game_profile.clone();
                let mut dummy = ServerPlayer::new(id, player.dimension.clone(), profile, Default::default());
                dummy.remove(RemovalReason::ChangedDimension);
                Some(Some(std::mem::replace(player, dummy)))
            })
        })
        .flatten()
        .flatten();
    let Some(moved) = taken else { return false };
    let Some(mut player) = moved else { return true };

    // cross dimension
    player.is_changing_dimension = true;
    if *old_dimension == DimensionKey::with_default_namespace("overworld").expect("valid") && t.level == DimensionKey::with_default_namespace("the_nether").expect("valid") {
        player.entered_nether_position = Some(player.position());
    }
    let destination = t.level.clone();
    let handle = player.handle.clone();
    player.dimension = destination.clone();
    server.with_level(&destination, |server, level| {
        let info = level_join_info(server, level);
        player.send(&ClientboundRespawnPacket { common_player_spawn_info: create_common_spawn_info(&player, &info, &destination), data_to_keep: 3 });
        player.send(&ClientboundChangeDifficultyPacket { difficulty: info.difficulty, locked: info.difficulty_locked });
        player.connection_teleport_pmr(t.position_move_rotation(), t.relatives);
        player.reset_position_after_teleport();
        player.is_changing_dimension = false;
        player.send(&abilities_packet(&player));
        if let Some(handle) = &handle {
            let host = ServerJoinHost { server: &mut *server, level: &mut *level };
            send_level_info(&host, &info, handle);
        }
        player.sync_handle();
        level.add_player(Box::new(player));
        level.with_player(id, |level, p| {
            p.init_inventory_menu(level);
            p.send_all_player_info(level);
            p.last_sent_exp = -1;
            p.last_sent_health = -1.0;
            p.last_sent_food = -1;
        });
        level.reset_empty_time();
    })
    .is_some()
}

impl ServerPlayer {
    /// `connection.resetPosition()`.
    pub fn reset_position_after_teleport(&mut self) {
        let pos = self.position();
        if let Some(l) = self.listener.as_mut() {
            l.reset_position(pos);
        }
    }

    /// `PlayerList.sendAllPlayerInfo(player)`: full inventory resend + held slot.
    pub fn send_all_player_info(&mut self, level: &mut ServerLevel) {
        use crate::world::entity::player::menu_ctx::PlayerCtx;
        use crate::world::inventory::PlayerMenus;
        let mut menus = std::mem::replace(&mut self.player.menus, PlayerMenus::new());
        {
            let mut ctx = PlayerCtx { player: self, level };
            menus.inventory_menu_mut().base_mut().send_all_data_to_remote(&mut ctx);
        }
        self.player.menus = menus;
        self.last_sent_health = -1.0;
        self.last_sent_food = -1;
        self.last_sent_exp = -1;
        let slot = self.player.inventory.get_selected_slot();
        self.send(&ClientboundSetHeldSlotPacket { slot });
    }
}
