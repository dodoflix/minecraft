//! Small wire types shared by several play packet groups.
use mc_core::phys::vec3::Vec3;

use mc_core::ident::Identifier;

use crate::codec::{read_varint, write_varint, DecodeResult};
use crate::{Decode, Encode};
use crate::packets::common::id_enum;

id_enum!(
    /// `GameType` (`GameType.STREAM_CODEC`: VarInt id, unknown ids = survival)
    GameType { Survival = 0, Creative = 1, Adventure = 2, Spectator = 3 } out_of_bounds = ZERO
);
id_enum!(
    /// `Difficulty` (`Difficulty.STREAM_CODEC`: VarInt id, WRAP)
    Difficulty { Peaceful = 0, Easy = 1, Normal = 2, Hard = 3 } out_of_bounds = WRAP
);
id_enum!(
    /// `InteractionHand` (VarInt id, unknown = main hand)
    InteractionHand { MainHand = 0, OffHand = 1 } out_of_bounds = ZERO
);
id_enum!(
    /// `EquipmentSlot.STREAM_CODEC`: VarInt *slot id* (not the Java ordinal), unknown = main hand.
    EquipmentSlot { MainHand = 0, Feet = 1, Legs = 2, Chest = 3, Head = 4, OffHand = 5, Body = 6, Saddle = 7 } out_of_bounds = ZERO
);

/// `PositionMoveRotation.STREAM_CODEC`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct PositionMoveRotation {
    pub position: Vec3,
    pub delta_movement: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// `Relative`: bit positions of the `Set<Relative>` int mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relative {
    X = 0,
    Y,
    Z,
    YRot,
    XRot,
    DeltaX,
    DeltaY,
    DeltaZ,
    RotateDelta,
}

/// `Set<Relative>` as vanilla puts it on the wire (`Relative.SET_STREAM_CODEC`): an int bit mask; bits
/// without a `Relative` are dropped on decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Relatives(u16);

impl Relatives {
    pub const NONE: Self = Self(0);
    /// `Relative.ALL`
    pub const ALL: Self = Self(0x1ff);
    /// `Relative.ROTATION`
    pub const ROTATION: Self = Self(1 << Relative::XRot as u16 | 1 << Relative::YRot as u16);
    /// `Relative.DELTA`
    pub const DELTA: Self = Self(
        1 << Relative::DeltaX as u16 | 1 << Relative::DeltaY as u16 | 1 << Relative::DeltaZ as u16 | 1 << Relative::RotateDelta as u16,
    );

    pub fn contains(self, r: Relative) -> bool {
        self.0 >> r as u16 & 1 != 0
    }
    pub fn insert(&mut self, r: Relative) {
        self.0 |= 1 << r as u16;
    }
    pub fn with(mut self, r: Relative) -> Self {
        self.insert(r);
        self
    }
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// `Relative.pack`
    pub fn pack(self) -> i32 {
        i32::from(self.0)
    }
    /// `Relative.unpack`
    pub fn unpack(v: i32) -> Self {
        Self((v & 0x1ff) as u16)
    }
}

impl FromIterator<Relative> for Relatives {
    fn from_iter<I: IntoIterator<Item = Relative>>(it: I) -> Self {
        it.into_iter().fold(Self::NONE, Self::with)
    }
}

impl Encode for Relatives {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pack().encode(out)
    }
}
impl Decode for Relatives {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i32::decode(input).map(Self::unpack)
    }
}

/// `SoundEvent.DIRECT_STREAM_CODEC`: an inline sound (identifier + optional fixed attenuation range).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DirectSoundEvent {
    pub location: Identifier,
    pub fixed_range: Option<f32>,
}

/// `SoundEvent.STREAM_CODEC` (`Holder<SoundEvent>`): registry id + 1, or `0` + an inline sound.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundEventHolder {
    Reference(mc_data::SoundEvent),
    Direct(DirectSoundEvent),
}

impl Encode for SoundEventHolder {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Reference(s) => write_varint(out, s.id() as i32 + 1),
            Self::Direct(d) => {
                write_varint(out, 0);
                d.encode(out)
            }
        }
    }
}
impl Decode for SoundEventHolder {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let mut peek = *input;
        if read_varint(&mut peek)? == 0 {
            *input = peek;
            return DirectSoundEvent::decode(input).map(Self::Direct);
        }
        // `byIdOrThrow(id - 1)`: a registry error mentions `id - 1`.
        let id = read_varint(input)?.wrapping_sub(1);
        u32::try_from(id)
            .ok()
            .and_then(mc_data::SoundEvent::from_id)
            .map(Self::Reference)
            .ok_or_else(|| crate::codec::DecodeError::Message(format!("No value with id {id}")))
    }
}
