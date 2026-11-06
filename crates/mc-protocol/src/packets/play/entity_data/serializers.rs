//! `SynchedEntityData.DataValue` and every `EntityDataSerializers` type (serializer ids = registration order).
use crate::codec::EncodeResult;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::Pose;
use mc_text::Component;
use uuid::Uuid;

use super::{ParticleOptions, ResolvableProfile};
use crate::codec::{
    DecodeError, DecodeResult, GlobalPos, Holder, OptionalVarInt, read_registry_id, read_varint,
    write_varint,
};
use crate::components::ItemStack;
use crate::packets::common::{HumanoidArm, id_enum};
use crate::{Decode, Encode};

/// `net.minecraft.core.Rotations.STREAM_CODEC`: three floats, normalised like the constructor
/// (non-finite => 0, otherwise `% 360`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotations {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Rotations {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        let wrap = |v: f32| if v.is_finite() { v % 360.0 } else { 0.0 };
        Self {
            x: wrap(x),
            y: wrap(y),
            z: wrap(z),
        }
    }
}

impl Encode for Rotations {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let n = Self::new(self.x, self.y, self.z);
        n.x.encode(out)?;
        n.y.encode(out)?;
        n.z.encode(out)?;
        Ok(())
    }
}
impl Decode for Rotations {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self::new(
            f32::decode(input)?,
            f32::decode(input)?,
            f32::decode(input)?,
        ))
    }
}

/// `VillagerData.STREAM_CODEC` (`level` is at least 1, as in the constructor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VillagerData {
    pub r#type: mc_data::VillagerType,
    pub profession: mc_data::VillagerProfession,
    pub level: i32,
}

impl Encode for VillagerData {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.r#type.encode(out)?;
        self.profession.encode(out)?;
        write_varint(out, self.level.max(1));
        Ok(())
    }
}
impl Decode for VillagerData {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        // `holderRegistry`: strict, unknown ids fail even though these registries are defaulted.
        Ok(Self {
            r#type: read_registry_id(input)?,
            profession: read_registry_id(input)?,
            level: read_varint(input)?.max(1),
        })
    }
}

/// `PaintingVariant.DIRECT_STREAM_CODEC` (inline painting variant of a `Holder<PaintingVariant>`).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PaintingVariant {
    #[mc(var)]
    pub width: i32,
    #[mc(var)]
    pub height: i32,
    pub asset_id: Identifier,
    pub title: Option<Component>,
    pub author: Option<Component>,
}

id_enum!(
    /// `Armadillo.ArmadilloState` (VarInt id, unknown = idle)
    ArmadilloState { Idle = 0, Rolling = 1, Scared = 2, Unrolling = 3 } out_of_bounds = ZERO
);
id_enum!(
    /// `Sniffer.State` (VarInt id, unknown = idling)
    SnifferState { Idling = 0, FeelingHappy = 1, Scenting = 2, Sniffing = 3, Searching = 4, Digging = 5, Rising = 6 } out_of_bounds = ZERO
);
id_enum!(
    /// `CopperGolemState` (VarInt id, unknown = idle)
    CopperGolemState { Idle = 0, GettingItem = 1, GettingNoItem = 2, DroppingItem = 3, DroppingNoItem = 4 } out_of_bounds = ZERO
);
id_enum!(
    /// `DyeColor` (VarInt id, unknown = white)
    DyeColor {
        White = 0, Orange = 1, Magenta = 2, LightBlue = 3, Yellow = 4, Lime = 5, Pink = 6, Gray = 7, LightGray = 8, Cyan = 9,
        Purple = 10, Blue = 11, Brown = 12, Green = 13, Red = 14, Black = 15
    } out_of_bounds = ZERO
);

/// `WeatheringCopper.WeatherState.STREAM_CODEC`: VarInt ordinal, out-of-range ids are clamped (`ByIdMap` CLAMP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeatheringCopperState {
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl WeatheringCopperState {
    const VALUES: [Self; 4] = [
        Self::Unaffected,
        Self::Exposed,
        Self::Weathered,
        Self::Oxidized,
    ];
}
impl Encode for WeatheringCopperState {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, *self as i32);
        Ok(())
    }
}
impl Decode for WeatheringCopperState {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self::VALUES[read_varint(input)?.clamp(0, 3) as usize])
    }
}

/// `Pose.STREAM_CODEC`: VarInt id (= ordinal), unknown = standing.
mod pose {
    use super::*;

    pub fn encode(p: &Pose, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, p.ordinal() as i32);
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Pose> {
        let id = read_varint(input)?;
        Ok(usize::try_from(id)
            .ok()
            .and_then(Pose::from_ordinal)
            .unwrap_or(Pose::Standing))
    }
}

/// `EntityDataSerializers.OPTIONAL_BLOCK_STATE_CODEC`: `0` = empty (so a present air state reads back as empty),
/// unknown ids fall back to air (`Block.stateById`).
mod optional_block_state {
    use super::*;

    pub fn encode(s: &Option<BlockState>, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, s.map_or(0, |s| s.id() as i32));
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<BlockState>> {
        Ok(match read_varint(input)? {
            0 => None,
            id => Some(
                u32::try_from(id)
                    .ok()
                    .and_then(BlockState::from_id)
                    .or_else(|| BlockState::from_id(0))
                    .ok_or_else(|| DecodeError::msg("block state registry is empty"))?,
            ),
        })
    }
}

/// `EntityDataSerializers.OPTIONAL_UNSIGNED_INT`: `0` = empty, otherwise `value + 1`.
mod optional_unsigned_int {
    use super::*;

    pub fn encode(v: &Option<i32>, out: &mut Vec<u8>) -> EncodeResult<()> {
        OptionalVarInt(*v).encode(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<i32>> {
        OptionalVarInt::decode(input).map(|v| v.0)
    }
}

/// A value of one of the 44 `EntityDataSerializers` (the VarInt serializer id is the variant). Registry-holder
/// variants of data-driven registries (variants/sound variants) carry the raw registry id.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[mc(error = "Unknown serializer type {}")]
#[expect(
    clippy::large_enum_variant,
    reason = "the 44 serializer values are held inline, a few (ItemStack, Component) are much larger than the scalar ones"
)]
pub enum EntityDataValue {
    Byte(i8),
    Int(#[mc(var)] i32),
    Long(#[mc(var)] i64),
    Float(f32),
    String(#[mc(max = 32767)] String),
    Component(Component),
    OptionalComponent(Option<Component>),
    ItemStack(ItemStack),
    Boolean(bool),
    Rotations(Rotations),
    BlockPos(BlockPos),
    OptionalBlockPos(Option<BlockPos>),
    Direction(Direction),
    /// `Optional<EntityReference<LivingEntity>>`: the entity's UUID.
    OptionalLivingEntityReference(Option<Uuid>),
    BlockState(BlockState),
    OptionalBlockState(#[mc(with = optional_block_state)] Option<BlockState>),
    Particle(ParticleOptions),
    Particles(Vec<ParticleOptions>),
    VillagerData(VillagerData),
    OptionalUnsignedInt(#[mc(with = optional_unsigned_int)] Option<i32>),
    Pose(#[mc(with = pose)] Pose),
    CatVariant(#[mc(var)] i32),
    CatSoundVariant(#[mc(var)] i32),
    CowVariant(#[mc(var)] i32),
    CowSoundVariant(#[mc(var)] i32),
    WolfVariant(#[mc(var)] i32),
    WolfSoundVariant(#[mc(var)] i32),
    FrogVariant(#[mc(var)] i32),
    PigVariant(#[mc(var)] i32),
    PigSoundVariant(#[mc(var)] i32),
    ChickenVariant(#[mc(var)] i32),
    ChickenSoundVariant(#[mc(var)] i32),
    ZombieNautilusVariant(#[mc(var)] i32),
    OptionalGlobalPos(Option<GlobalPos>),
    PaintingVariant(Holder<PaintingVariant>),
    SnifferState(SnifferState),
    ArmadilloState(ArmadilloState),
    CopperGolemState(CopperGolemState),
    WeatheringCopperState(WeatheringCopperState),
    /// `Vector3fc`: x, y, z.
    Vector3([f32; 3]),
    /// `Quaternionfc`: x, y, z, w.
    Quaternion([f32; 4]),
    ResolvableProfile(ResolvableProfile),
    HumanoidArm(HumanoidArm),
    DyeColor(DyeColor),
}

/// `SynchedEntityData.DataValue`: accessor id byte, serializer id + value. Id `255` is the list terminator of
/// `ClientboundSetEntityDataPacket` and must not be used.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DataValue {
    pub id: u8,
    pub value: EntityDataValue,
}
