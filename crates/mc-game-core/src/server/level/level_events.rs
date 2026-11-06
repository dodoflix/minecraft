//! Level-wide effect broadcasts of `ServerLevel` (sounds, level events, block events, block destruction progress): the
//! packets go to the players of this level within range (`PlayerList.broadcast`, which iterates the level's players here).

use super::{ServerLevel, TrackerPlayer};
use crate::server::network::ConnectionSender;
use crate::server::network::connection::Payload;
use crate::world::entity::EntityId;
use crate::world::entity::player::ServerPlayer;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::SoundEvent;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    ClientboundBlockDestructionPacket, ClientboundBlockEventPacket, ClientboundLevelEventPacket,
    ClientboundSoundEntityPacket, ClientboundSoundPacket, SoundEventHolder, SoundSource,
};
use std::sync::Arc;

/// Encode a packet once (`VarInt id + body`) for a broadcast.
pub fn encode_packet<P: Packet>(packet: &P) -> Payload {
    Payload::encode(packet)
}

/// `SoundEvent.getRange(volume)`.
pub fn sound_range(sound: SoundEvent, volume: f32) -> f64 {
    match sound.data().fixed_range {
        Some(range) => f64::from(range),
        None => {
            if volume > 1.0 {
                f64::from(16.0 * volume)
            } else {
                16.0
            }
        }
    }
}

/// One `PlayerList.broadcast(except, x, y, z, range, dimension, packet)` call, see `LevelTickState::broadcast_log`.
#[derive(Clone, Debug)]
pub struct BroadcastRecord {
    /// A player (not a non-player entity) is excluded from the broadcast.
    pub except: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub range: f64,
    /// `VarInt packet id + body` as sent.
    pub payload: Arc<[u8]>,
}

impl ServerLevel {
    /// `PlayerList.broadcast(except, x, y, z, range, dimension, packet)`: every player of this level but `except`
    /// within `range` blocks (squared distance below `range * range`) gets the packet.
    pub fn broadcast_nearby<P: Packet>(
        &self,
        except: Option<EntityId>,
        x: f64,
        y: f64,
        z: f64,
        range: f64,
        packet: &P,
    ) {
        let mut payload: Option<Payload> = None;
        if let Some(log) = &self.tick_state.broadcast_log {
            // a packet that failed to encode has no bytes to record
            if let Payload::Encoded(bytes) =
                payload.get_or_insert_with(|| encode_packet(packet)).clone()
            {
                log.lock().push(BroadcastRecord {
                    except: except.is_some(),
                    x,
                    y,
                    z,
                    range,
                    payload: bytes,
                });
            }
        }
        for &id in &self.players {
            if Some(id) == except {
                continue;
            }
            // a player that is running (taken out of the level: its own attack/use code) still hears its own sounds
            let (connection, position): (&ConnectionSender, _) = match self.get_entity(id) {
                Some(entity) => {
                    let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() else {
                        continue;
                    };
                    let Some(connection) = player.connection.as_ref() else {
                        continue;
                    };
                    (connection, entity.position())
                }
                None => match self.taken_out_players.get(&id) {
                    Some(TrackerPlayer {
                        connection: Some(connection),
                        position,
                        ..
                    }) => (connection, *position),
                    _ => continue,
                },
            };
            let (dx, dy, dz) = (x - position.x, y - position.y, z - position.z);
            if dx * dx + dy * dy + dz * dz < range * range {
                connection
                    .send_payload(payload.get_or_insert_with(|| encode_packet(packet)).clone());
            }
        }
    }

    /// `levelEvent(source, type, pos, data)`.
    pub fn level_event_impl(
        &self,
        source: Option<EntityId>,
        event_type: i32,
        pos: BlockPos,
        data: i32,
    ) {
        let except = source.filter(|id| self.is_player_id(*id));
        self.broadcast_nearby(
            except,
            f64::from(pos.x()),
            f64::from(pos.y()),
            f64::from(pos.z()),
            64.0,
            &ClientboundLevelEventPacket {
                r#type: event_type,
                pos,
                data,
                global_event: false,
            },
        );
    }

    /// `globalLevelEvent(type, pos, data)`; `global_sound_events` is `GameRules.GLOBAL_SOUND_EVENTS`.
    pub fn global_level_event(
        &self,
        global_sound_events: bool,
        event_type: i32,
        pos: BlockPos,
        data: i32,
        all_players: &[(EntityId, Option<ConnectionSender>, Vec3, bool)],
    ) {
        if !global_sound_events {
            self.level_event_impl(None, event_type, pos, data);
            return;
        }
        for (_, connection, player_pos, in_this_level) in all_players {
            let Some(connection) = connection else {
                continue;
            };
            let sound_pos = if *in_this_level {
                let center = Vec3::at_center_of(&pos);
                if player_pos.distance_to_sqr(center) < 32.0 * 32.0 {
                    center
                } else {
                    let direction = center.subtract(*player_pos).normalize();
                    player_pos.add(direction.scale(32.0))
                }
            } else {
                *player_pos
            };
            connection.send(&ClientboundLevelEventPacket {
                r#type: event_type,
                pos: BlockPos::containing_position(&sound_pos),
                data,
                global_event: true,
            });
        }
    }

    fn is_player_id(&self, id: EntityId) -> bool {
        self.players.contains(&id)
    }

    /// `playSeededSound(except, x, y, z, sound, source, volume, pitch, seed)`.
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_sound(
        &self,
        except: Option<EntityId>,
        x: f64,
        y: f64,
        z: f64,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) {
        let except = except.filter(|id| self.is_player_id(*id));
        self.broadcast_nearby(
            except,
            x,
            y,
            z,
            sound_range(sound, volume),
            &ClientboundSoundPacket::new(
                SoundEventHolder::Reference(sound),
                source,
                x,
                y,
                z,
                volume,
                pitch,
                seed,
            ),
        );
    }

    /// `playSeededSound(except, x, y, z, Holder.direct(SoundEvent.createVariableRangeEvent(id)), source, volume, pitch, seed)`:
    /// a sound that is not in the registry (note block custom head sounds) is sent inline.
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_variable_range_sound(
        &self,
        except: Option<EntityId>,
        x: f64,
        y: f64,
        z: f64,
        id: mc_core::ident::Identifier,
        source: SoundSource,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) {
        use mc_protocol::packets::play::types::DirectSoundEvent;
        let except = except.filter(|id| self.is_player_id(*id));
        let range = if volume > 1.0 {
            f64::from(16.0 * volume)
        } else {
            16.0
        };
        self.broadcast_nearby(
            except,
            x,
            y,
            z,
            range,
            &ClientboundSoundPacket::new(
                SoundEventHolder::Direct(DirectSoundEvent {
                    location: id,
                    fixed_range: None,
                }),
                source,
                x,
                y,
                z,
                volume,
                pitch,
                seed,
            ),
        );
    }

    /// `playSound(except, x, y, z, sound, source, volume, pitch)`: draws the seed from `soundSeedGenerator`.
    #[allow(clippy::too_many_arguments)]
    pub fn play_sound_at(
        &mut self,
        except: Option<EntityId>,
        x: f64,
        y: f64,
        z: f64,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        use mc_core::random::RandomSource;
        let seed = self.tick_state.sound_seed_generator.next_long();
        self.play_seeded_sound(except, x, y, z, sound, source, volume, pitch, seed);
    }

    /// `playSound(except, pos, sound, source, volume, pitch)` (block centre).
    pub fn play_sound(
        &mut self,
        except: Option<EntityId>,
        pos: BlockPos,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        self.play_sound_at(
            except,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
            sound,
            source,
            volume,
            pitch,
        );
    }

    /// `playSeededSound(except, sourceEntity, sound, source, volume, pitch, seed)` (`ClientboundSoundEntityPacket`).
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_sound_from_entity(
        &self,
        except: Option<EntityId>,
        source_entity: EntityId,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) {
        let Some(entity) = self.get_entity(source_entity) else {
            return;
        };
        let p = entity.position();
        let except = except.filter(|id| self.is_player_id(*id));
        self.broadcast_nearby(
            except,
            p.x,
            p.y,
            p.z,
            sound_range(sound, volume),
            &ClientboundSoundEntityPacket {
                sound: SoundEventHolder::Reference(sound),
                source,
                id: source_entity,
                volume,
                pitch,
                seed,
            },
        );
    }
    /// `playSound(except, sourceEntity, sound, source, volume, pitch)` for an entity that is out of the level's storage
    /// (`position` is its position): draws the seed like `playSound`, sends the `ClientboundSoundEntityPacket`.
    #[allow(clippy::too_many_arguments)]
    pub fn play_sound_from_entity_at(
        &mut self,
        except: Option<EntityId>,
        source_entity: EntityId,
        position: Vec3,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        use mc_core::random::RandomSource;
        let seed = self.tick_state.sound_seed_generator.next_long();
        let except = except.filter(|id| self.is_player_id(*id));
        self.broadcast_nearby(
            except,
            position.x,
            position.y,
            position.z,
            sound_range(sound, volume),
            &ClientboundSoundEntityPacket {
                sound: SoundEventHolder::Reference(sound),
                source,
                id: source_entity,
                volume,
                pitch,
                seed,
            },
        );
    }

    /// `destroyBlockProgress(id, pos, progress)`.
    pub fn destroy_block_progress(&self, id: i32, pos: BlockPos, progress: i32) {
        for &player_id in &self.players {
            if player_id == id {
                continue;
            }
            let Some(entity) = self.get_entity(player_id) else {
                continue;
            };
            let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() else {
                continue;
            };
            let p = entity.position();
            let (xd, yd, zd) = (
                f64::from(pos.x()) - p.x,
                f64::from(pos.y()) - p.y,
                f64::from(pos.z()) - p.z,
            );
            if xd * xd + yd * yd + zd * zd < 1024.0
                && let Some(connection) = &player.connection
            {
                connection.send(&ClientboundBlockDestructionPacket {
                    id,
                    pos,
                    progress: progress as u8,
                });
            }
        }
    }

    /// `ClientboundBlockEventPacket` to everyone within 64 blocks (the tail of `runBlockEvents`).
    pub(super) fn broadcast_block_event(&self, event: &super::block_events::BlockEventData) {
        self.broadcast_nearby(
            None,
            f64::from(event.pos.x()),
            f64::from(event.pos.y()),
            f64::from(event.pos.z()),
            64.0,
            &ClientboundBlockEventPacket {
                pos: event.pos,
                b0: event.param_a as u8,
                b1: event.param_b as u8,
                block: event.block,
            },
        );
    }

    /// `ServerChunkCache.sendToTrackingPlayers(entity, packet)`: the players tracking the entity.
    pub fn send_to_tracking_players<P: Packet>(&self, entity: EntityId, packet: &P) {
        self.entity_tracker
            .send_to_tracking_players(entity, &encode_packet(packet));
    }

    /// `ServerChunkCache.sendToTrackingPlayersAndSelf(entity, packet)`: the tracking
    /// players and, for a player, itself.
    pub fn send_to_tracking_players_and_self<P: Packet>(&self, entity: EntityId, packet: &P) {
        self.entity_tracker
            .send_to_tracking_players_and_self(entity, &encode_packet(packet));
    }

    /// `Level.broadcastEntityEvent(entity, event)` (`ServerLevel`: `ClientboundEntityEventPacket` to everyone tracking
    /// the entity, and to the entity itself if it is a player).
    pub fn broadcast_entity_event(&self, entity: EntityId, event_id: i8) {
        self.send_to_tracking_players_and_self(
            entity,
            &mc_protocol::packets::play::ClientboundEntityEventPacket {
                entity_id: entity,
                event_id,
            },
        );
    }
}
