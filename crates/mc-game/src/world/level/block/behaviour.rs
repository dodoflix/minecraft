//! `BlockBehaviour` (+ the overridable parts of `Block`) as a trait with vanilla defaults, and the registry that maps
//! every `Block` id to its static implementor.
//!
//! **Static data** (shapes, flags, light, hardness, ...) is NOT here: it comes from `mc_data` (`BlockState::data()`).
//! This trait holds the *logic* overridden by the vanilla `Block` subclasses. Each Java class becomes one Rust type
//! implementing the trait (parameterized when the Java class takes constructor arguments, e.g. `StairBlock{base}`),
//! registered with [`register_block_class!`] (all blocks whose vanilla Java class has that simple name) or
//! [`register_block!`] (one specific block, e.g. an anonymous subclass), from the module that implements it.
//! Blocks with no registration fall back along the Java class chain (`mc_data::BlockData::classes`), finally to
//! [`PlainBlock`] = vanilla's base `BlockBehaviour`/`Block` behavior.
//!
//! The signatures follow vanilla (`protected` methods are public here). `ServerLevel` replaces vanilla's `Level`
//! because the dedicated server only ever runs server levels. Methods that need `ItemStack`/`Player`/loot types are
//! added by the block slice as those types land (additive: defaults exist for every method).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::LevelAccessor;
use crate::world::level::LevelReader;
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use std::sync::OnceLock;

pub trait BlockBehaviour: Send + Sync + 'static {
    /// `updateShape`: called when a neighbor's shape changes; returns the (possibly changed) state.
    #[allow(clippy::too_many_arguments)]
    fn update_shape(
        &self,
        state: BlockState,
        _level: &mut dyn LevelAccessor,
        _pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockState,
        _random: &mut dyn RandomSource,
    ) -> BlockState {
        state
    }
    /// `neighborChanged` (server side).
    fn neighbor_changed(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _source_block: Block, _moved_by_piston: bool) {}
    /// `onPlace`.
    fn on_place(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {}
    /// `affectNeighborsAfterRemoval`.
    fn affect_neighbors_after_removal(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _moved_by_piston: bool) {}
    /// `triggerEvent` (block events: `level.blockEvent`).
    fn trigger_event(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _b0: i32, _b1: i32) -> bool {
        false
    }
    /// `canSurvive`.
    fn can_survive(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos) -> bool {
        true
    }
    /// `tick` (scheduled tick).
    fn tick(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _random: &mut dyn RandomSource) {}
    /// `randomTick`.
    fn random_tick(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _random: &mut dyn RandomSource) {}
    /// `entityInside` (the entity is not part of the level's entity slot while it is being ticked).
    fn entity_inside(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _entity: &mut dyn Entity) {}
    /// `getSignal` / `getDirectSignal` (redstone).
    fn get_signal(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }
    fn get_direct_signal(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }
    /// `getAnalogOutputSignal`.
    fn get_analog_output_signal(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }
    /// `shouldChangedStateKeepBlockEntity`.
    fn should_changed_state_keep_block_entity(&self, _old_state: BlockState) -> bool {
        false
    }
}

/// Vanilla's base behavior (every method default).
pub struct PlainBlock;
impl BlockBehaviour for PlainBlock {}

/// Registers behavior for every block whose Java class simple name is `class`.
pub struct BlockClassRegistration {
    pub class: &'static str,
    pub create: fn(Block) -> Box<dyn BlockBehaviour>,
}
/// Registers behavior for one block by its registry name (e.g. `minecraft:tnt`), overriding class registrations.
pub struct BlockRegistration {
    pub block: &'static str,
    pub create: fn(Block) -> Box<dyn BlockBehaviour>,
}

#[distributed_slice]
pub static BLOCK_CLASSES: [BlockClassRegistration];
#[distributed_slice]
pub static BLOCKS: [BlockRegistration];

/// `register_block_class!("StairBlock", |block| Box::new(StairBlock::new(block)))`.
#[macro_export]
macro_rules! register_block_class {
    ($class:literal, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::level::block::BLOCK_CLASSES)]
            static REG: $crate::world::level::block::BlockClassRegistration =
                $crate::world::level::block::BlockClassRegistration { class: $class, create: $create };
        };
    };
}
/// `register_block!("minecraft:tnt", |block| Box::new(TntBlock))`.
#[macro_export]
macro_rules! register_block {
    ($name:literal, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::level::block::BLOCKS)]
            static REG: $crate::world::level::block::BlockRegistration =
                $crate::world::level::block::BlockRegistration { block: $name, create: $create };
        };
    };
}

static TABLE: OnceLock<Vec<Box<dyn BlockBehaviour>>> = OnceLock::new();

fn build(block: Block) -> Box<dyn BlockBehaviour> {
    let data = block.data();
    if let Some(r) = BLOCKS.iter().find(|r| r.block == data.name) {
        return (r.create)(block);
    }
    for class in data.classes {
        if let Some(r) = BLOCK_CLASSES.iter().find(|r| r.class == *class) {
            return (r.create)(block);
        }
    }
    Box::new(PlainBlock)
}

/// The static behavior implementor of `block` (`BlockState.getBlock()` + virtual dispatch in vanilla).
pub fn behaviour(block: Block) -> &'static dyn BlockBehaviour {
    TABLE.get_or_init(|| Block::iter().map(build).collect())[block.id() as usize].as_ref()
}
