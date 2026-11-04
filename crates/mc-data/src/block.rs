//! Blocks and block states.
//!
//! [`Block`] is the registry id of a block (`Block::STONE`); [`BlockState`] is the global block state id
//! (`Block.BLOCK_STATE_REGISTRY` id, the id used on the wire and in chunk palettes). Within a block the states are laid
//! out in mixed radix over the block's properties (sorted by name, the last property varies fastest), which is how
//! [`BlockState::get`]/[`BlockState::set`] work without any per-state table.

use crate::enums::{NoteBlockInstrument, PushReaction};
use crate::fluid::FluidState;
use crate::props::{PROPERTY_DEFS, Property, PropertyDef};
use crate::shape::ShapeId;
use mc_core::direction::Direction;
use std::fmt;

pub use crate::ids::Block;
use crate::ids::{Item, SoundEvent};

/// `SupportType` (index order of [`StateData::face_sturdy`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportType {
    Full,
    Center,
    Rigid,
}

/// `PathComputationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathComputationType {
    Land,
    Water,
    Air,
}

/// `BlockBehaviour.OffsetType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetType {
    None,
    Xz,
    Xyz,
}

/// `SoundType`.
#[derive(Debug)]
pub struct SoundType {
    pub volume: f32,
    pub pitch: f32,
    pub break_sound: SoundEvent,
    pub step_sound: SoundEvent,
    pub place_sound: SoundEvent,
    pub hit_sound: SoundEvent,
    pub fall_sound: SoundEvent,
}

/// Bits of [`BlockData::flags`].
pub mod block_flags {
    pub const HAS_COLLISION: u32 = 1 << 0;
    pub const REQUIRES_CORRECT_TOOL_FOR_DROPS: u32 = 1 << 1;
    pub const RANDOM_TICKS: u32 = 1 << 2;
    pub const DYNAMIC_SHAPE: u32 = 1 << 3;
    pub const IGNITED_BY_LAVA: u32 = 1 << 4;
    pub const REPLACEABLE: u32 = 1 << 5;
    pub const CAN_OCCLUDE: u32 = 1 << 6;
    pub const IS_AIR: u32 = 1 << 7;
    pub const LIQUID: u32 = 1 << 8;
    pub const FORCE_SOLID_ON: u32 = 1 << 9;
    pub const FORCE_SOLID_OFF: u32 = 1 << 10;
    pub const SPAWN_TERRAIN_PARTICLES: u32 = 1 << 11;
    pub const HAS_BLOCK_ENTITY: u32 = 1 << 12;
}

/// Bits of [`StateData::flags`].
pub mod state_flags {
    pub const CAN_OCCLUDE: u32 = 1 << 0;
    pub const SOLID_RENDER: u32 = 1 << 1;
    pub const IS_AIR: u32 = 1 << 2;
    pub const PROPAGATES_SKYLIGHT_DOWN: u32 = 1 << 3;
    pub const USE_SHAPE_FOR_LIGHT_OCCLUSION: u32 = 1 << 4;
    pub const IS_REDSTONE_CONDUCTOR: u32 = 1 << 5;
    pub const IS_SUFFOCATING: u32 = 1 << 6;
    pub const IS_VIEW_BLOCKING: u32 = 1 << 7;
    pub const HAS_LARGE_COLLISION_SHAPE: u32 = 1 << 8;
    pub const REQUIRES_CORRECT_TOOL_FOR_DROPS: u32 = 1 << 9;
    pub const IS_COLLISION_SHAPE_FULL_BLOCK: u32 = 1 << 10;
    pub const LEGACY_SOLID: u32 = 1 << 11;
    pub const RANDOM_TICKING: u32 = 1 << 12;
    pub const SIGNAL_SOURCE: u32 = 1 << 13;
    pub const HAS_ANALOG_OUTPUT_SIGNAL: u32 = 1 << 14;
    pub const EMISSIVE_RENDERING: u32 = 1 << 15;
    pub const RENDER_MODEL: u32 = 1 << 16;
    pub const CAN_BE_REPLACED: u32 = 1 << 17;
    pub const LIQUID: u32 = 1 << 18;
    pub const IGNITED_BY_LAVA: u32 = 1 << 19;
    pub const HAS_OFFSET: u32 = 1 << 20;
}

/// Block-level data (`Block` / `BlockBehaviour.Properties`).
#[derive(Debug)]
pub struct BlockData {
    pub id: u32,
    pub name: &'static str,
    /// Simple name of the vanilla Java class (`StairBlock`).
    pub class: &'static str,
    /// Class chain from the concrete class up to `BlockBehaviour`.
    pub classes: &'static [&'static str],
    pub description_id: &'static str,
    pub default_state: u16,
    pub first_state: u16,
    pub state_count: u16,
    /// Indices into [`PROPERTY_DEFS`], sorted by property name.
    pub props: &'static [u16],
    pub destroy_time: f32,
    pub explosion_resistance: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub bounce_restitution: f32,
    pub fall_distance_reduction: f32,
    /// [`block_flags`].
    pub flags: u32,
    pub push_reaction: PushReaction,
    pub instrument: NoteBlockInstrument,
    /// `MapColor` id of the default state.
    pub map_color: u8,
    /// Index into [`SOUND_TYPES`] for the default state.
    pub sound_type: u16,
    pub max_horizontal_offset: f32,
    pub max_vertical_offset: f32,
    pub offset_type: OffsetType,
    pub loot_table: Option<&'static str>,
    pub item: Option<Item>,
    pub required_features: &'static [&'static str],
}

impl BlockData {
    pub const fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// Per-state data (`BlockBehaviour.BlockStateBase`), shapes evaluated with `EmptyBlockGetter` at `BlockPos.ZERO` and
/// `CollisionContext.empty()`. Identical states share one entry.
#[derive(Debug)]
pub struct StateData {
    /// [`state_flags`].
    pub flags: u32,
    pub light_emission: u8,
    /// `getLightDampening()` (a.k.a. light block).
    pub light_block: u8,
    pub destroy_speed: f32,
    /// `getShadeBrightness`.
    pub shade_brightness: f32,
    /// Bit `direction.ordinal() * 3 + SupportType.ordinal()` is `isFaceSturdy(direction, type)`.
    pub face_sturdy: u32,
    /// Bit `PathComputationType.ordinal()` is `isPathfindable(type)`.
    pub pathfindable: u8,
    pub fluid_state: FluidState,
    /// Index into [`SOUND_TYPES`] (`getSoundType()`).
    pub sound_type: u16,
    /// `MapColor` id.
    pub map_color: u8,
    /// `[collision, outline, occlusion, block support, interaction, visual]`.
    pub shapes: [ShapeId; 6],
}

include!(concat!(env!("OUT_DIR"), "/blocks.rs"));

/// Number of blocks (registry size).
pub const BLOCK_COUNT: usize = BLOCK_DATA.len();

/// Global block state id (`Block.getId(state)`). The state's block and all per-state data are table lookups.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockState(pub(crate) u16);

impl Block {
    pub fn data(self) -> &'static BlockData {
        &BLOCK_DATA[self.0 as usize]
    }

    pub fn default_state(self) -> BlockState {
        BlockState(self.data().default_state)
    }

    /// All states in id order.
    pub fn states(self) -> impl ExactSizeIterator<Item = BlockState> + Clone {
        let d = self.data();
        (d.first_state..d.first_state + d.state_count).map(BlockState)
    }

    pub fn state_count(self) -> usize {
        self.data().state_count as usize
    }

    /// The block's properties in vanilla order (sorted by name).
    pub fn properties(self) -> impl ExactSizeIterator<Item = &'static PropertyDef> {
        self.data().props.iter().map(|&p| &PROPERTY_DEFS[p as usize])
    }

    pub fn has_property<P: Property>(self, p: P) -> bool {
        self.data().props.iter().any(|&i| i as usize == p.index())
    }

    pub fn sound_type(self) -> &'static SoundType {
        &SOUND_TYPES[self.data().sound_type as usize]
    }

    /// `Block.asItem()` (None for `Items.AIR`).
    pub fn item(self) -> Option<Item> {
        self.data().item
    }

    pub fn is_air(self) -> bool {
        self.data().has(block_flags::IS_AIR)
    }

    /// State with the given `(property name, value name)` pairs applied to the default state; None if a property does not
    /// exist or a value is not one of its values (vanilla `StateHolder` codec behaviour).
    pub fn state_from_properties<'a>(self, props: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<BlockState> {
        let mut s = self.default_state();
        for (name, value) in props {
            s = s.with_property_str(name, value)?;
        }
        Some(s)
    }

    /// `state.is(tag)` for a block tag of the vanilla data pack (`minecraft:mineable/pickaxe` or `mineable/pickaxe`).
    /// Does a hash lookup per call: cache `tags::get::<Block>(..)` in hot paths.
    pub fn is_in_tag(self, tag: &str) -> bool {
        crate::tags::get::<Block>(tag).is_some_and(|t| t.contains(self))
    }
}

impl BlockState {
    /// Global state id.
    pub const fn id(self) -> u32 {
        self.0 as u32
    }

    pub fn from_id(id: u32) -> Option<BlockState> {
        if (id as usize) < STATE_COUNT { Some(BlockState(id as u16)) } else { None }
    }

    pub fn block(self) -> Block {
        Block(STATE_BLOCK[self.0 as usize])
    }

    pub fn data(self) -> &'static StateData {
        &STATE_DATA[STATE_DATA_INDEX[self.0 as usize] as usize]
    }

    pub fn is_default(self) -> bool {
        self == self.block().default_state()
    }

    fn local(self) -> usize {
        (self.0 - self.block().data().first_state) as usize
    }

    /// (stride, size) of a property within this block's state layout.
    fn locate(self, prop: usize) -> Option<(usize, usize)> {
        let mut stride = 1;
        for &pi in self.block().data().props.iter().rev() {
            let size = PROPERTY_DEFS[pi as usize].values.len();
            if pi as usize == prop {
                return Some((stride, size));
            }
            stride *= size;
        }
        None
    }

    pub fn has_property<P: Property>(self, p: P) -> bool {
        self.locate(p.index()).is_some()
    }

    /// `getValue`: panics if the block has no such property (vanilla throws `IllegalArgumentException`).
    pub fn get<P: Property>(self, p: P) -> P::Value {
        self.try_get(p).unwrap_or_else(|| panic!("{} does not have property {}", self.block(), p.def().name))
    }

    /// `getOptionalValue`.
    pub fn try_get<P: Property>(self, p: P) -> Option<P::Value> {
        let (stride, size) = self.locate(p.index())?;
        Some(p.value(self.local() / stride % size))
    }

    /// `trySetValue`: None if the block has no such property.
    pub fn try_set<P: Property>(self, p: P, value: P::Value) -> Option<BlockState> {
        let (stride, size) = self.locate(p.index())?;
        let cur = self.local() / stride % size;
        let new = p.value_index(value);
        Some(BlockState((self.0 as usize + new * stride - cur * stride) as u16))
    }

    /// `setValue`: panics if the block has no such property or the property cannot hold the value.
    pub fn set<P: Property>(self, p: P, value: P::Value) -> BlockState {
        self.try_set(p, value).unwrap_or_else(|| panic!("{} does not have property {}", self.block(), p.def().name))
    }

    /// Alias of [`BlockState::set`] (vanilla `setValue`).
    pub fn with_value<P: Property>(self, p: P, value: P::Value) -> BlockState {
        self.set(p, value)
    }

    /// `cycle(property)`: next value, wrapping around.
    pub fn cycle<P: Property>(self, p: P) -> BlockState {
        let (stride, size) = self
            .locate(p.index())
            .unwrap_or_else(|| panic!("{} does not have property {}", self.block(), p.def().name));
        let cur = self.local() / stride % size;
        let next = (cur + 1) % size;
        BlockState((self.0 as usize + next * stride - cur * stride) as u16)
    }

    /// `(definition, value index)` for every property of the block.
    pub fn values(self) -> impl Iterator<Item = (&'static PropertyDef, usize)> {
        let local = self.local();
        let mut stride = self.block().data().props.iter().map(|&p| PROPERTY_DEFS[p as usize].values.len()).product::<usize>().max(1);
        self.block().data().props.iter().map(move |&pi| {
            let def = &PROPERTY_DEFS[pi as usize];
            stride /= def.values.len();
            (def, local / stride % def.values.len())
        })
    }

    /// Serialized value of a property by serialized name.
    pub fn get_property_str(self, name: &str) -> Option<&'static str> {
        self.values().find(|(d, _)| d.name == name).map(|(d, i)| d.values[i])
    }

    /// Sets a property from its serialized name/value (e.g. when reading `Properties` from NBT or a command).
    pub fn with_property_str(self, name: &str, value: &str) -> Option<BlockState> {
        let (pi, def) = self.block().data().props.iter().map(|&p| (p as usize, &PROPERTY_DEFS[p as usize])).find(|(_, d)| d.name == name)?;
        let new = def.value_index(value)?;
        let (stride, size) = self.locate(pi)?;
        let cur = self.local() / stride % size;
        Some(BlockState((self.0 as usize + new * stride - cur * stride) as u16))
    }

    // --- BlockStateBase queries -------------------------------------------------------------------------------

    fn flag(self, f: u32) -> bool {
        self.data().flags & f != 0
    }

    pub fn get_light_emission(self) -> u8 {
        self.data().light_emission
    }
    /// `getLightDampening()`.
    pub fn get_light_block(self) -> u8 {
        self.data().light_block
    }
    pub fn can_occlude(self) -> bool {
        self.flag(state_flags::CAN_OCCLUDE)
    }
    pub fn is_solid_render(self) -> bool {
        self.flag(state_flags::SOLID_RENDER)
    }
    pub fn is_air(self) -> bool {
        self.flag(state_flags::IS_AIR)
    }
    pub fn propagates_skylight_down(self) -> bool {
        self.flag(state_flags::PROPAGATES_SKYLIGHT_DOWN)
    }
    pub fn use_shape_for_light_occlusion(self) -> bool {
        self.flag(state_flags::USE_SHAPE_FOR_LIGHT_OCCLUSION)
    }
    /// `isRedstoneConductor(EmptyBlockGetter, BlockPos.ZERO)`.
    pub fn is_redstone_conductor(self) -> bool {
        self.flag(state_flags::IS_REDSTONE_CONDUCTOR)
    }
    /// `isSuffocating(EmptyBlockGetter, BlockPos.ZERO)` (evaluated with the vanilla block tags bound).
    pub fn is_suffocating(self) -> bool {
        self.flag(state_flags::IS_SUFFOCATING)
    }
    pub fn is_view_blocking(self) -> bool {
        self.flag(state_flags::IS_VIEW_BLOCKING)
    }
    pub fn has_large_collision_shape(self) -> bool {
        self.flag(state_flags::HAS_LARGE_COLLISION_SHAPE)
    }
    pub fn requires_correct_tool_for_drops(self) -> bool {
        self.flag(state_flags::REQUIRES_CORRECT_TOOL_FOR_DROPS)
    }
    pub fn is_collision_shape_full_block(self) -> bool {
        self.flag(state_flags::IS_COLLISION_SHAPE_FULL_BLOCK)
    }
    /// Deprecated vanilla `isSolid()`.
    pub fn is_solid(self) -> bool {
        self.flag(state_flags::LEGACY_SOLID)
    }
    pub fn is_randomly_ticking(self) -> bool {
        self.flag(state_flags::RANDOM_TICKING)
    }
    pub fn is_signal_source(self) -> bool {
        self.flag(state_flags::SIGNAL_SOURCE)
    }
    pub fn has_analog_output_signal(self) -> bool {
        self.flag(state_flags::HAS_ANALOG_OUTPUT_SIGNAL)
    }
    pub fn emissive_rendering(self) -> bool {
        self.flag(state_flags::EMISSIVE_RENDERING)
    }
    /// `getRenderShape() == RenderShape.MODEL`.
    pub fn renders_model(self) -> bool {
        self.flag(state_flags::RENDER_MODEL)
    }
    pub fn can_be_replaced(self) -> bool {
        self.flag(state_flags::CAN_BE_REPLACED)
    }
    pub fn liquid(self) -> bool {
        self.flag(state_flags::LIQUID)
    }
    pub fn ignited_by_lava(self) -> bool {
        self.flag(state_flags::IGNITED_BY_LAVA)
    }
    pub fn has_offset_function(self) -> bool {
        self.flag(state_flags::HAS_OFFSET)
    }
    pub fn is_light_permeable(self) -> bool {
        !self.is_solid_render() || self.get_light_block() == 0
    }
    pub fn get_destroy_speed(self) -> f32 {
        self.data().destroy_speed
    }
    pub fn get_shade_brightness(self) -> f32 {
        self.data().shade_brightness
    }
    pub fn is_face_sturdy(self, direction: Direction, support: SupportType) -> bool {
        self.data().face_sturdy >> (direction.ordinal() * 3 + support as usize) & 1 != 0
    }
    pub fn is_pathfindable(self, ty: PathComputationType) -> bool {
        self.data().pathfindable >> ty as usize & 1 != 0
    }
    pub fn get_fluid_state(self) -> FluidState {
        self.data().fluid_state
    }
    pub fn get_sound_type(self) -> &'static SoundType {
        &SOUND_TYPES[self.data().sound_type as usize]
    }
    /// `MapColor` id (`getMapColor(EmptyBlockGetter, BlockPos.ZERO).id`).
    pub fn get_map_color(self) -> u8 {
        self.data().map_color
    }

    // --- shapes -----------------------------------------------------------------------------------------------

    /// `getCollisionShape(EmptyBlockGetter, ZERO, CollisionContext.empty())`.
    pub fn collision_shape(self) -> ShapeId {
        self.data().shapes[0]
    }
    /// `getShape` (the outline).
    pub fn shape(self) -> ShapeId {
        self.data().shapes[1]
    }
    /// `getOcclusionShape()`.
    pub fn occlusion_shape(self) -> ShapeId {
        self.data().shapes[2]
    }
    pub fn block_support_shape(self) -> ShapeId {
        self.data().shapes[3]
    }
    pub fn interaction_shape(self) -> ShapeId {
        self.data().shapes[4]
    }
    pub fn visual_shape(self) -> ShapeId {
        self.data().shapes[5]
    }
}

impl fmt::Display for BlockState {
    /// Vanilla `BlockState.toString()` form: `minecraft:oak_stairs[facing=north,half=top,...]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.block().name())?;
        let mut first = true;
        for (def, i) in self.values() {
            f.write_str(if first { "[" } else { "," })?;
            first = false;
            write!(f, "{}={}", def.name, def.values[i])?;
        }
        if !first {
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl fmt::Debug for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockState({}: {})", self.0, self)
    }
}
