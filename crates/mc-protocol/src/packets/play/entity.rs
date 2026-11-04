//! Play packets: group `entity` (see packets/play/mod.rs for the split).
use mc_core::ident::Identifier;
use mc_core::phys::vec3::Vec3;

use super::types::{InteractionHand, PositionMoveRotation, Relatives};
use crate::codec::{
    Angle, DecodeError, DecodeResult, LpVec3, read_count, read_varint, write_varint,
};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `SwingAnimationType` (VarInt id, unknown = none)
    SwingAnimationType { None = 0, Whack = 1, Stab = 2 } out_of_bounds = ZERO
);

/// `SwingAnimation.STREAM_CODEC` (also the `swing_animation` data component).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct SwingAnimation {
    pub kind: SwingAnimationType,
    #[mc(var)]
    pub duration: i32,
}

id_enum!(
    /// `AttributeModifier.Operation` (VarInt id, unknown = add value)
    AttributeModifierOperation { AddValue = 0, AddMultipliedBase = 1, AddMultipliedTotal = 2 } out_of_bounds = ZERO
);

/// `AttributeModifier.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttributeModifier {
    pub id: Identifier,
    pub amount: f64,
    pub operation: AttributeModifierOperation,
}

/// `EntityAnchorArgument.Anchor` (`readEnum`: VarInt ordinal; out-of-range ordinals throw `ArrayIndexOutOfBoundsException`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 2")]
pub enum Anchor {
    Feet,
    Eyes,
}

/// `PositionAndRotation.STREAM_CODEC` (position + yaw + pitch).
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct PositionAndRotation {
    pub position: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// `with` codec for entity ids sent as `id + 1` (`ClientboundDamageEventPacket.writeOptionalEntityId`: `-1` = none).
mod optional_entity_id {
    use super::*;

    pub fn encode(id: &i32, out: &mut Vec<u8>) {
        write_varint(out, id.wrapping_add(1))
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<i32> {
        read_varint(input).map(|v| v.wrapping_sub(1))
    }
}

/// `FriendlyByteBuf.readVarIntArray()`: VarInt count bounded by the readable bytes.
mod var_int_array {
    use super::*;

    pub fn encode(ids: &Vec<i32>, out: &mut Vec<u8>) {
        write_varint(out, ids.len() as i32);
        for id in ids {
            write_varint(out, *id);
        }
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<i32>> {
        let max = input.len();
        let size = read_count(input, i32::MAX as usize)?;
        if size > max {
            return Err(DecodeError::msg(format!(
                "VarIntArray with size {size} is bigger than allowed {max}"
            )));
        }
        (0..size).map(|_| read_varint(input)).collect()
    }
}

/// `ClientboundAnimatePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "animate")]
pub struct ClientboundAnimatePacket {
    #[mc(var)]
    pub id: i32,
    /// Unsigned byte (`WAKE_UP` 0, `CRITICAL_HIT` 1, `MAGIC_CRITICAL_HIT` 2).
    pub action: u8,
}

/// `ClientboundSwingAnimationPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "swing_animation")]
pub struct ClientboundSwingAnimationPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub hand: InteractionHand,
    pub animation: SwingAnimation,
}

/// `ClientboundEntityEventPacket` (`entity_id` is a plain big-endian int).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "entity_event")]
pub struct ClientboundEntityEventPacket {
    pub entity_id: i32,
    pub event_id: i8,
}

/// `ClientboundHurtAnimationPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "hurt_animation")]
pub struct ClientboundHurtAnimationPacket {
    #[mc(var)]
    pub id: i32,
    pub yaw: f32,
}

/// `ClientboundDamageEventPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "damage_event")]
pub struct ClientboundDamageEventPacket {
    #[mc(var)]
    pub entity_id: i32,
    /// `Holder<DamageType>` (data-driven registry: raw registry id).
    #[mc(var)]
    pub source_type: i32,
    /// `-1` = no entity.
    #[mc(with = optional_entity_id)]
    pub source_cause_id: i32,
    /// `-1` = no entity.
    #[mc(with = optional_entity_id)]
    pub source_direct_id: i32,
    pub source_position: Option<Vec3>,
}

/// `VecDelta.Stepped.DeltaStep`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct VecDeltaStep {
    #[mc(var)]
    pub ticks: i32,
    pub xa: i16,
    pub ya: i16,
    pub za: i16,
}

/// `VecDelta`: position change in 1/4096 block units as shorts, either one delta or several timed steps.
/// The step count travels in the packet's property field (see [`ClientboundMoveEntityPosPacket`]), so this type has
/// no codec of its own; an empty `Stepped` is indistinguishable from `Linear` to a reader (vanilla quirk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VecDelta {
    Linear { xa: i16, ya: i16, za: i16 },
    Stepped(Vec<VecDeltaStep>),
}

impl VecDelta {
    pub const ZERO: Self = Self::Linear {
        xa: 0,
        ya: 0,
        za: 0,
    };

    pub fn step_count(&self) -> usize {
        match self {
            Self::Linear { .. } => 0,
            Self::Stepped(steps) => steps.len(),
        }
    }

    /// `VecDelta.write`
    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Linear { xa, ya, za } => {
                xa.encode(out);
                ya.encode(out);
                za.encode(out);
            }
            Self::Stepped(steps) => steps.iter().for_each(|s| s.encode(out)),
        }
    }

    /// `VecDelta.read`: a step count of 0 reads a `Linear`, otherwise that many steps (at least 7 bytes each).
    fn read(input: &mut &[u8], step_count: usize) -> DecodeResult<Self> {
        if step_count == 0 {
            return Ok(Self::Linear {
                xa: i16::decode(input)?,
                ya: i16::decode(input)?,
                za: i16::decode(input)?,
            });
        }
        let max = input.len() / 7;
        if step_count > max {
            return Err(DecodeError::msg(format!(
                "VecDelta with size {step_count} is bigger than allowed {max}"
            )));
        }
        (0..step_count)
            .map(|_| VecDeltaStep::decode(input))
            .collect::<DecodeResult<_>>()
            .map(Self::Stepped)
    }
}

/// `ClientboundMoveEntityPacket` property field: bit 0 = on ground, the rest the step count.
fn pack_properties(on_ground: bool, step_count: usize) -> i32 {
    i32::from(on_ground) | (step_count as i32) << 1
}

fn read_move(input: &mut &[u8]) -> DecodeResult<(i32, VecDelta, bool)> {
    let entity_id = read_varint(input)?;
    let properties = read_varint(input)?;
    let delta = VecDelta::read(input, (properties as u32 >> 1) as usize)?;
    Ok((entity_id, delta, properties & 1 != 0))
}

/// `ClientboundMoveEntityPacket.Pos`
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = clientbound, name = "move_entity_pos")]
pub struct ClientboundMoveEntityPosPacket {
    pub entity_id: i32,
    pub delta: VecDelta,
    pub on_ground: bool,
}

impl Encode for ClientboundMoveEntityPosPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.entity_id);
        write_varint(
            out,
            pack_properties(self.on_ground, self.delta.step_count()),
        );
        self.delta.write(out);
    }
}
impl Decode for ClientboundMoveEntityPosPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let (entity_id, delta, on_ground) = read_move(input)?;
        Ok(Self {
            entity_id,
            delta,
            on_ground,
        })
    }
}

/// `ClientboundMoveEntityPacket.PosRot` (`y_rot`/`x_rot` are the raw packed-degree bytes, `Mth.packDegrees`).
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = clientbound, name = "move_entity_pos_rot")]
pub struct ClientboundMoveEntityPosRotPacket {
    pub entity_id: i32,
    pub delta: VecDelta,
    pub y_rot: i8,
    pub x_rot: i8,
    pub on_ground: bool,
}

impl Encode for ClientboundMoveEntityPosRotPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.entity_id);
        write_varint(
            out,
            pack_properties(self.on_ground, self.delta.step_count()),
        );
        self.delta.write(out);
        self.y_rot.encode(out);
        self.x_rot.encode(out);
    }
}
impl Decode for ClientboundMoveEntityPosRotPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let (entity_id, delta, on_ground) = read_move(input)?;
        Ok(Self {
            entity_id,
            delta,
            y_rot: i8::decode(input)?,
            x_rot: i8::decode(input)?,
            on_ground,
        })
    }
}

/// `ClientboundMoveEntityPacket.Rot` (`y_rot`/`x_rot` are the raw packed-degree bytes).
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = clientbound, name = "move_entity_rot")]
pub struct ClientboundMoveEntityRotPacket {
    pub entity_id: i32,
    pub on_ground: bool,
    pub y_rot: i8,
    pub x_rot: i8,
}

impl Encode for ClientboundMoveEntityRotPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.entity_id);
        self.on_ground.encode(out);
        self.y_rot.encode(out);
        self.x_rot.encode(out);
    }
}
impl Decode for ClientboundMoveEntityRotPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self {
            entity_id: read_varint(input)?,
            on_ground: bool::decode(input)?,
            y_rot: i8::decode(input)?,
            x_rot: i8::decode(input)?,
        })
    }
}

/// `NewMinecartBehavior.MinecartStep`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MinecartStep {
    pub position: Vec3,
    pub movement: Vec3,
    pub y_rot: Angle,
    pub x_rot: Angle,
    pub weight: f32,
}

/// `ClientboundMoveMinecartPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "move_minecart_along_track")]
pub struct ClientboundMoveMinecartPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub lerp_steps: Vec<MinecartStep>,
}

/// `PositionStep`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct PositionStep {
    pub position: Vec3,
    #[mc(var)]
    pub tick_offset: i32,
}

/// `PositionPath`: type id (`ByIdMap` ZERO) then either the end position or the list of steps.
/// `Stepped` carries only the steps on the wire (vanilla derives `endPosition` from the last one and rejects an empty list).
#[derive(Debug, Clone, PartialEq)]
pub enum PositionPath {
    Linear(Vec3),
    Stepped(Vec<PositionStep>),
}

impl Encode for PositionPath {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Linear(end) => {
                write_varint(out, 0);
                end.encode(out);
            }
            Self::Stepped(steps) => {
                write_varint(out, 1);
                steps.encode(out);
            }
        }
    }
}
impl Decode for PositionPath {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if read_varint(input)? != 1 {
            return Vec3::decode(input).map(Self::Linear);
        }
        let steps = Vec::<PositionStep>::decode(input)?;
        if steps.is_empty() {
            return Err(DecodeError::msg("java.util.NoSuchElementException"));
        }
        Ok(Self::Stepped(steps))
    }
}

/// `ClientboundEntityPositionSyncPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "entity_position_sync")]
pub struct ClientboundEntityPositionSyncPacket {
    #[mc(var)]
    pub id: i32,
    pub position: PositionPath,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// `ClientboundTeleportEntityPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "teleport_entity")]
pub struct ClientboundTeleportEntityPacket {
    #[mc(var)]
    pub id: i32,
    pub change: PositionMoveRotation,
    pub relatives: Relatives,
    pub on_ground: bool,
}

/// `ClientboundRotateHeadPacket` (`y_head_rot` is the raw packed-degree byte).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "rotate_head")]
pub struct ClientboundRotateHeadPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub y_head_rot: i8,
}

/// `ClientboundSetEntityMotionPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_entity_motion")]
pub struct ClientboundSetEntityMotionPacket {
    #[mc(var)]
    pub id: i32,
    pub movement: LpVec3,
}

/// `ClientboundSetEntityLinkPacket` (plain ints; `dest_id` 0 = detach)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_entity_link")]
pub struct ClientboundSetEntityLinkPacket {
    pub source_id: i32,
    pub dest_id: i32,
}

/// `ClientboundSetPassengersPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_passengers")]
pub struct ClientboundSetPassengersPacket {
    #[mc(var)]
    pub vehicle: i32,
    #[mc(with = var_int_array)]
    pub passengers: Vec<i32>,
}

/// `ClientboundRemoveEntitiesPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "remove_entities")]
pub struct ClientboundRemoveEntitiesPacket {
    #[mc(var)]
    pub entity_ids: Vec<i32>,
}

/// `ClientboundTakeItemEntityPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "take_item_entity")]
pub struct ClientboundTakeItemEntityPacket {
    #[mc(var)]
    pub item_id: i32,
    #[mc(var)]
    pub player_id: i32,
    #[mc(var)]
    pub amount: i32,
}

/// `ClientboundUpdateAttributesPacket.AttributeSnapshot`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttributeSnapshot {
    pub attribute: mc_data::Attribute,
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
}

/// `ClientboundUpdateAttributesPacket` (at most 128 snapshots)
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "update_attributes")]
pub struct ClientboundUpdateAttributesPacket {
    #[mc(var)]
    pub entity_id: i32,
    #[mc(max = 128)]
    pub attributes: Vec<AttributeSnapshot>,
}

/// `ClientboundUpdateMobEffectPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "update_mob_effect")]
pub struct ClientboundUpdateMobEffectPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub effect: mc_data::MobEffect,
    #[mc(var)]
    pub effect_amplifier: i32,
    #[mc(var)]
    pub effect_duration_ticks: i32,
    /// Bit set of `FLAG_*`; unknown bits are carried through like vanilla's raw byte.
    pub flags: u8,
}

impl ClientboundUpdateMobEffectPacket {
    pub const FLAG_AMBIENT: u8 = 1;
    pub const FLAG_VISIBLE: u8 = 2;
    pub const FLAG_SHOW_ICON: u8 = 4;
    pub const FLAG_BLEND: u8 = 8;

    pub fn is_ambient(&self) -> bool {
        self.flags & Self::FLAG_AMBIENT != 0
    }
    pub fn is_visible(&self) -> bool {
        self.flags & Self::FLAG_VISIBLE != 0
    }
    pub fn shows_icon(&self) -> bool {
        self.flags & Self::FLAG_SHOW_ICON != 0
    }
    pub fn should_blend(&self) -> bool {
        self.flags & Self::FLAG_BLEND != 0
    }
}

/// `ClientboundRemoveMobEffectPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "remove_mob_effect")]
pub struct ClientboundRemoveMobEffectPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub effect: mc_data::MobEffect,
}

/// `ClientboundSetCameraPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_camera")]
pub struct ClientboundSetCameraPacket {
    #[mc(var)]
    pub camera_id: i32,
}

/// `ClientboundMountScreenOpenPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "mount_screen_open")]
pub struct ClientboundMountScreenOpenPacket {
    #[mc(var)]
    pub container_id: i32,
    #[mc(var)]
    pub inventory_columns: i32,
    /// Plain big-endian int.
    pub entity_id: i32,
}

/// `ClientboundProjectilePowerPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "projectile_power")]
pub struct ClientboundProjectilePowerPacket {
    #[mc(var)]
    pub id: i32,
    pub acceleration_power: f64,
}

/// `ClientboundPlayerPositionPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_position")]
pub struct ClientboundPlayerPositionPacket {
    #[mc(var)]
    pub id: i32,
    pub change: PositionMoveRotation,
    pub relatives: Relatives,
}

/// `ClientboundPlayerRotationPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_rotation")]
pub struct ClientboundPlayerRotationPacket {
    pub y_rot: f32,
    pub relative_y: bool,
    pub x_rot: f32,
    pub relative_x: bool,
}

/// The entity part of `ClientboundPlayerLookAtPacket` (sent after the `atEntity` flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PlayerLookAtEntity {
    #[mc(var)]
    pub entity: i32,
    pub to_anchor: Anchor,
}

/// `ClientboundPlayerLookAtPacket`: `x`/`y`/`z` are always sent (for an entity target: its anchor position).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "player_look_at")]
pub struct ClientboundPlayerLookAtPacket {
    pub from_anchor: Anchor,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub at_entity: Option<PlayerLookAtEntity>,
}

/// `ClientboundMoveVehiclePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "move_vehicle")]
pub struct ClientboundMoveVehiclePacket {
    pub moving_to: PositionAndRotation,
}

/// `ClientboundPlayerAbilitiesPacket`: flags byte (invulnerable 1, flying 2, can fly 4, instabuild 8) + two floats.
#[derive(Debug, Clone, PartialEq, Packet)]
#[packet(state = play, flow = clientbound, name = "player_abilities")]
pub struct ClientboundPlayerAbilitiesPacket {
    pub invulnerable: bool,
    pub is_flying: bool,
    pub can_fly: bool,
    pub instabuild: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

impl Encode for ClientboundPlayerAbilitiesPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        let flags = u8::from(self.invulnerable)
            | u8::from(self.is_flying) << 1
            | u8::from(self.can_fly) << 2
            | u8::from(self.instabuild) << 3;
        flags.encode(out);
        self.flying_speed.encode(out);
        self.walking_speed.encode(out);
    }
}
impl Decode for ClientboundPlayerAbilitiesPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let flags = u8::decode(input)?;
        Ok(Self {
            invulnerable: flags & 1 != 0,
            is_flying: flags & 2 != 0,
            can_fly: flags & 4 != 0,
            instabuild: flags & 8 != 0,
            flying_speed: f32::decode(input)?,
            walking_speed: f32::decode(input)?,
        })
    }
}
