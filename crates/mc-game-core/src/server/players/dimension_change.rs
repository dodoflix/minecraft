//! `Entity.teleport(TeleportTransition)` and `ServerPlayer.teleport(TeleportTransition)`: same-dimension moves and
//! the cross-dimension hand-over between `ServerLevel`s (`ClientboundRespawn`, level info resend, tracker/chunk-map
//! re-registration through the level's entity events), plus `Entity.teleportToPortalDestination`.
//!
//! Levels are owned by the server and the level being ticked is taken out of it, so a cross-level move is a free
//! function over `(&mut MinecraftServer, &mut ServerLevel /* the entity's level */, &mut dyn Entity /* taken out */)`:
//! [`teleport_taken_out`]. The by-id entry points ([`teleport_entity`], [`teleport_player`]) take the entity out of its
//! level first. A player that changes dimension is moved out of the entity slot (a removed dummy with the same id stays
//! behind, like `removePlayerImmediately`), so the caller must not use the slot afterwards.
//!
//! Portal teleports (`handlePortal`) are queued by the entity (`ServerLevel::queue_portal_teleport`) and run by
//! [`teleport_through_portal`] right after the entity's tick (non-players: `tick_listed_entity`; players: the end of
//! the connection tick, see `game_packets::tick_connection`).

pub use super::teleport_transition::{
    PostTeleportTransition, TeleportTransition, place_portal_ticket,
};

use super::join::{
    abilities_packet, create_common_spawn_info, send_active_player_effects, send_level_info,
    send_player_permission_level, send_post_effects,
};
use super::server_join_host::{ServerJoinHost, level_join_info};
use crate::advancements::triggers::CriteriaTriggers;
use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::server::level::server_entity::encode_payload;
use crate::server::world_dimensions::{nether_key, overworld_key};
use crate::world::entity::entity::{Entity, position_move_rotation_of};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::{EntityId, RemovalReason, ServerPlayer};
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::GameRules;
use mc_core::phys::Vec3;
use mc_data::enums::EntitySpawnReason;
use mc_protocol::packets::play::{
    ClientboundChangeDifficultyPacket, ClientboundGameEventPacket, ClientboundRespawnPacket,
    ClientboundSetHeldSlotPacket, ClientboundTeleportEntityPacket, PositionMoveRotation, Relative,
    Relatives,
};

/// `calculatePassengerTransition(transition, passenger)` of `vehicle`.
fn calculate_passenger_transition(
    vehicle: &dyn Entity,
    passenger: &dyn Entity,
    t: &TeleportTransition,
) -> TeleportTransition {
    let rel = |r| t.relatives.contains(r);
    let y_rot = t.y_rot
        + if rel(Relative::YRot) {
            0.0
        } else {
            passenger.base().y_rot() - vehicle.base().y_rot()
        };
    let x_rot = t.x_rot
        + if rel(Relative::XRot) {
            0.0
        } else {
            passenger.base().x_rot() - vehicle.base().x_rot()
        };
    let offset = passenger
        .base()
        .position()
        .subtract(vehicle.base().position());
    let position = t.position.add_xyz(
        if rel(Relative::X) { 0.0 } else { offset.x },
        if rel(Relative::Y) { 0.0 } else { offset.y },
        if rel(Relative::Z) { 0.0 } else { offset.z },
    );
    t.with_position(position)
        .with_rotation(y_rot, x_rot)
        .transition_as_passenger()
}

/// `ServerPlayer.teleport` / `Entity.teleportSameDimension` for an entity (player or not) that is out of its level's
/// storage; passengers move with it. The caller already did `stopRiding` / `removeVehicle` for a non-passenger move.
pub fn teleport_same_dimension(
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    t: &TeleportTransition,
) {
    if let Some(player) = entity.as_any_mut().downcast_mut::<ServerPlayer>() {
        player.connection_teleport_pmr(t.position_move_rotation(), t.relatives);
        player.reset_position_after_teleport();
        t.post_teleport_transition.on_transition(level, player);
        return;
    }
    for pid in entity.base().passengers.clone() {
        let Some(pt) = level
            .entity_manager
            .get_any(pid)
            .map(|p| calculate_passenger_transition(entity, p, t))
        else {
            continue;
        };
        level.with_entity(pid, |level, p| {
            if !p.is_removed() {
                player_prologue(level, p, &pt);
                teleport_same_dimension(level, p, &pt);
            }
        });
    }
    entity.teleport_set_position(t.position_move_rotation(), t.relatives);
    if !t.as_passenger {
        send_teleport_transition_to_players(level, entity, t);
    }
    t.post_teleport_transition.on_transition(level, entity);
}

/// The start of `ServerPlayer.teleport`: `NO_RESPAWN_BLOCK_AVAILABLE`, then `removeVehicle()` for a non-passenger move.
fn player_prologue(level: &mut ServerLevel, entity: &mut dyn Entity, t: &TeleportTransition) {
    if !entity.is_player() {
        return;
    }
    if t.missing_respawn_block
        && let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>()
    {
        player.send(&ClientboundGameEventPacket {
            event: ClientboundGameEventPacket::NO_RESPAWN_BLOCK_AVAILABLE,
            param: 0.0,
        });
    }
    if !t.as_passenger {
        level.remove_vehicle(entity);
    }
}

/// `Entity.sendTeleportTransitionToPlayers`.
fn send_teleport_transition_to_players(
    level: &mut ServerLevel,
    entity: &dyn Entity,
    t: &TeleportTransition,
) {
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
        if let Some(player) = level
            .entity_manager
            .get_any(pid)
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
        {
            if controller == Some(pid) {
                player.send(&ClientboundTeleportEntityPacket {
                    id: entity.id(),
                    change: t.position_move_rotation(),
                    relatives: t.relatives,
                    on_ground,
                });
            } else {
                player.send(&ClientboundTeleportEntityPacket {
                    id: entity.id(),
                    change: position_move_rotation_of(entity),
                    relatives: Relatives::NONE,
                    on_ground,
                });
            }
        }
    }
    let ty = entity.get_type();
    if ty.data().update_interval == i32::MAX || entity.move_simulation_type_is_server_and_client() {
        let payload = encode_payload(&ClientboundTeleportEntityPacket {
            id: entity.id(),
            change: position_move_rotation_of(entity),
            relatives: Relatives::NONE,
            on_ground,
        });
        level
            .entity_tracker
            .send_to_tracking_players_filtered(entity.id(), &payload, |pid| {
                !passengers.contains(&pid)
            });
    }
}

/// `Entity.teleport(transition)` (and its `ServerPlayer` override) for `entity` that is out of `level` (its level,
/// which is out of `server`). Returns the id of the entity that exists afterwards: a new one when a non-player changed
/// dimension, the same id otherwise; `None` when nothing happened.
pub fn teleport_taken_out(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    t: &TeleportTransition,
) -> Option<EntityId> {
    if entity.is_removed() {
        return None;
    }
    let other_dimension = t.level != level.dimension;
    if entity.is_player() {
        player_prologue(level, entity, t);
        if !other_dimension {
            teleport_same_dimension(level, entity, t);
            return Some(entity.id());
        }
        let player = entity.as_any_mut().downcast_mut::<ServerPlayer>()?;
        return teleport_player_cross_dimension(server, level, player, t).then(|| player.base_id());
    }
    if !t.as_passenger {
        level.stop_riding(entity);
    }
    if other_dimension {
        teleport_cross_dimension(server, level, entity, t)
    } else {
        teleport_same_dimension(level, entity, t);
        Some(entity.id())
    }
}

/// `Entity.teleportCrossDimension` for a non-player.
fn teleport_cross_dimension(
    server: &mut MinecraftServer,
    old_level: &mut ServerLevel,
    entity: &mut dyn Entity,
    t: &TeleportTransition,
) -> Option<EntityId> {
    let old_passengers = entity.base().passengers.clone();
    old_level.eject_passengers(entity);
    let mut new_passengers = Vec::new();
    for pid in old_passengers {
        let Some(pt) = old_level
            .entity_manager
            .get_any(pid)
            .map(|p| calculate_passenger_transition(entity, p, t))
        else {
            continue;
        };
        if let Some(Some(new_id)) =
            old_level.with_entity(pid, |level, p| teleport_taken_out(server, level, p, &pt))
        {
            new_passengers.push(new_id);
        }
    }
    let new_id = server
        .with_level(&t.level, |_, new_level| {
            let mut new = factory::create(
                entity.get_type(),
                new_level,
                EntitySpawnRequest {
                    reason: EntitySpawnReason::DimensionTravel,
                    ignore_checks: true,
                },
            )?;
            restore_from(new.as_mut(), entity);
            let current = position_move_rotation_of(entity);
            remove_after_changing_dimensions(old_level, entity);
            new.teleport_set_position_from(current, t.position_move_rotation(), t.relatives);
            let new_id = new.id();
            new_level.add_fresh_entity(new);
            for &passenger in &new_passengers {
                new_level.with_entity(passenger, |level, p| {
                    if level.start_riding(p, new_id, true, false)
                        && let Some(player) = p.as_any_mut().downcast_mut::<ServerPlayer>()
                    {
                        // `ServerPlayer.startRiding`
                        let position = player.position();
                        player.connection_teleport_pmr(
                            PositionMoveRotation {
                                position,
                                delta_movement: Vec3::ZERO,
                                y_rot: 0.0,
                                x_rot: 0.0,
                            },
                            Relatives::ROTATION,
                        );
                    }
                });
            }
            new_level.reset_empty_time();
            new_level.with_entity(new_id, |level, e| {
                t.post_teleport_transition.on_transition(level, e)
            });
            Some(new_id)
        })
        .flatten()?;
    teleport_spectators(server, old_level, entity.id(), t);
    Some(new_id)
}

/// `Entity.restoreFrom(oldEntity)`: save + load, then the portal state.
fn restore_from(new: &mut dyn Entity, old: &mut dyn Entity) {
    let mut tag = mc_nbt::CompoundTag::new();
    let none = |_| None;
    old.save_without_id(&mut tag, &none);
    tag.remove("Dimension");
    tag.remove("Passengers");
    let _ = new.load(&tag);
    new.base_mut().portal_cooldown = old.base().portal_cooldown;
    new.base_mut().portal_process = old.base().portal_process.clone();
}

/// `Entity.removeAfterChangingDimensions()`.
fn remove_after_changing_dimensions(level: &mut ServerLevel, entity: &mut dyn Entity) {
    entity.remove(RemovalReason::ChangedDimension);
    entity.remove_leash(level);
}

/// `Entity.teleportSpectators(transition, oldLevel)`: players spectating `entity` follow it.
fn teleport_spectators(
    server: &mut MinecraftServer,
    old_level: &mut ServerLevel,
    entity: EntityId,
    t: &TeleportTransition,
) {
    for pid in old_level.players().to_vec() {
        if old_level
            .get_entity(pid)
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
            .is_none_or(|p| p.camera != Some(entity))
        {
            continue;
        }
        old_level.with_entity(pid, |level, p| teleport_taken_out(server, level, p, t));
        if t.level == old_level.dimension {
            old_level.with_player(pid, |level, p| p.set_camera(level, None));
        } else {
            server.with_level(&t.level, |_, level| {
                level.with_player(pid, |level, p| p.set_camera(level, None))
            });
        }
    }
}

/// `ServerPlayer.teleport` to another dimension. `slot` is the player (out of `old_level`); on success the real player
/// lives in the destination level and `slot` holds a removed placeholder.
fn teleport_player_cross_dimension(
    server: &mut MinecraftServer,
    old_level: &mut ServerLevel,
    slot: &mut ServerPlayer,
    t: &TeleportTransition,
) -> bool {
    let destination = t.level.clone();
    if server.level(&destination).is_none() {
        return false;
    }
    let id = slot.base_id();
    let old_dimension = old_level.dimension.clone();
    let moved = server.with_level(&destination, |server, new_level| {
        slot.is_changing_dimension = true;
        let info = level_join_info(server, new_level);
        slot.send(&ClientboundRespawnPacket {
            common_player_spawn_info: create_common_spawn_info(slot, &info, &destination),
            data_to_keep: 3,
        });
        slot.send(&ClientboundChangeDifficultyPacket {
            difficulty: info.difficulty,
            locked: info.difficulty_locked,
        });
        let handle = slot.handle.clone();
        if let Some(handle) = &handle {
            let mut host = ServerJoinHost {
                server: &mut *server,
                level: &mut *new_level,
            };
            send_player_permission_level(&mut host, handle);
        }
        // `oldLevel.removePlayerImmediately(this, CHANGED_DIMENSION)` + `unsetRemoved()`: the player leaves the old slot
        slot.close_menus_on_removal(old_level);
        let mut dummy = ServerPlayer::new(
            id,
            slot.dimension.clone(),
            slot.player.game_profile.clone(),
            Default::default(),
        );
        dummy.remove(RemovalReason::ChangedDimension);
        let mut player = std::mem::replace(slot, dummy);
        if old_dimension == overworld_key() && destination == nether_key() {
            player.entered_nether_position = Some(player.position());
        }
        // `setServerLevel(newLevel)`
        player.dimension = destination.clone();
        player.connection_teleport_pmr(t.position_move_rotation(), t.relatives);
        player.reset_position_after_teleport();
        player.sync_handle();
        new_level.add_player(Box::new(player));
        new_level.with_player(id, |level, player| {
            // `triggerDimensionChangeTriggers(oldLevel)`
            CriteriaTriggers::CHANGED_DIMENSION.trigger(
                level,
                player,
                &old_dimension,
                &destination,
            );
            if old_dimension == nether_key() && destination == overworld_key() {
                if let Some(entered) = player.entered_nether_position {
                    CriteriaTriggers::NETHER_TRAVEL.trigger(level, player, entered);
                }
            }
            if destination != nether_key() {
                player.entered_nether_position = None;
            }
            player.stop_using_item(level);
            player.send(&abilities_packet(player));
            if let Some(handle) = &handle {
                let host = ServerJoinHost {
                    server: &mut *server,
                    level: &mut *level,
                };
                send_level_info(&host, &info, handle);
            }
            player.send_all_player_info(level);
            send_active_player_effects(player);
            send_post_effects(player);
            t.post_teleport_transition.on_transition(level, player);
            player.last_sent_exp = -1;
            player.last_sent_health = -1.0;
            player.last_sent_food = -1;
        });
        true
    });
    if moved != Some(true) {
        return false;
    }
    teleport_spectators(server, old_level, id, t);
    true
}

/// `Entity.teleport(transition)` for entity `id` in the storage of `old_dimension`: returns the resulting entity id.
pub fn teleport_entity(
    server: &mut MinecraftServer,
    old_dimension: &DimensionKey,
    id: EntityId,
    t: &TeleportTransition,
) -> Option<EntityId> {
    server
        .with_level(old_dimension, |server, level| {
            level.with_entity(id, |level, e| teleport_taken_out(server, level, e, t))
        })
        .flatten()
        .flatten()
}

/// `ServerPlayer.teleport(transition)` by id: whether it happened (the player keeps its id).
pub fn teleport_player(
    server: &mut MinecraftServer,
    old_dimension: &DimensionKey,
    id: EntityId,
    t: &TeleportTransition,
) -> bool {
    teleport_entity(server, old_dimension, id, t).is_some()
}

/// `ServerLevel.isAllowedToEnterPortal(toLevel)`.
pub fn is_allowed_to_enter_portal(server: &MinecraftServer, to: &DimensionKey) -> bool {
    *to != nether_key()
        || server
            .game_rules_ref()
            .get(GameRules::ALLOW_ENTERING_NETHER_USING_PORTALS)
}

/// `Entity.teleportToPortalDestination(level, transition)`.
pub fn teleport_to_portal_destination(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    t: &TeleportTransition,
) {
    if is_allowed_to_enter_portal(server, &t.level)
        && (t.level == level.dimension || entity.can_teleport(level, &level.dimension, &t.level))
    {
        teleport_taken_out(server, level, entity, t);
    }
}

/// The body of `Entity.handlePortal()` once `PortalProcessor.processPortalTeleportation` said yes: resolve the portal's
/// destination and teleport. `entity` is out of `level`.
pub fn teleport_through_portal(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
) {
    let Some(process) = entity.base().portal_process.clone() else {
        return;
    };
    if let Some(t) = process.get_portal_destination(server, level, entity) {
        teleport_to_portal_destination(server, level, entity, &t);
    }
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
            let mut ctx = PlayerCtx {
                player: self,
                level,
            };
            menus
                .inventory_menu_mut()
                .base_mut()
                .send_all_data_to_remote(&mut ctx);
        }
        self.player.menus = menus;
        self.last_sent_health = -1.0;
        self.last_sent_food = -1;
        self.last_sent_exp = -1;
        let slot = self.player.inventory.get_selected_slot();
        self.send(&ClientboundSetHeldSlotPacket { slot });
    }
}

/// The `Portal` branch of `ServerPlayerGameMode.useItemOn` for a spectator right-clicking a portal block: teleports the
/// player through it (`teleportToPortalDestination`) or tells it that it cannot. `None` when the block is no portal.
/// `level` is the player's level (out of `server`), `player` is out of `level`.
pub fn spectator_portal_use(
    server: &mut MinecraftServer,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    pos: mc_core::pos::BlockPos,
) -> Option<crate::world::item::InteractionResult> {
    use crate::world::level::BlockGetter;
    use crate::world::level::block::behaviour;
    let portal = behaviour(level.get_block_state(pos).block()).as_portal()?;
    match portal.get_portal_destination(server, level, player, pos) {
        Some(t) => {
            if player.camera.is_some() {
                player.set_camera(level, None);
            }
            teleport_to_portal_destination(server, level, player, &t);
        }
        None => player.send(&mc_protocol::packets::play::ClientboundSystemChatPacket {
            content: mc_text::Component::translatable("spectator.cannot_teleport", Vec::new())
                .with_format(mc_text::ChatFormatting::Red),
            overlay: true,
        }),
    }
    Some(crate::world::item::InteractionResult::CONSUME)
}
