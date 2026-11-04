//! Composite codecs: `Either`, holders, angles, UUID, GameProfile, instants, signatures.
use mc_core::ident::Identifier;
use uuid::Uuid;

use super::{
    read_count, read_varint, take, take_array, write_varint, Decode, DecodeBounded, DecodeError, DecodeResult, Encode,
};

/// `ByteBufCodecs.either`: `true` = left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L: Encode, R: Encode> Encode for Either<L, R> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Left(l) => {
                out.push(1);
                l.encode(out)
            }
            Self::Right(r) => {
                out.push(0);
                r.encode(out)
            }
        }
    }
}
impl<L: Decode, R: Decode> Decode for Either<L, R> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? { L::decode(input).map(Self::Left) } else { R::decode(input).map(Self::Right) }
    }
}

/// `ByteBufCodecs.holder`: `0` + inline value, or `registry id + 1`. Registry ids are protocol ids
/// of the (possibly dynamic) registry; resolving them is up to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder<T> {
    Reference(i32),
    Direct(T),
}

impl<T: Encode> Encode for Holder<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Reference(id) => write_varint(out, id.wrapping_add(1)),
            Self::Direct(v) => {
                write_varint(out, 0);
                v.encode(out)
            }
        }
    }
}
impl<T: Decode> Decode for Holder<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match read_varint(input)? {
            0 => T::decode(input).map(Self::Direct),
            id => Ok(Self::Reference(id.wrapping_sub(1))),
        }
    }
}

/// `ByteBufCodecs.holderSet`: `0` + tag identifier, or `count + 1` followed by registry ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HolderSet {
    Named(Identifier),
    Direct(Vec<i32>),
}

impl Encode for HolderSet {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Named(tag) => {
                write_varint(out, 0);
                tag.encode(out)
            }
            Self::Direct(ids) => {
                write_varint(out, ids.len() as i32 + 1);
                for id in ids {
                    write_varint(out, *id);
                }
            }
        }
    }
}
impl Decode for HolderSet {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = i64::from(read_varint(input)?) - 1;
        if count == -1 {
            return Identifier::decode(input).map(Self::Named);
        }
        if count < -1 {
            return Err(DecodeError::Message(format!("Illegal Capacity: {count}")));
        }
        let mut ids = Vec::with_capacity((count as usize).min(65536));
        for _ in 0..count {
            ids.push(read_varint(input)?);
        }
        Ok(Self::Direct(ids))
    }
}

/// `ByteBufCodecs.ROTATION_BYTE`: degrees packed into a byte (256 steps).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Angle(pub f32);

impl Angle {
    /// `Mth.packDegrees`
    pub fn pack(deg: f32) -> i8 {
        ((deg * 256.0 / 360.0).floor() as i32) as i8
    }
    /// `Mth.unpackDegrees`
    pub fn unpack(b: i8) -> f32 {
        (i32::from(b) * 360) as f32 / 256.0
    }
}
impl Encode for Angle {
    fn encode(&self, out: &mut Vec<u8>) {
        Self::pack(self.0).encode(out)
    }
}
impl Decode for Angle {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i8::decode(input).map(|b| Self(Self::unpack(b)))
    }
}

/// `ByteBufCodecs.INSTANT`: epoch milliseconds as a long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Instant(pub i64);

impl Encode for Instant {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode(out)
    }
}
impl Decode for Instant {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i64::decode(input).map(Self)
    }
}

impl Encode for Uuid {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes())
    }
}
impl Decode for Uuid {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Uuid::from_bytes(take_array(input)?))
    }
}

/// `ByteBufCodecs.GAME_PROFILE_PROPERTIES` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

/// `ByteBufCodecs.GAME_PROFILE`: UUID, name (max 16), properties (max 16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub id: Uuid,
    pub name: String,
    pub properties: Vec<Property>,
}

impl Encode for GameProfile {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        self.name.encode(out);
        write_varint(out, self.properties.len() as i32);
        for p in &self.properties {
            p.name.encode(out);
            p.value.encode(out);
            p.signature.encode(out);
        }
    }
}
/// `ByteBufCodecs.GAME_PROFILE_PROPERTIES` count: at most 16; a negative count (accepted by vanilla) reads no properties.
pub(crate) fn read_property_count(input: &mut &[u8]) -> DecodeResult<usize> {
    let count = read_varint(input)?;
    if count > 16 {
        return Err(DecodeError::Message(format!("{count} elements exceeded max size of: 16")));
    }
    Ok(usize::try_from(count).unwrap_or(0))
}

/// `ImmutableMultimap` iteration order of properties: keys by first insertion, values per key by insertion.
pub(crate) fn group_properties_by_name(props: Vec<Property>) -> Vec<Property> {
    let mut names: Vec<&str> = Vec::new();
    for p in &props {
        if !names.contains(&p.name.as_str()) {
            names.push(&p.name);
        }
    }
    let order: Vec<String> = names.into_iter().map(str::to_string).collect();
    order.iter().flat_map(|n| props.iter().filter(move |p| &p.name == n).cloned().collect::<Vec<_>>()).collect()
}

impl Decode for GameProfile {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = Uuid::decode(input)?;
        let name = String::decode_bounded(input, 16)?;
        let count = read_property_count(input)?;
        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            properties.push(Property {
                name: String::decode_bounded(input, 64)?,
                value: String::decode_bounded(input, 32767)?,
                signature: Option::<String>::decode_bounded(input, 1024)?,
            });
        }
        Ok(Self { id, name, properties: group_properties_by_name(properties) })
    }
}

/// `MessageSignature`: 256 raw bytes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; 256]);

impl std::fmt::Debug for MessageSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MessageSignature({:02x?}..)", &self.0[..4])
    }
}
impl Encode for MessageSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0)
    }
}
impl Decode for MessageSignature {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self(take(input, 256)?.try_into().expect("length checked")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{from_bytes, to_bytes};

    #[test]
    fn angle_matches_mth() {
        assert_eq!(Angle::pack(90.0), 64);
        assert_eq!(Angle::pack(-90.0), -64);
        assert_eq!(Angle::pack(180.0), -128);
        assert_eq!(Angle::pack(359.9), 255u8 as i8);
        assert_eq!(Angle::unpack(-128), -180.0);
        assert_eq!(from_bytes::<Angle>(&to_bytes(&Angle(45.0))).unwrap(), Angle(45.0));
    }

    #[test]
    fn holder_ids() {
        assert_eq!(to_bytes(&Holder::<u8>::Reference(4)), [5]);
        assert_eq!(to_bytes(&Holder::Direct(9u8)), [0, 9]);
        assert_eq!(from_bytes::<Holder<u8>>(&[0, 9]).unwrap(), Holder::Direct(9));
        assert_eq!(from_bytes::<Holder<u8>>(&[5]).unwrap(), Holder::Reference(4));
    }
}

/// The remainder of the frame as raw bytes, at most `MAX` (vanilla `DiscardedPayload` size check).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rest<const MAX: usize>(pub Vec<u8>);

impl<const MAX: usize> Encode for Rest<MAX> {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0)
    }
}
impl<const MAX: usize> Decode for Rest<MAX> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if input.len() > MAX {
            return Err(DecodeError::Message(format!("Payload may not be larger than {MAX} bytes")));
        }
        Ok(Self(std::mem::take(input).to_vec()))
    }
}

/// `ByteBufCodecs.OPTIONAL_VAR_INT`: `0` = empty, otherwise `value + 1` (wrapping, as in Java).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OptionalVarInt(pub Option<i32>);

impl Encode for OptionalVarInt {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.0.map_or(0, |v| v.wrapping_add(1)))
    }
}
impl Decode for OptionalVarInt {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self(match read_varint(input)? {
            0 => None,
            i => Some(i.wrapping_sub(1)),
        }))
    }
}

/// `ByteBufCodecs.RGB_COLOR`: three bytes; decoded colors are opaque ARGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RgbColor(pub i32);

impl Encode for RgbColor {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&[(self.0 >> 16) as u8, (self.0 >> 8) as u8, self.0 as u8])
    }
}
impl Decode for RgbColor {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let [r, g, b] = take_array(input)?;
        Ok(Self(i32::from_be_bytes([255, r, g, b])))
    }
}

/// `GlobalPos.STREAM_CODEC`: dimension key + block position.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlobalPos {
    pub dimension: Identifier,
    pub pos: mc_core::pos::block_pos::BlockPos,
}

impl Encode for GlobalPos {
    fn encode(&self, out: &mut Vec<u8>) {
        self.dimension.encode(out);
        self.pos.encode(out);
    }
}
impl Decode for GlobalPos {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self { dimension: Identifier::decode(input)?, pos: Decode::decode(input)? })
    }
}

/// `ByteBufCodecs.CONTAINER_ID`: a VarInt.
pub type ContainerId = super::VarInt;

/// `ByteBufCodecs.lengthPrefixed(MAX)`: the inner value is encoded into its own buffer, preceded by
/// that buffer's VarInt size (at most `MAX`). Decoding hands the inner decoder exactly that slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LengthPrefixed<T, const MAX: usize>(pub T);

impl<T: Encode, const MAX: usize> Encode for LengthPrefixed<T, MAX> {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut inner = Vec::new();
        self.0.encode(&mut inner);
        write_varint(out, inner.len() as i32);
        out.extend_from_slice(&inner);
    }
}
impl<T: Decode, const MAX: usize> Decode for LengthPrefixed<T, MAX> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let size = read_varint(input)?;
        if size > MAX as i32 {
            return Err(DecodeError::Message(format!("Buffer size {size} is larger than allowed limit of {MAX}")));
        }
        let size = usize::try_from(size).map_err(|_| DecodeError::Message(format!("Buffer size {size} is negative")))?;
        T::decode(&mut take(input, size)?).map(Self)
    }
}

/// `Vector3f` on the wire: three floats.
pub type Vector3f = [f32; 3];
/// `Quaternionf` on the wire: x, y, z, w floats.
pub type Quaternionf = [f32; 4];
