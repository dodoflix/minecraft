//! Play packets: group `debug` (debug subscriptions, debug samples, game rules, game tests).
use crate::codec::EncodeResult;
use mc_core::direction::{Axis, AxisDirection, Direction};
use mc_core::ident::Identifier;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::{BlockPos, ChunkPos, Vec3i};
use mc_text::Component;

use crate::codec::{DecodeError, DecodeResult, read_registry_id, read_varint, write_varint};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

// ---- debug subscription values (`util.debug.*`) ----------------------------------------------------------------

/// `DebugBeeInfo`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugBeeInfo {
    pub hive_pos: Option<BlockPos>,
    pub flower_pos: Option<BlockPos>,
    #[mc(var)]
    pub travel_ticks: i32,
    pub blacklisted_hives: Vec<BlockPos>,
}

/// `DebugBrainDump` (the `Set<BlockPos>` fields are `HashSet`s: lists in wire order, repeats collapse on decode).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugBrainDump {
    pub name: String,
    pub profession: String,
    pub xp: i32,
    pub health: f32,
    pub max_health: f32,
    pub inventory: String,
    pub wants_golem: bool,
    pub anger_level: i32,
    pub activities: Vec<String>,
    pub behaviors: Vec<String>,
    pub memories: Vec<String>,
    pub gossips: Vec<String>,
    #[mc(set)]
    pub pois: Vec<BlockPos>,
    #[mc(set)]
    pub potential_pois: Vec<BlockPos>,
}

/// `DebugBreezeInfo`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugBreezeInfo {
    #[mc(var)]
    pub attack_target: Option<i32>,
    pub jump_target: Option<BlockPos>,
}

id_enum!(
    /// `DebugEntityBlockIntersection` (VarInt id, unknown = `IN_BLOCK`)
    DebugEntityBlockIntersection { InBlock = 0, InFluid = 1, InAir = 2 } out_of_bounds = ZERO
);

/// `DebugGameEventInfo`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugGameEventInfo {
    pub event: mc_data::GameEvent,
    pub pos: Vec3,
}

/// `DebugGameEventListenerInfo`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct DebugGameEventListenerInfo {
    #[mc(var)]
    pub listener_radius: i32,
}

/// `DebugGoalInfo.DebugGoal`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugGoalInfoDebugGoal {
    #[mc(var)]
    pub priority: i32,
    pub is_running: bool,
    #[mc(max = 255)]
    pub name: String,
}

/// `DebugGoalInfo`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugGoalInfo {
    pub goals: Vec<DebugGoalInfoDebugGoal>,
}

/// `DebugHiveInfo`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct DebugHiveInfo {
    pub r#type: mc_data::Block,
    #[mc(var)]
    pub occupant_count: i32,
    #[mc(var)]
    pub honey_level: i32,
    pub sedated: bool,
}

id_enum!(
    /// `PathType` (VarInt id, unknown = `BLOCKED`)
    PathType {
        Blocked = 0, Open = 1, Walkable = 2, WalkableDoor = 3, Trapdoor = 4, PowderSnow = 5, OnTopOfPowderSnow = 6,
        Fence = 7, Lava = 8, Water = 9, WaterBorder = 10, Rail = 11, UnpassableRail = 12, FireInNeighbor = 13, Fire = 14,
        DamagingInNeighbor = 15, Damaging = 16, DoorOpen = 17, DoorWoodClosed = 18, DoorIronClosed = 19, Breach = 20,
        Leaves = 21, StickyHoney = 22, Cocoa = 23, DamageCautious = 24, OnTopOfTrapdoor = 25, BigMobsCloseToDanger = 26
    } out_of_bounds = ZERO
);

/// `Node.DEBUG_STREAM_CODEC` (also `Target.DEBUG_STREAM_CODEC`: a target is sent as a plain node).
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct DebugPathNode {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub walked_distance: f32,
    pub cost_malus: f32,
    pub closed: bool,
    pub r#type: PathType,
    pub f: f32,
}

/// `Path.DebugData`. `target_nodes` is a `Set<Target>` (`HashSet`; `Node.equals` compares the block position only, so
/// a repeated position collapses on decode).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugPathData {
    pub open_set: Vec<DebugPathNode>,
    pub closed_set: Vec<DebugPathNode>,
    #[mc(with = target_nodes)]
    pub target_nodes: Vec<DebugPathNode>,
}

mod target_nodes {
    use super::*;

    pub fn encode(nodes: &Vec<DebugPathNode>, out: &mut Vec<u8>) -> EncodeResult<()> {
        nodes.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<DebugPathNode>> {
        let mut unique: Vec<DebugPathNode> = Vec::new();
        for n in Vec::<DebugPathNode>::decode(input)? {
            if !unique.iter().any(|u| (u.x, u.y, u.z) == (n.x, n.y, n.z)) {
                unique.push(n);
            }
        }
        Ok(unique)
    }
}

/// `Path.DEBUG_STREAM_CODEC` writes `debugData`, throwing `IllegalStateException("Missing debug data")` when it has
/// no target nodes.
mod checked_debug_data {
    use super::*;

    pub fn encode(data: &DebugPathData, out: &mut Vec<u8>) -> EncodeResult<()> {
        if data.target_nodes.is_empty() {
            return Err(crate::codec::EncodeError::msg(
                "java.lang.IllegalStateException: Missing debug data",
            ));
        }
        data.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<DebugPathData> {
        DebugPathData::decode(input)
    }
}

/// `Path.DEBUG_STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugPath {
    pub reached: bool,
    pub next_node_index: i32,
    pub target: BlockPos,
    pub nodes: Vec<DebugPathNode>,
    #[mc(with = checked_debug_data)]
    pub debug_data: DebugPathData,
}

/// `DebugPathInfo`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DebugPathInfo {
    pub path: DebugPath,
    pub max_node_distance: f32,
}

/// `DebugPoiInfo`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct DebugPoiInfo {
    pub pos: BlockPos,
    pub poi_type: mc_data::PointOfInterestType,
    #[mc(var)]
    pub free_ticket_count: i32,
}

/// `BoundingBox.STREAM_CODEC`: min and max corner as packed block positions (inverted bounds are swapped).
mod bounding_box {
    use mc_core::bounding_box::BoundingBox;

    use super::*;

    pub fn encode(b: &BoundingBox, out: &mut Vec<u8>) -> EncodeResult<()> {
        BlockPos::new(b.min_x(), b.min_y(), b.min_z()).encode(out)?;
        BlockPos::new(b.max_x(), b.max_y(), b.max_z()).encode(out)?;
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<BoundingBox> {
        let (min, max) = (BlockPos::decode(input)?, BlockPos::decode(input)?);
        Ok(BoundingBox::new(
            min.x(),
            min.y(),
            min.z(),
            max.x(),
            max.y(),
            max.z(),
        ))
    }
}

/// `DebugStructureInfo.Piece`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct DebugStructureInfoPiece {
    #[mc(with = bounding_box)]
    pub bounding_box: mc_core::bounding_box::BoundingBox,
    pub is_start: bool,
}

/// `DebugStructureInfo`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DebugStructureInfo {
    #[mc(with = bounding_box)]
    pub bounding_box: mc_core::bounding_box::BoundingBox,
    pub pieces: Vec<DebugStructureInfoPiece>,
}

/// `Orientation.SideBias`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RedstoneOrientationSideBias {
    Left,
    Right,
}

/// `Orientation` (redstone wire update orientation). Wire form: `Orientation.getIndex()` (0..48) as a VarInt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RedstoneOrientation {
    pub up: Direction,
    pub front: Direction,
    pub side_bias: RedstoneOrientationSideBias,
}

impl RedstoneOrientation {
    const COUNT: i32 = 48;

    /// `Orientation.getIndex` (`generateIndex`); `up` and `front` must be on different axes.
    pub fn index(self) -> i32 {
        let key = if self.up.get_axis() == Axis::Y {
            self.front.get_axis() == Axis::X
        } else {
            self.front.get_axis() == Axis::Y
        };
        let front_key = (key as i32) << 1 | self.front.get_axis_direction() as i32;
        ((self.up.ordinal() as i32) << 2) + front_key << 1 | self.side_bias as i32
    }

    /// Inverse of [`index`](Self::index); `None` outside `0..48`.
    pub fn from_index(index: i32) -> Option<Self> {
        if !(0..Self::COUNT).contains(&index) {
            return None;
        }
        let side_bias = if index & 1 == 0 {
            RedstoneOrientationSideBias::Left
        } else {
            RedstoneOrientationSideBias::Right
        };
        let up = Direction::from_3d_data_value(index >> 3);
        let front_key = index >> 1 & 3;
        let front_axis = match (up.get_axis(), front_key >> 1) {
            (Axis::Y | Axis::X, 0) => Axis::Z,
            (Axis::Z, 0) => Axis::X,
            (Axis::Y, _) => Axis::X,
            _ => Axis::Y,
        };
        let axis_direction = if front_key & 1 == 0 {
            AxisDirection::Positive
        } else {
            AxisDirection::Negative
        };
        Some(Self {
            up,
            front: Direction::from_axis_and_direction(front_axis, axis_direction),
            side_bias,
        })
    }
}

impl Encode for RedstoneOrientation {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.index());
        Ok(())
    }
}
impl Decode for RedstoneOrientation {
    /// `Orientation.fromIndex`: an `ArrayIndexOutOfBoundsException` outside `0..48`.
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let index = read_varint(input)?;
        Self::from_index(index).ok_or_else(|| {
            DecodeError::Message(format!(
                "Index {index} out of bounds for length {}",
                Self::COUNT
            ))
        })
    }
}

/// `DebugSubscriptions` registration order = registry ids; id 0 (`dedicated_server_tick_time`) carries no value.
macro_rules! debug_subscriptions {
    ($($id:literal $variant:ident($ty:ty)),+ $(,)?) => {
        /// `DebugSubscription.Event<?>`: a subscription and its value; one variant per subscription that has a value codec.
        #[derive(Debug, Clone, PartialEq)]
        pub enum DebugSubscriptionEvent {
            $($variant($ty)),+
        }

        impl DebugSubscriptionEvent {
            /// The registry entry this value belongs to.
            pub fn subscription(&self) -> mc_data::DebugSubscription {
                let id = match self { $(Self::$variant(_) => $id),+ };
                mc_data::DebugSubscription::from_id(id).expect("debug subscription ids are fixed by the registry")
            }

            fn encode_value(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
                match self { $(Self::$variant(v) => v.encode(out)?),+ };
                Ok(())
            }

            fn decode_value(subscription: mc_data::DebugSubscription, input: &mut &[u8]) -> DecodeResult<Self> {
                match subscription.id() {
                    $($id => <$ty>::decode(input).map(Self::$variant),)+
                    _ => Err(no_value_codec()),
                }
            }
        }
    };
}

debug_subscriptions!(
    1 Bees(DebugBeeInfo),
    2 Brains(DebugBrainDump),
    3 Breezes(DebugBreezeInfo),
    4 GoalSelectors(DebugGoalInfo),
    5 EntityPaths(DebugPathInfo),
    6 EntityBlockIntersections(DebugEntityBlockIntersection),
    7 BeeHives(DebugHiveInfo),
    8 Pois(DebugPoiInfo),
    9 RedstoneWireOrientations(RedstoneOrientation),
    10 VillageSections(()),
    11 Raids(Vec<BlockPos>),
    12 Structures(Vec<DebugStructureInfo>),
    13 GameEventListeners(DebugGameEventListenerInfo),
    14 NeighborUpdates(BlockPos),
    15 GameEvents(DebugGameEventInfo),
);

/// `DebugSubscriptions.DEDICATED_SERVER_TICK_TIME` has a `null` value codec: vanilla fails on `requireNonNull` when it
/// (de)serializes an event/update for it.
fn no_value_codec() -> DecodeError {
    DecodeError::msg("minecraft:dedicated_server_tick_time has no value stream codec")
}

fn read_subscription(input: &mut &[u8]) -> DecodeResult<mc_data::DebugSubscription> {
    let subscription: mc_data::DebugSubscription = read_registry_id(input)?;
    if subscription.id() == 0 {
        Err(no_value_codec())
    } else {
        Ok(subscription)
    }
}

impl Encode for DebugSubscriptionEvent {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.subscription().encode(out)?;
        self.encode_value(out)?;
        Ok(())
    }
}
impl Decode for DebugSubscriptionEvent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let subscription = read_subscription(input)?;
        Self::decode_value(subscription, input)
    }
}

/// `DebugSubscription.Update<?>`: the subscription id, then an optional value (`Optional.empty` clears it client side).
#[derive(Debug, Clone, PartialEq)]
pub enum DebugSubscriptionUpdate {
    /// `emptyUpdate()`. The subscription must have a value codec (not `dedicated_server_tick_time`).
    Empty(mc_data::DebugSubscription),
    /// `packUpdate(value)`
    Value(DebugSubscriptionEvent),
}

impl Encode for DebugSubscriptionUpdate {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            Self::Empty(subscription) => {
                subscription.encode(out)?;
                false.encode(out)?;
            }
            Self::Value(event) => {
                event.subscription().encode(out)?;
                true.encode(out)?;
                event.encode_value(out)?;
            }
        };
        Ok(())
    }
}
impl Decode for DebugSubscriptionUpdate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let subscription = read_subscription(input)?;
        if bool::decode(input)? {
            DebugSubscriptionEvent::decode_value(subscription, input).map(Self::Value)
        } else {
            Ok(Self::Empty(subscription))
        }
    }
}

// ---- clientbound ---------------------------------------------------------------------------------------------

/// `ClientboundDebugBlockValuePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "debug/block_value")]
pub struct ClientboundDebugBlockValuePacket {
    pub block_pos: BlockPos,
    pub update: DebugSubscriptionUpdate,
}

/// `ClientboundDebugChunkValuePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "debug/chunk_value")]
pub struct ClientboundDebugChunkValuePacket {
    pub chunk_pos: ChunkPos,
    pub update: DebugSubscriptionUpdate,
}

/// `ClientboundDebugEntityValuePacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "debug/entity_value")]
pub struct ClientboundDebugEntityValuePacket {
    #[mc(var)]
    pub entity_id: i32,
    pub update: DebugSubscriptionUpdate,
}

/// `ClientboundDebugEventPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "debug/event")]
pub struct ClientboundDebugEventPacket {
    pub event: DebugSubscriptionEvent,
}

/// `RemoteDebugSampleType`: written with `writeEnum` (VarInt ordinal, no out-of-bounds strategy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 1")]
pub enum RemoteDebugSampleType {
    TickTime,
}

/// `FriendlyByteBuf.readLongArray` (count bounded by the readable bytes) for a `long[]` field.
mod long_array {
    use crate::codec::LongArray;

    use super::*;

    pub fn encode(v: &[i64], out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, v.len() as i32);
        for l in v {
            l.encode(out)?;
        }
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<i64>> {
        LongArray::decode(input).map(|a| a.0)
    }
}

/// `ClientboundDebugSamplePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "debug_sample")]
pub struct ClientboundDebugSamplePacket {
    #[mc(with = long_array)]
    pub sample: Vec<i64>,
    pub debug_sample_type: RemoteDebugSampleType,
}

/// `ClientboundGameRuleValuesPacket`: `GameRule` key -> value text, in wire order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "game_rule_values")]
pub struct ClientboundGameRuleValuesPacket {
    #[mc(map)]
    pub values: Vec<(Identifier, String)>,
}

/// `ClientboundGameTestHighlightPosPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "game_test_highlight_pos")]
pub struct ClientboundGameTestHighlightPosPacket {
    pub absolute_pos: BlockPos,
    pub relative_pos: BlockPos,
}

/// `ClientboundTestInstanceBlockStatus`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "test_instance_block_status")]
pub struct ClientboundTestInstanceBlockStatus {
    pub status: Component,
    pub size: Option<Vec3i>,
}

// ---- serverbound ---------------------------------------------------------------------------------------------

/// `ServerboundDebugSubscriptionRequestPacket` (a `Set`; at most 32 entries).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "debug_subscription_request")]
pub struct ServerboundDebugSubscriptionRequestPacket {
    #[mc(max = 32, set)]
    pub subscriptions: Vec<mc_data::DebugSubscription>,
}

/// `ServerboundSetGameRulePacket.Entry`: `gameRuleKey` is the `GameRule` registry key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ServerboundSetGameRulePacketEntry {
    pub game_rule_key: Identifier,
    pub value: String,
}

/// `ServerboundSetGameRulePacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_game_rule")]
pub struct ServerboundSetGameRulePacket {
    pub entries: Vec<ServerboundSetGameRulePacketEntry>,
}

id_enum!(
    /// `TestBlockMode` (VarInt id, unknown = `START`)
    TestBlockMode { Start = 0, Log = 1, Fail = 2, Accept = 3 } out_of_bounds = ZERO
);

/// `ServerboundSetTestBlockPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_test_block")]
pub struct ServerboundSetTestBlockPacket {
    pub position: BlockPos,
    pub mode: TestBlockMode,
    pub message: String,
}

id_enum!(
    /// `ServerboundTestInstanceBlockActionPacket.Action` (VarInt id, unknown = `INIT`)
    ServerboundTestInstanceBlockActionPacketAction { Init = 0, Query = 1, Set = 2, Reset = 3, Save = 4, Export = 5, Run = 6 }
    out_of_bounds = ZERO
);

id_enum!(
    /// `TestInstanceBlockEntity.Status` (VarInt id, unknown = `CLEARED`)
    TestInstanceBlockEntityStatus { Cleared = 0, Running = 1, Finished = 2 } out_of_bounds = ZERO
);

/// `Rotation.STREAM_CODEC`: VarInt index, WRAP.
mod rotation {
    use mc_core::rotation::Rotation;

    use super::*;

    pub fn encode(r: &Rotation, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, r.get_index());
        Ok(())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Rotation> {
        read_varint(input).map(Rotation::by_id)
    }
}

/// `TestInstanceBlockEntity.Data`: `test` is the `GameTestInstance` resource key.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TestInstanceBlockEntityData {
    pub test: Option<Identifier>,
    pub size: Vec3i,
    #[mc(with = rotation)]
    pub rotation: mc_core::rotation::Rotation,
    pub ignore_entities: bool,
    pub status: TestInstanceBlockEntityStatus,
    pub error_message: Option<Component>,
}

/// `ServerboundTestInstanceBlockActionPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "test_instance_block_action")]
pub struct ServerboundTestInstanceBlockActionPacket {
    pub pos: BlockPos,
    pub action: ServerboundTestInstanceBlockActionPacketAction,
    pub data: TestInstanceBlockEntityData,
}
