//! Play packets: group `entity_data` — entity spawn/metadata, particles and explosions.
use mc_core::phys::vec3::Vec3;
use uuid::Uuid;

use super::SoundEventHolder;
use crate::codec::{DecodeError, DecodeResult, LpVec3, read_count, read_varint, write_varint};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

mod particle;
mod profile;
mod serializers;

pub use particle::*;
pub use profile::*;
pub use serializers::*;

/// `ClientboundAddEntityPacket`. The three angles are vanilla's packed bytes (`Mth.packDegrees`, see `Angle`).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "add_entity")]
pub struct ClientboundAddEntityPacket {
    #[mc(var)]
    pub id: i32,
    pub uuid: Uuid,
    pub r#type: mc_data::EntityType,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub movement: LpVec3,
    pub x_rot: i8,
    pub y_rot: i8,
    pub y_head_rot: i8,
    #[mc(var)]
    pub data: i32,
}

/// `ClientboundSetEntityDataPacket`: entity id, then the data values followed by the `0xFF` terminator.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_entity_data")]
pub struct ClientboundSetEntityDataPacket {
    #[mc(var)]
    pub id: i32,
    #[mc(with = packed_items)]
    pub packed_items: Vec<DataValue>,
}

impl ClientboundSetEntityDataPacket {
    pub const EOF_MARKER: u8 = 255;
}

mod packed_items {
    use super::*;

    pub fn encode(items: &Vec<DataValue>, out: &mut Vec<u8>) {
        for item in items {
            item.encode(out);
        }
        out.push(ClientboundSetEntityDataPacket::EOF_MARKER);
    }

    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<DataValue>> {
        let mut items = Vec::new();
        while input.first() != Some(&ClientboundSetEntityDataPacket::EOF_MARKER) {
            items.push(DataValue::decode(input)?);
        }
        *input = &input[1..];
        Ok(items)
    }
}

id_enum!(
    /// `ClientboundLevelParticlesPacket.RandomizationType` (VarInt id, unknown = default)
    RandomizationType { Default = 0, Alternative = 1, AlternativeWithSpeed = 2 } out_of_bounds = ZERO
);

/// `ClientboundLevelParticlesPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "level_particles")]
pub struct ClientboundLevelParticlesPacket {
    pub particle: ParticleOptions,
    pub override_limiter: bool,
    pub always_show: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x_dist: f32,
    pub y_dist: f32,
    pub z_dist: f32,
    pub x_max_speed: f32,
    pub y_max_speed: f32,
    pub z_max_speed: f32,
    #[mc(var)]
    pub count: i32,
    pub randomization_type: RandomizationType,
}

/// `ExplosionParticleInfo.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ExplosionParticleInfo {
    pub particle: ParticleOptions,
    pub scaling: f32,
    pub speed: f32,
}

/// `Weighted<T>` (`Weighted.streamCodec`: value, VarInt weight); decoding rejects a negative weight like the constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct Weighted<T> {
    pub value: T,
    pub weight: i32,
}

/// `WeightedList.streamCodec`: a plain VarInt-counted list of [`Weighted`]; decoding rejects total weights above
/// `i32::MAX` like `WeightedList`'s constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedList<T>(pub Vec<Weighted<T>>);

impl<T: Encode> Encode for WeightedList<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.len() as i32);
        for w in &self.0 {
            w.value.encode(out);
            write_varint(out, w.weight);
        }
    }
}
impl<T: Decode> Decode for WeightedList<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut items = Vec::with_capacity(count.min(65536));
        let mut total = 0i64;
        for _ in 0..count {
            let value = T::decode(input)?;
            let weight = read_varint(input)?;
            if weight < 0 {
                return Err(DecodeError::msg("Weight should be >= 0"));
            }
            total += i64::from(weight);
            items.push(Weighted { value, weight });
        }
        if total > i64::from(i32::MAX) {
            return Err(DecodeError::msg("Sum of weights must be <= 2147483647"));
        }
        Ok(Self(items))
    }
}

/// `ClientboundExplodePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "explode")]
pub struct ClientboundExplodePacket {
    pub center: Vec3,
    pub radius: f32,
    pub block_count: i32,
    pub player_knockback: Option<Vec3>,
    pub explosion_particle: ParticleOptions,
    pub explosion_sound: SoundEventHolder,
    pub block_particles: WeightedList<ExplosionParticleInfo>,
    pub play_sound: bool,
}
