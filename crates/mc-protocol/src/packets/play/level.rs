//! Play packets: group `level` (blocks, chunks, light, border, time, login/respawn, sounds, tick state).
//!
//! Chunk/biome section buffers stay raw bytes: `mc-world` serialises the sections (`LevelChunkSection.write`, one
//! after another in section order) into `ClientboundLevelChunkPacketData::buffer` and the biome containers into
//! `ChunkBiomeData::buffer`.
use crate::codec::EncodeResult;
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;
use mc_data::block::BlockState;
use mc_data::{Block, BlockEntityType};
use mc_nbt::{CompoundTag, Tag};

use super::types::{Difficulty, GameType, SoundEventHolder};
use crate::codec::{
    BitSet, BitSetBytes, DecodeBounded, DecodeError, DecodeResult, GlobalPos, LongArray, NbtLimit,
    OptionalVarInt, decode_tag_with, read_count, read_varint, read_varlong, write_varint,
    write_varlong,
};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

id_enum!(
    /// `Heightmap.Types` (`Heightmap.Types.STREAM_CODEC`: VarInt id, unknown = `WorldSurfaceWg`)
    HeightmapTypes {
        WorldSurfaceWg = 0,
        WorldSurface = 1,
        OceanFloorWg = 2,
        OceanFloor = 3,
        MotionBlocking = 4,
        MotionBlockingNoLeaves = 5
    } out_of_bounds = ZERO
);

/// `SoundSource` (`FriendlyByteBuf.readEnum`: VarInt ordinal; an unknown ordinal is an `ArrayIndexOutOfBoundsException`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 11")]
pub enum SoundSource {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

// ---- wire adapters for `#[mc(with = ..)]` fields ------------------------------------------------

/// `ByteBufCodecs.TRUSTED_COMPOUND_TAG` (server -> client: unlimited quota, root must be a compound).
mod trusted_compound {
    use super::*;

    pub fn encode(tag: &CompoundTag, out: &mut Vec<u8>) -> EncodeResult<()> {
        tag.encode(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<CompoundTag> {
        match decode_tag_with(input, NbtLimit::UNLIMITED)? {
            None => Err(DecodeError::msg("Expected non-null compound tag")),
            Some(Tag::Compound(c)) => Ok(c),
            Some(other) => Err(DecodeError::Message(format!("Not a compound tag: {other}"))),
        }
    }
}

/// `ByteBufCodecs.OPTIONAL_COMPOUND_TAG` (`FriendlyByteBuf.readNbt`: default quota, a lone `End` = absent).
mod optional_compound {
    use super::*;

    pub fn encode(tag: &Option<CompoundTag>, out: &mut Vec<u8>) -> EncodeResult<()> {
        match tag {
            Some(c) => c.encode(out)?,
            None => out.push(0),
        };
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<CompoundTag>> {
        match decode_tag_with(input, NbtLimit::DEFAULT)? {
            None => Ok(None),
            Some(Tag::Compound(c)) => Ok(Some(c)),
            Some(other) => Err(DecodeError::Message(format!("Not a compound tag: {other}"))),
        }
    }
}

/// `ByteBufCodecs.BIT_SET`: the bit set as a `byte[]`.
mod bit_set_bytes {
    use super::*;

    pub fn encode(bits: &BitSet, out: &mut Vec<u8>) -> EncodeResult<()> {
        bits.to_bytes().encode(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<BitSet> {
        BitSetBytes::decode(input).map(|b| b.0)
    }
}

/// `ByteBufCodecs.byteArray(2048).apply(ByteBufCodecs.list())`: sections of light data.
mod light_layers {
    use super::*;

    pub fn encode(layers: &Vec<Vec<u8>>, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, layers.len() as i32);
        for l in layers {
            crate::codec::EncodeBounded::encode_bounded(l, out, 2048)?;
        }
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<Vec<u8>>> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut v = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            v.push(Vec::<u8>::decode_bounded(input, 2048)?);
        }
        Ok(v)
    }
}

/// `ByteBufCodecs.map(.., Heightmap.Types.STREAM_CODEC, ByteBufCodecs.LONG_ARRAY)`, in wire order.
mod heightmaps {
    use super::*;

    pub fn encode(maps: &Vec<(HeightmapTypes, Vec<i64>)>, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, maps.len() as i32);
        for (t, longs) in maps {
            t.encode(out)?;
            write_varint(out, longs.len() as i32);
            for l in longs {
                l.encode(out)?;
            }
        }
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<(HeightmapTypes, Vec<i64>)>> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut v = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            v.push((HeightmapTypes::decode(input)?, LongArray::decode(input)?.0));
        }
        Ok(crate::codec::collapse_map(v))
    }
}

/// `ByteBufCodecs.map(.., WorldClock.STREAM_CODEC, ClockNetworkState.STREAM_CODEC)`: world clock registry id
/// (`holderRegistry` = plain VarInt) -> state, in wire order.
mod clock_updates {
    use super::*;

    pub fn encode(updates: &Vec<(i32, ClockNetworkState)>, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, updates.len() as i32);
        for (id, state) in updates {
            write_varint(out, *id);
            state.encode(out)?;
        }
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<(i32, ClockNetworkState)>> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut v = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            v.push((read_varint(input)?, ClockNetworkState::decode(input)?));
        }
        Ok(crate::codec::collapse_map(v))
    }
}

/// `GameType.OPTIONAL_STREAM_CODEC`: `OPTIONAL_VAR_INT` of the game type id.
mod optional_game_type {
    use super::*;

    pub fn encode(t: &Option<GameType>, out: &mut Vec<u8>) -> EncodeResult<()> {
        OptionalVarInt(t.map(GameType::id)).encode(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<GameType>> {
        Ok(OptionalVarInt::decode(input)?.0.map(GameType::by_id))
    }
}

// ---- blocks --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "block_changed_ack")]
pub struct ClientboundBlockChangedAckPacket {
    #[mc(var)]
    pub sequence: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "block_destruction")]
pub struct ClientboundBlockDestructionPacket {
    #[mc(var)]
    pub id: i32,
    pub pos: BlockPos,
    /// Unsigned byte on the wire.
    pub progress: u8,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "block_entity_data")]
pub struct ClientboundBlockEntityDataPacket {
    pub pos: BlockPos,
    pub r#type: BlockEntityType,
    #[mc(with = trusted_compound)]
    pub tag: CompoundTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "block_event")]
pub struct ClientboundBlockEventPacket {
    pub pos: BlockPos,
    /// Unsigned byte on the wire.
    pub b0: u8,
    /// Unsigned byte on the wire.
    pub b1: u8,
    pub block: Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "block_update")]
pub struct ClientboundBlockUpdatePacket {
    pub pos: BlockPos,
    pub block_state: BlockState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "add_transient_block")]
pub struct ClientboundAddTransientBlockPacket {
    pub pos: BlockPos,
    pub block_state: BlockState,
}

/// `ClientboundSectionBlocksUpdatePacket`: `changes` are `(position in section, state)` where the position is
/// `SectionPos.sectionRelativePos` (`x << 8 | z << 4 | y`, 12 bits). On the wire each change is the VarLong
/// `state id << 12 | position`.
///
/// Deviation: vanilla decodes an unknown state id to `null` (and fails later in the client); here it is a decode error.
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = clientbound, name = "section_blocks_update")]
pub struct ClientboundSectionBlocksUpdatePacket {
    pub section_pos: SectionPos,
    pub changes: Vec<(u16, BlockState)>,
}

impl Encode for ClientboundSectionBlocksUpdatePacket {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.section_pos.encode(out)?;
        write_varint(out, self.changes.len() as i32);
        for (pos, state) in &self.changes {
            write_varlong(out, i64::from(state.id()) << 12 | i64::from(*pos));
        }
        Ok(())
    }
}
impl Decode for ClientboundSectionBlocksUpdatePacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let section_pos = SectionPos::decode(input)?;
        let count = read_count(input, i32::MAX as usize)?;
        let mut changes = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            let packed = read_varlong(input)?;
            let id = (packed as u64 >> 12) as i32;
            let state = u32::try_from(id)
                .ok()
                .and_then(BlockState::from_id)
                .ok_or_else(|| DecodeError::Message(format!("No value with id {id}")))?;
            changes.push(((packed & 4095) as u16, state));
        }
        Ok(Self {
            section_pos,
            changes,
        })
    }
}

// ---- chunks and light ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "chunk_batch_finished")]
pub struct ClientboundChunkBatchFinishedPacket {
    #[mc(var)]
    pub batch_size: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "chunk_batch_start")]
pub struct ClientboundChunkBatchStartPacket;

/// `ClientboundChunksBiomesPacket.ChunkBiomeData`: `buffer` = the chunk's biome containers (`PalettedContainer.write`)
/// for every section in order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChunkBiomeData {
    pub pos: ChunkPos,
    #[mc(max = 2097152)]
    pub buffer: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "chunks_biomes")]
pub struct ClientboundChunksBiomesPacket {
    pub chunk_biome_data: Vec<ChunkBiomeData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "forget_level_chunk")]
pub struct ClientboundForgetLevelChunkPacket {
    pub pos: ChunkPos,
}

/// `ClientboundLevelChunkPacketData.BlockEntityInfo`: `packed_xz = (x & 15) << 4 | (z & 15)`, `y` = block y.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BlockEntityInfo {
    pub packed_xz: i8,
    pub y: i16,
    pub r#type: BlockEntityType,
    /// Absent when the update tag is empty.
    #[mc(with = optional_compound)]
    pub tag: Option<CompoundTag>,
}

/// `ClientboundLevelChunkPacketData`: heightmaps the client needs, the serialised sections (`buffer`) and the block entities.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ClientboundLevelChunkPacketData {
    #[mc(with = heightmaps)]
    pub heightmaps: Vec<(HeightmapTypes, Vec<i64>)>,
    #[mc(max = 2097152)]
    pub buffer: Vec<u8>,
    pub block_entities_data: Vec<BlockEntityInfo>,
}

/// `ClientboundLightUpdatePacketData`: masks are section indices (bit 0 = the section below the world); each mask bit of
/// `sky_y_mask`/`block_y_mask` has one 2048-byte layer in `sky_updates`/`block_updates`, in bit order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClientboundLightUpdatePacketData {
    #[mc(with = bit_set_bytes)]
    pub sky_y_mask: BitSet,
    #[mc(with = bit_set_bytes)]
    pub block_y_mask: BitSet,
    #[mc(with = bit_set_bytes)]
    pub empty_sky_y_mask: BitSet,
    #[mc(with = bit_set_bytes)]
    pub empty_block_y_mask: BitSet,
    #[mc(with = light_layers)]
    pub sky_updates: Vec<Vec<u8>>,
    #[mc(with = light_layers)]
    pub block_updates: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "level_chunk_with_light")]
pub struct ClientboundLevelChunkWithLightPacket {
    pub x: i32,
    pub z: i32,
    pub chunk_data: ClientboundLevelChunkPacketData,
    pub light_data: ClientboundLightUpdatePacketData,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "light_update")]
pub struct ClientboundLightUpdatePacket {
    #[mc(var)]
    pub x: i32,
    #[mc(var)]
    pub z: i32,
    pub light_data: ClientboundLightUpdatePacketData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_chunk_cache_center")]
pub struct ClientboundSetChunkCacheCenterPacket {
    #[mc(var)]
    pub x: i32,
    #[mc(var)]
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_chunk_cache_radius")]
pub struct ClientboundSetChunkCacheRadiusPacket {
    #[mc(var)]
    pub radius: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_simulation_distance")]
pub struct ClientboundSetSimulationDistancePacket {
    #[mc(var)]
    pub simulation_distance: i32,
}

// ---- difficulty, events, world border ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "change_difficulty")]
pub struct ClientboundChangeDifficultyPacket {
    pub difficulty: Difficulty,
    pub locked: bool,
}

/// `ClientboundGameEventPacket`: `event` is the id of one of the `ClientboundGameEventPacket.Type` constants below
/// (an unknown id is not rejected by vanilla's decoder either).
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "game_event")]
pub struct ClientboundGameEventPacket {
    pub event: u8,
    pub param: f32,
}

impl ClientboundGameEventPacket {
    pub const NO_RESPAWN_BLOCK_AVAILABLE: u8 = 0;
    pub const START_RAINING: u8 = 1;
    pub const STOP_RAINING: u8 = 2;
    pub const CHANGE_GAME_MODE: u8 = 3;
    pub const WIN_GAME: u8 = 4;
    pub const DEMO_EVENT: u8 = 5;
    pub const PLAY_ARROW_HIT_SOUND: u8 = 6;
    pub const RAIN_LEVEL_CHANGE: u8 = 7;
    pub const THUNDER_LEVEL_CHANGE: u8 = 8;
    pub const PUFFER_FISH_STING: u8 = 9;
    pub const GUARDIAN_ELDER_EFFECT: u8 = 10;
    pub const IMMEDIATE_RESPAWN: u8 = 11;
    pub const LIMITED_CRAFTING: u8 = 12;
    pub const LEVEL_CHUNKS_LOAD_START: u8 = 13;
    pub const DEMO_PARAM_INTRO: f32 = 0.0;
    pub const DEMO_PARAM_HINT_1: f32 = 101.0;
    pub const DEMO_PARAM_HINT_2: f32 = 102.0;
    pub const DEMO_PARAM_HINT_3: f32 = 103.0;
    pub const DEMO_PARAM_HINT_4: f32 = 104.0;
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "initialize_border")]
pub struct ClientboundInitializeBorderPacket {
    pub new_center_x: f64,
    pub new_center_z: f64,
    pub old_size: f64,
    pub new_size: f64,
    #[mc(var)]
    pub lerp_time: i64,
    #[mc(var)]
    pub new_absolute_max_size: i32,
    #[mc(var)]
    pub warning_blocks: i32,
    #[mc(var)]
    pub warning_time: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_border_center")]
pub struct ClientboundSetBorderCenterPacket {
    pub new_center_x: f64,
    pub new_center_z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_border_lerp_size")]
pub struct ClientboundSetBorderLerpSizePacket {
    pub old_size: f64,
    pub new_size: f64,
    #[mc(var)]
    pub lerp_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_border_size")]
pub struct ClientboundSetBorderSizePacket {
    pub size: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_border_warning_delay")]
pub struct ClientboundSetBorderWarningDelayPacket {
    #[mc(var)]
    pub warning_delay: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_border_warning_distance")]
pub struct ClientboundSetBorderWarningDistancePacket {
    #[mc(var)]
    pub warning_blocks: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "level_event")]
pub struct ClientboundLevelEventPacket {
    /// A `LevelEvent` constant.
    pub r#type: i32,
    pub pos: BlockPos,
    pub data: i32,
    pub global_event: bool,
}

// ---- login / respawn --------------------------------------------------------------------------------

/// `CommonPlayerSpawnInfo`. `dimension_type` is the (data-driven) dimension type registry id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CommonPlayerSpawnInfo {
    #[mc(var)]
    pub dimension_type: i32,
    pub dimension: Identifier,
    pub seed: i64,
    pub game_type: GameType,
    #[mc(with = optional_game_type)]
    pub previous_game_type: Option<GameType>,
    pub is_debug: bool,
    pub is_flat: bool,
    pub last_death_location: Option<GlobalPos>,
    #[mc(var)]
    pub portal_cooldown: i32,
    #[mc(var)]
    pub sea_level: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "login")]
pub struct ClientboundLoginPacket {
    pub player_id: i32,
    pub hardcore: bool,
    /// `Set<ResourceKey<Level>>` (`HashSet`): repeats collapse on decode.
    #[mc(set)]
    pub levels: Vec<Identifier>,
    #[mc(var)]
    pub max_players: i32,
    #[mc(var)]
    pub chunk_radius: i32,
    #[mc(var)]
    pub simulation_distance: i32,
    pub reduced_debug_info: bool,
    pub show_death_screen: bool,
    pub do_limited_crafting: bool,
    pub common_player_spawn_info: CommonPlayerSpawnInfo,
    pub online_mode: bool,
    pub enforces_secure_chat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "respawn")]
pub struct ClientboundRespawnPacket {
    pub common_player_spawn_info: CommonPlayerSpawnInfo,
    /// Bit mask of the `KEEP_*` constants.
    pub data_to_keep: i8,
}

impl ClientboundRespawnPacket {
    pub const KEEP_ATTRIBUTE_MODIFIERS: i8 = 1;
    pub const KEEP_ENTITY_DATA: i8 = 2;
    pub const KEEP_ALL_DATA: i8 = 3;

    pub fn should_keep(&self, mask: i8) -> bool {
        self.data_to_keep & mask != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "low_disk_space_warning")]
pub struct ClientboundLowDiskSpaceWarningPacket;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "start_configuration")]
pub struct ClientboundStartConfigurationPacket;

/// `LevelData.RespawnData`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct RespawnData {
    pub global_pos: GlobalPos,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_default_spawn_position")]
pub struct ClientboundSetDefaultSpawnPositionPacket {
    pub respawn_data: RespawnData,
}

// ---- time and ticking -------------------------------------------------------------------------------

/// `ClockNetworkState`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct ClockNetworkState {
    #[mc(var)]
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
}

/// `clock_updates`: `(world clock registry id, state)` in wire order.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "set_time")]
pub struct ClientboundSetTimePacket {
    pub game_time: i64,
    #[mc(with = clock_updates)]
    pub clock_updates: Vec<(i32, ClockNetworkState)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "ticking_state")]
pub struct ClientboundTickingStatePacket {
    pub tick_rate: f32,
    pub is_frozen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "ticking_step")]
pub struct ClientboundTickingStepPacket {
    #[mc(var)]
    pub tick_steps: i32,
}

// ---- sounds ------------------------------------------------------------------------------------------

/// `ClientboundSoundPacket`: `x`/`y`/`z` are fixed point (`position * 8`).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "sound")]
pub struct ClientboundSoundPacket {
    pub sound: SoundEventHolder,
    pub source: SoundSource,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

impl ClientboundSoundPacket {
    pub const LOCATION_ACCURACY: f32 = 8.0;

    /// The vanilla constructor: world position to fixed point (`(int)(v * 8.0)`).
    pub fn new(
        sound: SoundEventHolder,
        source: SoundSource,
        x: f64,
        y: f64,
        z: f64,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self {
            sound,
            source,
            x: (x * 8.0) as i32,
            y: (y * 8.0) as i32,
            z: (z * 8.0) as i32,
            volume,
            pitch,
            seed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "sound_entity")]
pub struct ClientboundSoundEntityPacket {
    pub sound: SoundEventHolder,
    pub source: SoundSource,
    #[mc(var)]
    pub id: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

/// `ClientboundStopSoundPacket`: flags byte (`1` = source, `2` = name), then the present fields (source first).
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = clientbound, name = "stop_sound")]
pub struct ClientboundStopSoundPacket {
    pub name: Option<Identifier>,
    pub source: Option<SoundSource>,
}

impl Encode for ClientboundStopSoundPacket {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.push(u8::from(self.source.is_some()) | u8::from(self.name.is_some()) << 1);
        if let Some(s) = &self.source {
            s.encode(out)?;
        }
        if let Some(n) = &self.name {
            n.encode(out)?;
        };
        Ok(())
    }
}
impl Decode for ClientboundStopSoundPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let flags = i8::decode(input)?;
        let source = if flags & 1 > 0 {
            Some(SoundSource::decode(input)?)
        } else {
            None
        };
        let name = if flags & 2 > 0 {
            Some(Identifier::decode(input)?)
        } else {
            None
        };
        Ok(Self { name, source })
    }
}
