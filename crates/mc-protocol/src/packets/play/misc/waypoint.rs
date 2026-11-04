//! `ClientboundTrackedWaypointPacket` and `TrackedWaypoint`.
use mc_core::ident::Identifier;
use mc_core::pos::{chunk_pos::ChunkPos, vec3i::Vec3i};
use uuid::Uuid;

use crate::codec::{read_varint, write_varint, DecodeError, DecodeResult, Either, RgbColor};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `ClientboundTrackedWaypointPacket.Operation` (`BY_ID`: VarInt ordinal, WRAP)
    WaypointOperation { Track = 0, Untrack = 1, Update = 2 } out_of_bounds = WRAP
);

/// `Waypoint.Icon.STREAM_CODEC`: style asset id and optional RGB color (decoded colors are opaque ARGB).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct WaypointIcon {
    pub style: Identifier,
    pub color: Option<RgbColor>,
}

impl WaypointIcon {
    /// `Waypoint.Icon.NULL`: the default style, no color.
    pub fn null() -> Self {
        Self { style: Identifier::with_default_namespace("default").expect("valid"), color: None }
    }
}

/// The type-dependent payload of a `TrackedWaypoint` (`TrackedWaypoint.Type` ordinal = variant id).
#[derive(Debug, Clone, PartialEq)]
pub enum TrackedWaypointData {
    Empty,
    Vec3i(Vec3i),
    Chunk(ChunkPos),
    /// Angle in radians.
    Azimuth(f32),
}

/// `TrackedWaypoint.STREAM_CODEC`: UUID-or-name, icon, type, payload.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedWaypoint {
    pub identifier: Either<Uuid, String>,
    pub icon: WaypointIcon,
    pub data: TrackedWaypointData,
}

impl TrackedWaypoint {
    /// `TrackedWaypoint.empty`
    pub fn empty(identifier: Uuid) -> Self {
        Self { identifier: Either::Left(identifier), icon: WaypointIcon::null(), data: TrackedWaypointData::Empty }
    }
}

impl Encode for TrackedWaypoint {
    fn encode(&self, out: &mut Vec<u8>) {
        self.identifier.encode(out);
        self.icon.encode(out);
        match &self.data {
            TrackedWaypointData::Empty => write_varint(out, 0),
            TrackedWaypointData::Vec3i(v) => {
                write_varint(out, 1);
                v.encode(out)
            }
            TrackedWaypointData::Chunk(c) => {
                write_varint(out, 2);
                write_varint(out, c.x);
                write_varint(out, c.z);
            }
            TrackedWaypointData::Azimuth(angle) => {
                write_varint(out, 3);
                angle.encode(out)
            }
        }
    }
}
impl Decode for TrackedWaypoint {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let identifier = Either::decode(input)?;
        let icon = WaypointIcon::decode(input)?;
        let data = match read_varint(input)? {
            0 => TrackedWaypointData::Empty,
            1 => TrackedWaypointData::Vec3i(Vec3i::decode(input)?),
            2 => TrackedWaypointData::Chunk(ChunkPos::new(read_varint(input)?, read_varint(input)?)),
            3 => TrackedWaypointData::Azimuth(f32::decode(input)?),
            id => return Err(DecodeError::Message(format!("Index {id} out of bounds for length 4"))),
        };
        Ok(Self { identifier, icon, data })
    }
}

/// `ClientboundTrackedWaypointPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "waypoint")]
pub struct ClientboundTrackedWaypointPacket {
    pub operation: WaypointOperation,
    pub waypoint: TrackedWaypoint,
}
