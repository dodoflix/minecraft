//! Codecs for `mc-core` value types.
use crate::codec::EncodeResult;
use mc_core::{
    ident::Identifier,
    phys::vec3::Vec3,
    pos::{block_pos::BlockPos, chunk_pos::ChunkPos, section_pos::SectionPos, vec3i::Vec3i},
};

use super::{
    Decode, DecodeError, DecodeResult, Encode, MAX_STRING_LENGTH, read_utf, read_varint, take,
    write_varint,
};

/// `Identifier.STREAM_CODEC`: the `namespace:path` string.
impl Encode for Identifier {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        super::write_utf(out, &self.to_string(), MAX_STRING_LENGTH)?;
        Ok(())
    }
}
impl Decode for Identifier {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let s = read_utf(input, MAX_STRING_LENGTH)?;
        Identifier::parse(&s).map_err(|e| DecodeError::Message(e.to_string()))
    }
}

/// `Vec3.STREAM_CODEC`: three doubles.
impl Encode for Vec3 {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.x.encode(out)?;
        self.y.encode(out)?;
        self.z.encode(out)?;
        Ok(())
    }
}
impl Decode for Vec3 {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Vec3::new(
            f64::decode(input)?,
            f64::decode(input)?,
            f64::decode(input)?,
        ))
    }
}

/// `Vec3.LP_STREAM_CODEC` (`net.minecraft.network.LpVec3`): low-precision Vec3 (15-bit mantissas + shared scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LpVec3(pub Vec3);

impl LpVec3 {
    pub const ABS_MAX_VALUE: f64 = 1.7179869183E10;
    pub const ABS_MIN_VALUE: f64 = 3.051944088384301E-5;

    fn sanitize(v: f64) -> f64 {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-Self::ABS_MAX_VALUE, Self::ABS_MAX_VALUE)
        }
    }
    /// Java `Math.round` on the non-negative values used here.
    fn pack(v: f64) -> i64 {
        ((v * 0.5 + 0.5) * 32766.0).round() as i64
    }
    fn unpack(v: i64) -> f64 {
        ((v & 32767) as f64).min(32766.0) * 2.0 / 32766.0 - 1.0
    }
}

impl Encode for LpVec3 {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let (x, y, z) = (
            Self::sanitize(self.0.x),
            Self::sanitize(self.0.y),
            Self::sanitize(self.0.z),
        );
        let chessboard = x.abs().max(y.abs().max(z.abs()));
        if chessboard < Self::ABS_MIN_VALUE {
            out.push(0);
            return Ok(());
        }
        // Mth.ceilLong
        let scale = {
            let l = chessboard as i64;
            if chessboard > l as f64 { l + 1 } else { l }
        };
        let partial = scale & 3 != scale;
        let markers = if partial { scale & 3 | 4 } else { scale };
        let buffer = markers
            | Self::pack(x / scale as f64) << 3
            | Self::pack(y / scale as f64) << 18
            | Self::pack(z / scale as f64) << 33;
        out.push(buffer as u8);
        out.push((buffer >> 8) as u8);
        ((buffer >> 16) as i32).encode(out)?;
        if partial {
            write_varint(out, (scale >> 2) as i32);
        };
        Ok(())
    }
}
impl Decode for LpVec3 {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let lowest = take(input, 1)?[0];
        if lowest == 0 {
            return Ok(Self(Vec3::ZERO));
        }
        let middle = take(input, 1)?[0];
        let highest = u32::decode(input)?;
        let buffer = i64::from(highest) << 16 | i64::from(middle) << 8 | i64::from(lowest);
        let mut scale = i64::from(lowest & 3);
        if lowest & 4 == 4 {
            scale |= i64::from(read_varint(input)? as u32) << 2;
        }
        let s = scale as f64;
        Ok(Self(Vec3::new(
            Self::unpack(buffer >> 3) * s,
            Self::unpack(buffer >> 18) * s,
            Self::unpack(buffer >> 33) * s,
        )))
    }
}

/// `BlockPos.STREAM_CODEC`: packed long.
impl Encode for BlockPos {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.as_long().encode(out)?;
        Ok(())
    }
}
impl Decode for BlockPos {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i64::decode(input).map(BlockPos::of_long)
    }
}

/// `ChunkPos.STREAM_CODEC`: packed long.
impl Encode for ChunkPos {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.pack().encode(out)?;
        Ok(())
    }
}
impl Decode for ChunkPos {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i64::decode(input).map(ChunkPos::unpack)
    }
}

/// `SectionPos.STREAM_CODEC`: packed long.
impl Encode for SectionPos {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.as_long().encode(out)?;
        Ok(())
    }
}
impl Decode for SectionPos {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i64::decode(input).map(SectionPos::of_long)
    }
}

/// `Vec3i.STREAM_CODEC`: three VarInts.
impl Encode for Vec3i {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.x);
        write_varint(out, self.y);
        write_varint(out, self.z);
        Ok(())
    }
}
impl Decode for Vec3i {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Vec3i {
            x: read_varint(input)?,
            y: read_varint(input)?,
            z: read_varint(input)?,
        })
    }
}

/// `Direction.STREAM_CODEC`: VarInt 3D data value (`ByIdMap.continuous`, WRAP).
impl Encode for mc_core::direction::Direction {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.get_3d_data_value() as i32);
        Ok(())
    }
}
impl Decode for mc_core::direction::Direction {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input).map(mc_core::direction::Direction::by_id)
    }
}
