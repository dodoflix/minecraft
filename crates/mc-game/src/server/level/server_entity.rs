//! `net.minecraft.server.level.ServerEntity`: per-tracked-entity packet generation (movement deltas, rotation,
//! motion, dirty synched data, attributes, passengers) and the pairing packets (`ClientboundAddEntity` + initial state).
//!
//! Packets are encoded once into `Arc<[u8]>` payloads and handed to a [`Synchronizer`] (the tracked entity's
//! `seenBy` fan-out).

use crate::world::entity::interpolation::end_position;
use crate::world::entity::vec_delta_codec::{self, VecDeltaCodec};
use crate::world::entity::{Entity, EntityId, TrackingKind};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_protocol::codec::{LpVec3, write_varint};
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    AttributeSnapshot, ClientboundAddEntityPacket, ClientboundBundleDelimiterPacket, ClientboundEntityPositionSyncPacket, ClientboundMoveEntityPosPacket,
    ClientboundMoveEntityPosRotPacket, ClientboundMoveEntityRotPacket, ClientboundProjectilePowerPacket, ClientboundRemoveEntitiesPacket,
    ClientboundRotateHeadPacket, ClientboundSetEntityDataPacket, ClientboundSetEntityLinkPacket, ClientboundSetEntityMotionPacket,
    ClientboundSetPassengersPacket, ClientboundUpdateAttributesPacket, DataValue, PositionPath, VecDelta, VecDeltaStep,
};
use std::sync::Arc;

const TOLERANCE_LEVEL_POSITION: f64 = 7.6293945E-6f32 as f64;
pub const FORCED_POS_UPDATE_PERIOD: i32 = 60;
const FORCED_TELEPORT_PERIOD: i32 = 400;

/// A packet as `VarInt id + body`, ready for `ConnectionSender::send_payload`.
pub type Payload = Arc<[u8]>;

/// Encode a packet like `ConnectionSender::send`.
pub fn encode_payload<P: Packet>(packet: &P) -> Payload {
    let mut buf = Vec::with_capacity(64);
    write_varint(&mut buf, P::ID);
    packet.encode(&mut buf);
    buf.into()
}

/// `ClientboundBundlePacket`: delimiter, the packets, delimiter.
pub fn bundle(packets: Vec<Payload>) -> Vec<Payload> {
    let delimiter = encode_payload(&ClientboundBundleDelimiterPacket);
    let mut out = Vec::with_capacity(packets.len() + 2);
    out.push(delimiter.clone());
    out.extend(packets);
    out.push(delimiter);
    out
}

/// `UpdateInterval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateInterval {
    Never,
    Periodic(i32),
}

impl UpdateInterval {
    pub fn test(self, tick: i32) -> bool {
        match self {
            UpdateInterval::Never => false,
            UpdateInterval::Periodic(p) => tick % p == 0,
        }
    }

    pub fn next_interval(self, tick: i32) -> i32 {
        match self {
            UpdateInterval::Never => 0,
            UpdateInterval::Periodic(p) => tick / p * p + p,
        }
    }
}

/// `ServerEntity.Synchronizer`.
pub trait Synchronizer {
    fn send_to_tracking_players(&mut self, payload: Payload);
    fn send_to_tracking_players_and_self(&mut self, payload: Payload);
}

pub struct ServerEntity {
    update_interval: UpdateInterval,
    track_delta: bool,
    position_codec: VecDeltaCodec,
    last_sent_y_rot: i8,
    last_sent_x_rot: i8,
    last_sent_y_head_rot: i8,
    last_sent_movement: Vec3,
    tick_count: i32,
    teleport_delay: i32,
    last_passengers: Vec<EntityId>,
    was_riding: bool,
    was_on_ground: bool,
    tracked_data_values: Option<Vec<DataValue>>,
}

impl ServerEntity {
    pub fn new(entity: &dyn Entity, update_interval: UpdateInterval, track_delta: bool) -> Self {
        let mut position_codec = VecDeltaCodec::default();
        position_codec.set_base(entity.tracking_position());
        Self {
            update_interval,
            track_delta,
            position_codec,
            last_sent_y_rot: mth::pack_degrees(entity.base().y_rot()),
            last_sent_x_rot: mth::pack_degrees(entity.base().x_rot()),
            last_sent_y_head_rot: mth::pack_degrees(entity.get_y_head_rot()),
            last_sent_movement: entity.base().delta_movement(),
            tick_count: 0,
            teleport_delay: 0,
            last_passengers: Vec::new(),
            was_riding: false,
            was_on_ground: entity.base().on_ground(),
            tracked_data_values: entity.base().entity_data.get_non_default_values(),
        }
    }

    pub fn get_position_base(&self) -> Vec3 {
        self.position_codec.get_base()
    }
    pub fn get_last_sent_movement(&self) -> Vec3 {
        self.last_sent_movement
    }
    pub fn get_last_sent_x_rot(&self) -> f32 {
        mth::unpack_degrees(self.last_sent_x_rot)
    }
    pub fn get_last_sent_y_rot(&self) -> f32 {
        mth::unpack_degrees(self.last_sent_y_rot)
    }
    pub fn get_last_sent_y_head_rot(&self) -> f32 {
        mth::unpack_degrees(self.last_sent_y_head_rot)
    }

    /// `new ClientboundAddEntityPacket(entity, serverEntity, data)`.
    pub fn add_entity_packet(&self, id: EntityId, uuid: uuid::Uuid, entity_type: mc_data::EntityType, data: i32) -> ClientboundAddEntityPacket {
        let base = self.position_codec.get_base();
        ClientboundAddEntityPacket {
            id,
            uuid,
            r#type: entity_type,
            x: base.x,
            y: base.y,
            z: base.z,
            movement: LpVec3(self.last_sent_movement),
            x_rot: mth::pack_degrees(self.get_last_sent_x_rot()),
            y_rot: mth::pack_degrees(self.get_last_sent_y_rot()),
            y_head_rot: mth::pack_degrees(self.get_last_sent_y_head_rot()),
            data,
        }
    }

    /// `sendChanges()`.
    pub fn send_changes(&mut self, entity: &mut dyn Entity, sync: &mut dyn Synchronizer) {
        entity.update_data_before_sync();
        let passengers = entity.base().passengers.clone();
        if passengers != self.last_passengers {
            sync.send_to_tracking_players(encode_payload(&ClientboundSetPassengersPacket { vehicle: entity.id(), passengers: passengers.clone() }));
            self.last_passengers = passengers;
        }

        let current_position = entity.tracking_position();
        {
            let b = entity.base_mut();
            let mut sync_position = b.sync_position;
            b.interpolation_tracker.update_tracking(current_position, &mut sync_position);
            b.sync_position = sync_position;
        }
        if entity.base().sync_position {
            self.tick_count = self.update_interval.next_interval(self.tick_count);
            entity.base_mut().sync_position = false;
        }

        if entity.base().needs_sync || self.update_interval.test(self.tick_count) || entity.base().entity_data.is_dirty() {
            let y_rotn = mth::pack_degrees(entity.base().y_rot());
            let x_rotn = mth::pack_degrees(entity.base().x_rot());
            let should_send_rotation = (i32::from(y_rotn) - i32::from(self.last_sent_y_rot)).abs() >= 1 || (i32::from(x_rotn) - i32::from(self.last_sent_x_rot)).abs() >= 1;
            let id = entity.id();
            if entity.base().is_passenger() {
                if should_send_rotation {
                    sync.send_to_tracking_players(encode_payload(&ClientboundMoveEntityRotPacket { entity_id: id, on_ground: entity.base().on_ground(), y_rot: y_rotn, x_rot: x_rotn }));
                    self.last_sent_y_rot = y_rotn;
                    self.last_sent_x_rot = x_rotn;
                }
                entity.base_mut().interpolation_tracker.clear();
                self.position_codec.set_base(current_position);
                self.send_dirty_entity_data(entity, sync);
                self.was_riding = true;
            } else {
                self.teleport_delay += 1;
                let position = entity.base_mut().interpolation_tracker.get_position_path(current_position);
                entity.base_mut().interpolation_tracker.clear();
                let packet = if entity.tracking_kind() == TrackingKind::Arrow {
                    self.create_move_packet(entity, &position, y_rotn, x_rotn, true, true)
                } else {
                    let position_changed = self.position_codec.delta(current_position).length_sqr() >= TOLERANCE_LEVEL_POSITION;
                    let should_send_position = position_changed || self.tick_count % FORCED_POS_UPDATE_PERIOD == 0;
                    self.create_move_packet(entity, &position, y_rotn, x_rotn, should_send_position, should_send_rotation)
                };

                let fall_flying = entity.living().is_some_and(|l| l.is_fall_flying());
                if entity.base().needs_sync || self.track_delta || fall_flying {
                    let movement = entity.base().delta_movement();
                    let diff = movement.distance_to_sqr(self.last_sent_movement);
                    if diff > 1.0E-7 || (diff > 0.0 && movement.length_sqr() == 0.0) {
                        self.last_sent_movement = movement;
                        let motion = encode_payload(&ClientboundSetEntityMotionPacket { id, movement: LpVec3(self.last_sent_movement) });
                        match entity.hurting_projectile_power() {
                            Some(power) => {
                                for p in bundle(vec![motion, encode_payload(&ClientboundProjectilePowerPacket { id, acceleration_power: power })]) {
                                    sync.send_to_tracking_players(p);
                                }
                            }
                            None => sync.send_to_tracking_players(motion),
                        }
                    }
                }

                match packet {
                    Some(MovePacket { payload, has_position, has_rotation }) => {
                        sync.send_to_tracking_players(payload);
                        self.send_dirty_entity_data(entity, sync);
                        if has_position {
                            self.position_codec.set_base(current_position);
                        }
                        if has_rotation {
                            self.last_sent_y_rot = y_rotn;
                            self.last_sent_x_rot = x_rotn;
                        }
                    }
                    None => self.send_dirty_entity_data(entity, sync),
                }
                self.was_riding = false;
            }

            let y_head_rot = mth::pack_degrees(entity.get_y_head_rot());
            if (i32::from(y_head_rot) - i32::from(self.last_sent_y_head_rot)).abs() >= 1 {
                sync.send_to_tracking_players(encode_payload(&ClientboundRotateHeadPacket { entity_id: id, y_head_rot }));
                self.last_sent_y_head_rot = y_head_rot;
            }
            entity.base_mut().needs_sync = false;
        }

        self.tick_count = self.tick_count.wrapping_add(1);
        if entity.base().sync_velocity {
            entity.base_mut().sync_velocity = false;
            sync.send_to_tracking_players_and_self(encode_payload(&ClientboundSetEntityMotionPacket { id: entity.id(), movement: LpVec3(entity.base().delta_movement()) }));
        }
    }

    fn create_move_packet(&mut self, entity: &dyn Entity, position: &PositionPath, y_rotn: i8, x_rotn: i8, should_send_position: bool, should_send_rotation: bool) -> Option<MovePacket> {
        let b = entity.base();
        let id = entity.id();
        if b.requires_precise_position || self.teleport_delay > FORCED_TELEPORT_PERIOD || self.was_riding || self.was_on_ground != b.on_ground() {
            self.was_on_ground = b.on_ground();
            self.teleport_delay = 0;
            return Some(position_sync(entity, position));
        }
        if should_send_position {
            let Some(encoded) = self.try_encode(position) else { return Some(position_sync(entity, position)) };
            if self.is_full_precision_encoding_required(entity, end_position(position), &encoded) {
                return Some(position_sync(entity, position));
            }
            let on_ground = b.on_ground();
            return Some(if should_send_rotation {
                MovePacket {
                    payload: encode_payload(&ClientboundMoveEntityPosRotPacket { entity_id: id, delta: encoded, y_rot: y_rotn, x_rot: x_rotn, on_ground }),
                    has_position: true,
                    has_rotation: true,
                }
            } else {
                MovePacket { payload: encode_payload(&ClientboundMoveEntityPosPacket { entity_id: id, delta: encoded, on_ground }), has_position: true, has_rotation: false }
            });
        }
        should_send_rotation.then(|| MovePacket {
            payload: encode_payload(&ClientboundMoveEntityRotPacket { entity_id: id, on_ground: b.on_ground(), y_rot: y_rotn, x_rot: x_rotn }),
            has_position: false,
            has_rotation: true,
        })
    }

    /// `VecDeltaCodec.tryEncode(PositionPath)`.
    fn try_encode(&self, path: &PositionPath) -> Option<VecDelta> {
        match path {
            PositionPath::Linear(p) => self.position_codec.try_encode(*p).map(|l| VecDelta::Linear { xa: l.xa, ya: l.ya, za: l.za }),
            PositionPath::Stepped(steps) => {
                if steps.is_empty() {
                    return Some(VecDelta::Stepped(Vec::new()));
                }
                let mut codec = self.position_codec;
                let mut out = Vec::with_capacity(steps.len());
                for step in steps {
                    let pos = step.position;
                    let (xa, ya, za) = (codec.encode_x(pos), codec.encode_y(pos), codec.encode_z(pos));
                    if vec_delta_codec::is_delta_too_big(xa, ya, za) {
                        return None;
                    }
                    out.push(VecDeltaStep { ticks: step.tick_offset, xa: xa as i16, ya: ya as i16, za: za as i16 });
                    codec.set_base(pos);
                }
                Some(VecDelta::Stepped(out))
            }
        }
    }

    fn is_full_precision_encoding_required(&self, entity: &dyn Entity, pos: Vec3, delta: &VecDelta) -> bool {
        if entity.tracking_kind() != TrackingKind::ItemEntity {
            return false;
        }
        let (has_x, has_z) = match delta {
            VecDelta::Linear { xa, za, .. } => (*xa != 0, *za != 0),
            VecDelta::Stepped(steps) => (steps.iter().any(|s| s.xa != 0), steps.iter().any(|s| s.za != 0)),
        };
        let b = entity.base();
        let loss = vec_delta_codec::encoding_precision_loss;
        (b.vertical_collision && ((has_x && loss(pos.x) != 0.0) || (has_z && loss(pos.z) != 0.0)))
            || (b.horizontal_collision && ((has_x && loss(pos.z) != 0.0) || (has_z && loss(pos.x) != 0.0)))
    }

    /// `removePairing(player)` packet.
    pub fn remove_pairing_packet(entity_id: EntityId) -> Payload {
        encode_payload(&ClientboundRemoveEntitiesPacket { entity_ids: vec![entity_id] })
    }

    /// `sendPairingData(player, broadcast)`: the packets a newly tracking player gets (inside a bundle).
    ///
    /// `vehicle` is the entity's vehicle with the vehicle's passenger list; `leash_holder` is `Some(holder id)` /
    /// `Some(None)`-less: pass `Some(id)` when the entity is leashed.
    pub fn send_pairing_data(&mut self, entity: &mut dyn Entity, vehicle: Option<(EntityId, Vec<EntityId>)>, leash_holder: Option<EntityId>) -> Vec<Payload> {
        entity.update_data_before_sync();
        if entity.is_removed() {
            log::warn!("Fetching packet for removed entity {}['{}'/{}]", entity.get_type().data().name, entity.get_type().data().name, entity.id());
        }
        let mut out = Vec::new();
        out.push(encode_payload(&entity.get_add_entity_packet(self)));
        if let Some(values) = &self.tracked_data_values {
            out.push(encode_payload(&ClientboundSetEntityDataPacket { id: entity.id(), packed_items: values.clone() }));
        }
        if let Some(living) = entity.living() {
            let attributes: Vec<AttributeSnapshot> = living
                .attributes
                .get_syncable_attributes()
                .into_iter()
                .map(|i| AttributeSnapshot { attribute: i.get_attribute(), base: i.get_base_value(), modifiers: i.get_modifiers().cloned().collect() })
                .collect();
            if !attributes.is_empty() {
                out.push(encode_payload(&ClientboundUpdateAttributesPacket { entity_id: entity.id(), attributes }));
            }
        }
        if let Some(packet) = entity.equipment_packet() {
            out.push(encode_payload(&packet));
        }
        if !entity.base().passengers.is_empty() {
            out.push(encode_payload(&ClientboundSetPassengersPacket { vehicle: entity.id(), passengers: entity.base().passengers.clone() }));
        }
        if let Some((vehicle_id, passengers)) = vehicle {
            out.push(encode_payload(&ClientboundSetPassengersPacket { vehicle: vehicle_id, passengers }));
        }
        if let Some(holder) = leash_holder {
            out.push(encode_payload(&ClientboundSetEntityLinkPacket { source_id: entity.id(), dest_id: holder }));
        }
        out
    }

    /// `sendDirtyEntityData()`.
    fn send_dirty_entity_data(&mut self, entity: &mut dyn Entity, sync: &mut dyn Synchronizer) {
        if let Some(packed) = entity.base_mut().entity_data.pack_dirty() {
            self.tracked_data_values = entity.base().entity_data.get_non_default_values();
            sync.send_to_tracking_players_and_self(encode_payload(&ClientboundSetEntityDataPacket { id: entity.id(), packed_items: packed }));
        }
        let id = entity.id();
        if let Some(living) = entity.living_mut() {
            let to_sync = living.attributes.take_attributes_to_sync();
            if !to_sync.is_empty() {
                let attributes = to_sync
                    .into_iter()
                    .filter_map(|a| living.attributes.get_instance(a).map(|i| AttributeSnapshot { attribute: a, base: i.get_base_value(), modifiers: i.get_modifiers().cloned().collect() }))
                    .collect();
                sync.send_to_tracking_players_and_self(encode_payload(&ClientboundUpdateAttributesPacket { entity_id: id, attributes }));
            }
        }
    }
}

struct MovePacket {
    payload: Payload,
    has_position: bool,
    has_rotation: bool,
}

/// `ClientboundEntityPositionSyncPacket.of(entity, position)`.
fn position_sync(entity: &dyn Entity, position: &PositionPath) -> MovePacket {
    MovePacket {
        payload: encode_payload(&ClientboundEntityPositionSyncPacket {
            id: entity.id(),
            position: position.clone(),
            y_rot: entity.base().y_rot(),
            x_rot: entity.base().x_rot(),
            on_ground: entity.base().on_ground(),
        }),
        has_position: true,
        has_rotation: true,
    }
}
