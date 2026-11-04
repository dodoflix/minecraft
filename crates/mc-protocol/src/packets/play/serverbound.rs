//! Play packets: group `serverbound` (movement, actions, block-entity editors, interaction).
//! Enums read with `FriendlyByteBuf.readEnum` are VarInt ordinals that fail like Java's array access
//! (`Index N out of bounds for length L`); `ByIdMap` enums wrap/zero (see `id_enum!`).
use mc_core::direction::Direction;
use mc_core::mirror::Mirror;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::vec3i::Vec3i;
use mc_core::rotation::Rotation;
use uuid::Uuid;

use super::types::{Difficulty, GameType, InteractionHand};
use crate::codec::{
    DecodeError, DecodeResult, LpVec3, OptionalVarInt, read_utf, read_varint, write_utf,
    write_varint,
};
use crate::packets::common::id_enum;
use crate::{Decode, Encode, Packet};

// ---- `with` adapters ---------------------------------------------------------------------------

/// `Vec3.LP_STREAM_CODEC`.
mod lp_vec3 {
    use super::*;
    pub fn encode(v: &Vec3, out: &mut Vec<u8>) {
        LpVec3(*v).encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec3> {
        LpVec3::decode(input).map(|l| l.0)
    }
}

/// `ByteBufCodecs.OPTIONAL_VAR_INT` as an `Option<i32>`.
mod optional_var_int {
    use super::*;
    pub fn encode(v: &Option<i32>, out: &mut Vec<u8>) {
        OptionalVarInt(*v).encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<i32>> {
        OptionalVarInt::decode(input).map(|v| v.0)
    }
}

/// `writeByte(direction.get3DDataValue())` / `Direction.from3DDataValue(readUnsignedByte())`.
mod direction_u8 {
    use super::*;
    pub fn encode(d: &Direction, out: &mut Vec<u8>) {
        out.push(d.get_3d_data_value() as u8)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Direction> {
        u8::decode(input).map(|v| Direction::from_3d_data_value(i32::from(v)))
    }
}

/// `readEnum(Mirror.class)`.
mod mirror_enum {
    use super::*;
    pub fn encode(m: &Mirror, out: &mut Vec<u8>) {
        write_varint(out, *m as i32)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Mirror> {
        let id = read_varint(input)?;
        usize::try_from(id)
            .ok()
            .and_then(|i| Mirror::ALL.get(i).copied())
            .ok_or_else(|| DecodeError::Message(format!("Index {id} out of bounds for length 3")))
    }
}

/// `Rotation.STREAM_CODEC` (`ByIdMap`, WRAP).
mod rotation_id {
    use super::*;
    pub fn encode(r: &Rotation, out: &mut Vec<u8>) {
        write_varint(out, r.get_index())
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Rotation> {
        read_varint(input).map(Rotation::by_id)
    }
}

/// `stringUtf8(384).apply(fixedSizeList(4))`: four strings, no length prefix.
mod sign_lines {
    use super::*;
    pub fn encode(lines: &[String; 4], out: &mut Vec<u8>) {
        for l in lines {
            write_utf(out, l)
        }
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<[String; 4]> {
        Ok([
            read_utf(input, 384)?,
            read_utf(input, 384)?,
            read_utf(input, 384)?,
            read_utf(input, 384)?,
        ])
    }
}

// ---- simple packets ----------------------------------------------------------------------------

/// `ServerboundAcceptTeleportationPacket`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "accept_teleportation")]
pub struct ServerboundAcceptTeleportationPacket {
    #[mc(var)]
    pub id: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// `ServerboundAttackPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "attack")]
pub struct ServerboundAttackPacket {
    #[mc(var)]
    pub entity_id: i32,
}

/// `ServerboundChangeDifficultyPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "change_difficulty")]
pub struct ServerboundChangeDifficultyPacket {
    pub difficulty: Difficulty,
}

/// `ServerboundChangeGameModePacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "change_game_mode")]
pub struct ServerboundChangeGameModePacket {
    pub mode: GameType,
}

/// `ServerboundChunkBatchReceivedPacket`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "chunk_batch_received")]
pub struct ServerboundChunkBatchReceivedPacket {
    pub desired_chunks_per_tick: f32,
}

/// `ServerboundClientCommandPacket.Action`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 3")]
pub enum ServerboundClientCommandAction {
    PerformRespawn,
    RequestStats,
    RequestGameruleValues,
}

/// `ServerboundClientCommandPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "client_command")]
pub struct ServerboundClientCommandPacket {
    pub action: ServerboundClientCommandAction,
}

/// `ServerboundClientTickEndPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "client_tick_end")]
pub struct ServerboundClientTickEndPacket;

/// `ServerboundConfigurationAcknowledgedPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "configuration_acknowledged")]
pub struct ServerboundConfigurationAcknowledgedPacket;

/// `ServerboundPlayerLoadedPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "player_loaded")]
pub struct ServerboundPlayerLoadedPacket;

/// `ServerboundPunchPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "punch")]
pub struct ServerboundPunchPacket;

/// `ServerboundInteractPacket`: the hit location is a `LpVec3`.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "interact")]
pub struct ServerboundInteractPacket {
    #[mc(var)]
    pub entity_id: i32,
    pub hand: InteractionHand,
    #[mc(with = lp_vec3)]
    pub location: Vec3,
    pub using_secondary_action: bool,
}

/// `ServerboundJigsawGeneratePacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "jigsaw_generate")]
pub struct ServerboundJigsawGeneratePacket {
    pub pos: BlockPos,
    #[mc(var)]
    pub levels: i32,
    pub keep_jigsaws: bool,
}

/// `ServerboundLockDifficultyPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "lock_difficulty")]
pub struct ServerboundLockDifficultyPacket {
    pub locked: bool,
}

/// `ServerboundPaddleBoatPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "paddle_boat")]
pub struct ServerboundPaddleBoatPacket {
    pub left: bool,
    pub right: bool,
}

/// `ServerboundPickItemFromBlockPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "pick_item_from_block")]
pub struct ServerboundPickItemFromBlockPacket {
    pub pos: BlockPos,
    pub include_data: bool,
}

/// `ServerboundPickItemFromEntityPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "pick_item_from_entity")]
pub struct ServerboundPickItemFromEntityPacket {
    #[mc(var)]
    pub id: i32,
    pub include_data: bool,
}

/// `ServerboundPlayerAbilitiesPacket`: a flags byte of which only bit 2 (flying) is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Packet)]
#[packet(state = play, flow = serverbound, name = "player_abilities")]
pub struct ServerboundPlayerAbilitiesPacket {
    pub is_flying: bool,
}

impl Encode for ServerboundPlayerAbilitiesPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(if self.is_flying { 2 } else { 0 })
    }
}
impl Decode for ServerboundPlayerAbilitiesPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self {
            is_flying: u8::decode(input)? & 2 != 0,
        })
    }
}

/// `ServerboundPlayerActionPacket.Action`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 9")]
pub enum ServerboundPlayerActionAction {
    StartDestroyBlock,
    ChangeDestroyDirection,
    AbortDestroyBlock,
    StopDestroyBlock,
    DropAllItems,
    DropItem,
    ReleaseUseItem,
    SwapItemWithOffhand,
    Stab,
}

/// `ServerboundPlayerActionPacket`: the direction is an unsigned byte folded with `from3DDataValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "player_action")]
pub struct ServerboundPlayerActionPacket {
    pub action: ServerboundPlayerActionAction,
    pub pos: BlockPos,
    #[mc(with = direction_u8)]
    pub direction: Direction,
    #[mc(var)]
    pub sequence: i32,
}

/// `ServerboundPlayerCommandPacket.Action`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 7")]
pub enum ServerboundPlayerCommandAction {
    StopSleeping,
    StartSprinting,
    StopSprinting,
    StartRidingJump,
    StopRidingJump,
    OpenInventory,
    StartFallFlying,
}

/// `ServerboundPlayerCommandPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "player_command")]
pub struct ServerboundPlayerCommandPacket {
    #[mc(var)]
    pub id: i32,
    pub action: ServerboundPlayerCommandAction,
    #[mc(var)]
    pub data: i32,
}

/// `Input` (`Input.STREAM_CODEC`): one flags byte, bits 0..=6 = forward, backward, left, right, jump, shift, sprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Input {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}

impl Encode for Input {
    fn encode(&self, out: &mut Vec<u8>) {
        let bits = [
            self.forward,
            self.backward,
            self.left,
            self.right,
            self.jump,
            self.shift,
            self.sprint,
        ];
        out.push(
            bits.iter()
                .enumerate()
                .fold(0, |acc, (i, &b)| acc | u8::from(b) << i),
        )
    }
}
impl Decode for Input {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let f = u8::decode(input)?;
        let bit = |i: u8| f >> i & 1 != 0;
        Ok(Self {
            forward: bit(0),
            backward: bit(1),
            left: bit(2),
            right: bit(3),
            jump: bit(4),
            shift: bit(5),
            sprint: bit(6),
        })
    }
}

/// `ServerboundPlayerInputPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "player_input")]
pub struct ServerboundPlayerInputPacket {
    pub input: Input,
}

/// `ServerboundSetBeaconPacket`: optional `Holder<MobEffect>`s (static registry, VarInt ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_beacon")]
pub struct ServerboundSetBeaconPacket {
    pub primary: Option<mc_data::MobEffect>,
    pub secondary: Option<mc_data::MobEffect>,
}

// ---- movement ----------------------------------------------------------------------------------

/// `ServerboundMovePlayerPacket` subclasses: the payload fields followed by one flags byte
/// (bit 0 on ground, bit 1 horizontal collision; higher bits are ignored on decode).
macro_rules! move_player {
    ($(#[$m:meta])* $name:ident, $wire:literal { $($field:ident: $ty:ty),* }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Packet)]
        #[packet(state = play, flow = serverbound, name = $wire)]
        pub struct $name {
            $(pub $field: $ty,)*
            pub on_ground: bool,
            pub horizontal_collision: bool,
        }

        impl Encode for $name {
            fn encode(&self, out: &mut Vec<u8>) {
                $(self.$field.encode(out);)*
                out.push(u8::from(self.on_ground) | u8::from(self.horizontal_collision) << 1)
            }
        }
        impl Decode for $name {
            fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
                $(let $field = <$ty>::decode(input)?;)*
                let flags = u8::decode(input)?;
                Ok(Self { $($field,)* on_ground: flags & 1 != 0, horizontal_collision: flags & 2 != 0 })
            }
        }
    };
}

/// The abstract `ServerboundMovePlayerPacket` accessors: a component the packet does not carry reads as `fallback`.
macro_rules! move_player_getters {
    (@pick $s:ident none $fb:ident) => { $fb };
    (@pick $s:ident $f:ident $fb:ident) => { $s.$f };
    ($name:ident, has_pos = $has_pos:literal, has_rot = $has_rot:literal, x = $x:tt, y = $y:tt, z = $z:tt, y_rot = $yr:tt, x_rot = $xr:tt) => {
        #[allow(unused_variables)]
        impl $name {
            pub fn get_x(&self, fallback: f64) -> f64 { move_player_getters!(@pick self $x fallback) }
            pub fn get_y(&self, fallback: f64) -> f64 { move_player_getters!(@pick self $y fallback) }
            pub fn get_z(&self, fallback: f64) -> f64 { move_player_getters!(@pick self $z fallback) }
            pub fn get_y_rot(&self, fallback: f32) -> f32 { move_player_getters!(@pick self $yr fallback) }
            pub fn get_x_rot(&self, fallback: f32) -> f32 { move_player_getters!(@pick self $xr fallback) }
            pub fn has_position(&self) -> bool { $has_pos }
            pub fn has_rotation(&self) -> bool { $has_rot }
            pub fn is_on_ground(&self) -> bool { self.on_ground }
            pub fn horizontal_collision(&self) -> bool { self.horizontal_collision }
        }
    };
}

move_player!(
    /// `ServerboundMovePlayerPacket.Pos`
    ServerboundMovePlayerPosPacket, "move_player_pos" { x: f64, y: f64, z: f64 }
);
move_player_getters!(
    ServerboundMovePlayerPosPacket,
    has_pos = true,
    has_rot = false,
    x = x,
    y = y,
    z = z,
    y_rot = none,
    x_rot = none
);
move_player!(
    /// `ServerboundMovePlayerPacket.PosRot`
    ServerboundMovePlayerPosRotPacket, "move_player_pos_rot" { x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32 }
);
move_player_getters!(
    ServerboundMovePlayerPosRotPacket,
    has_pos = true,
    has_rot = true,
    x = x,
    y = y,
    z = z,
    y_rot = y_rot,
    x_rot = x_rot
);
move_player!(
    /// `ServerboundMovePlayerPacket.Rot`
    ServerboundMovePlayerRotPacket, "move_player_rot" { y_rot: f32, x_rot: f32 }
);
move_player_getters!(
    ServerboundMovePlayerRotPacket,
    has_pos = false,
    has_rot = true,
    x = none,
    y = none,
    z = none,
    y_rot = y_rot,
    x_rot = x_rot
);
move_player!(
    /// `ServerboundMovePlayerPacket.StatusOnly`
    ServerboundMovePlayerStatusOnlyPacket, "move_player_status_only" {}
);
move_player_getters!(
    ServerboundMovePlayerStatusOnlyPacket,
    has_pos = false,
    has_rot = false,
    x = none,
    y = none,
    z = none,
    y_rot = none,
    x_rot = none
);

// ---- block editors -----------------------------------------------------------------------------

/// `CommandBlockEntity.Mode`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 3")]
pub enum CommandBlockEntityMode {
    Sequence,
    Auto,
    Redstone,
}

/// `ServerboundSetCommandBlockPacket`: flags byte bit 0 track output, 1 conditional, 2 automatic.
#[derive(Debug, Clone, PartialEq, Eq, Packet)]
#[packet(state = play, flow = serverbound, name = "set_command_block")]
pub struct ServerboundSetCommandBlockPacket {
    pub pos: BlockPos,
    pub command: String,
    pub mode: CommandBlockEntityMode,
    pub track_output: bool,
    pub conditional: bool,
    pub automatic: bool,
}

impl Encode for ServerboundSetCommandBlockPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.command.encode(out);
        self.mode.encode(out);
        out.push(
            u8::from(self.track_output)
                | u8::from(self.conditional) << 1
                | u8::from(self.automatic) << 2,
        )
    }
}
impl Decode for ServerboundSetCommandBlockPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let pos = BlockPos::decode(input)?;
        let command = String::decode(input)?;
        let mode = CommandBlockEntityMode::decode(input)?;
        let flags = u8::decode(input)?;
        Ok(Self {
            pos,
            command,
            mode,
            track_output: flags & 1 != 0,
            conditional: flags & 2 != 0,
            automatic: flags & 4 != 0,
        })
    }
}

/// `ServerboundSetCommandMinecartPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_command_minecart")]
pub struct ServerboundSetCommandMinecartPacket {
    #[mc(var)]
    pub entity: i32,
    pub command: String,
    pub track_output: bool,
}

/// `JigsawBlockEntity.JointType`: sent as its serialized name; unknown names decode as `Aligned`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JigsawBlockEntityJointType {
    Rollable,
    Aligned,
}

impl Encode for JigsawBlockEntityJointType {
    fn encode(&self, out: &mut Vec<u8>) {
        write_utf(
            out,
            match self {
                Self::Rollable => "rollable",
                Self::Aligned => "aligned",
            },
        )
    }
}
impl Decode for JigsawBlockEntityJointType {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(match String::decode(input)?.as_str() {
            "rollable" => Self::Rollable,
            _ => Self::Aligned,
        })
    }
}

/// `ServerboundSetJigsawBlockPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "set_jigsaw_block")]
pub struct ServerboundSetJigsawBlockPacket {
    pub pos: BlockPos,
    pub name: mc_core::ident::Identifier,
    pub target: mc_core::ident::Identifier,
    pub pool: mc_core::ident::Identifier,
    pub final_state: String,
    pub joint: JigsawBlockEntityJointType,
    #[mc(var)]
    pub selection_priority: i32,
    #[mc(var)]
    pub placement_priority: i32,
}

/// `StructureBlockEntity.UpdateType`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 4")]
pub enum StructureBlockEntityUpdateType {
    UpdateData,
    SaveArea,
    LoadArea,
    ScanArea,
}

/// `StructureMode`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 4")]
pub enum StructureMode {
    Save,
    Load,
    Corner,
    Data,
}

/// `ServerboundSetStructureBlockPacket`. Decoding clamps like vanilla: offset to `-48..=48`, size to `0..=48`,
/// integrity to `0.0..=1.0` (NaN passes through); the three offset/size components are single (signed) bytes.
#[derive(Debug, Clone, PartialEq, Packet)]
#[packet(state = play, flow = serverbound, name = "set_structure_block")]
pub struct ServerboundSetStructureBlockPacket {
    pub pos: BlockPos,
    pub update_type: StructureBlockEntityUpdateType,
    pub mode: StructureMode,
    pub name: String,
    pub offset: BlockPos,
    pub size: Vec3i,
    pub mirror: Mirror,
    pub rotation: Rotation,
    pub data: String,
    pub ignore_entities: bool,
    pub strict: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub integrity: f32,
    pub seed: i64,
}

impl Encode for ServerboundSetStructureBlockPacket {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.update_type.encode(out);
        self.mode.encode(out);
        self.name.encode(out);
        out.extend(
            [
                self.offset.x(),
                self.offset.y(),
                self.offset.z(),
                self.size.x,
                self.size.y,
                self.size.z,
            ]
            .map(|v| v as u8),
        );
        mirror_enum::encode(&self.mirror, out);
        rotation_id::encode(&self.rotation, out);
        self.data.encode(out);
        self.integrity.encode(out);
        crate::codec::VarEncode::encode_var(&self.seed, out);
        out.push(
            u8::from(self.ignore_entities)
                | u8::from(self.show_air) << 1
                | u8::from(self.show_bounding_box) << 2
                | u8::from(self.strict) << 3,
        )
    }
}
impl Decode for ServerboundSetStructureBlockPacket {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let pos = BlockPos::decode(input)?;
        let update_type = StructureBlockEntityUpdateType::decode(input)?;
        let mode = StructureMode::decode(input)?;
        let name = String::decode(input)?;
        let mut off = [0i32; 3];
        for v in &mut off {
            *v = i32::from(i8::decode(input)?).clamp(-48, 48);
        }
        let mut size = [0i32; 3];
        for v in &mut size {
            *v = i32::from(i8::decode(input)?).clamp(0, 48);
        }
        let mirror = mirror_enum::decode(input)?;
        let rotation = rotation_id::decode(input)?;
        let data = read_utf(input, 128)?;
        // `Mth.clamp(float)`: `value < min ? min : Math.min(value, max)`; NaN stays NaN.
        let integrity = f32::decode(input)?;
        let integrity = if integrity < 0.0 {
            0.0
        } else if integrity > 1.0 {
            1.0
        } else {
            integrity
        };
        let seed = <i64 as crate::codec::VarEncode>::decode_var(input)?;
        let flags = u8::decode(input)?;
        Ok(Self {
            pos,
            update_type,
            mode,
            name,
            offset: BlockPos::new(off[0], off[1], off[2]),
            size: Vec3i::new(size[0], size[1], size[2]),
            mirror,
            rotation,
            data,
            ignore_entities: flags & 1 != 0,
            strict: flags & 8 != 0,
            show_air: flags & 2 != 0,
            show_bounding_box: flags & 4 != 0,
            integrity,
            seed,
        })
    }
}

id_enum!(
    /// `SignTextSlot` (VarInt id, unknown = back)
    SignTextSlot { Back = 0, Front = 1 } out_of_bounds = ZERO
);

/// `ServerboundSignUpdatePacket`: exactly four lines of at most 384 chars each, no length prefix.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "sign_update")]
pub struct ServerboundSignUpdatePacket {
    pub pos: BlockPos,
    #[mc(with = sign_lines)]
    pub lines: [String; 4],
    pub slot: SignTextSlot,
}

/// `ServerboundSpectatorActionPacket`: `OPTIONAL_VAR_INT` entity id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "spectator_action")]
pub struct ServerboundSpectatorActionPacket {
    #[mc(with = optional_var_int)]
    pub spectate_entity_id: Option<i32>,
}

/// `ServerboundTeleportToEntityPacket`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "teleport_to_entity")]
pub struct ServerboundTeleportToEntityPacket {
    pub uuid: Uuid,
}

// ---- use item ----------------------------------------------------------------------------------

/// `BlockHitResult.STREAM_CODEC`: block position, face, the click offset inside the block as floats, inside flag
/// and world-border flag. (Vanilla's decoded `location` is `pos + click` in doubles: see [`Self::location`].)
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct BlockHitResult {
    pub block_pos: BlockPos,
    pub direction: Direction,
    pub click_x: f32,
    pub click_y: f32,
    pub click_z: f32,
    pub inside: bool,
    pub world_border_hit: bool,
}

impl BlockHitResult {
    /// `BlockHitResult.getLocation()` as vanilla reconstructs it from the wire.
    pub fn location(&self) -> Vec3 {
        Vec3::new(
            f64::from(self.block_pos.x()) + f64::from(self.click_x),
            f64::from(self.block_pos.y()) + f64::from(self.click_y),
            f64::from(self.block_pos.z()) + f64::from(self.click_z),
        )
    }
}

/// `ServerboundUseItemOnPacket`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "use_item_on")]
pub struct ServerboundUseItemOnPacket {
    pub hand: InteractionHand,
    pub hit_result: BlockHitResult,
    #[mc(var)]
    pub sequence: i32,
}

/// `ServerboundUseItemPacket`
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "use_item")]
pub struct ServerboundUseItemPacket {
    pub hand: InteractionHand,
    #[mc(var)]
    pub sequence: i32,
    pub y_rot: f32,
    pub x_rot: f32,
}
