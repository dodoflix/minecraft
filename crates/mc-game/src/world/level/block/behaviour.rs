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
use crate::world::item::BlockPlaceContext;
use crate::world::level::collision::CollisionContext;
use crate::world::level::precipitation::Precipitation;
use crate::world::level::redstone::Orientation;
use crate::world::level::shapes::voxel_shape;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState, PathComputationType};
use mc_data::fluid::Fluid;
use std::sync::OnceLock;

/// Overridable behavior of `BlockBehaviour` + `Block`. Every method's default is vanilla's base implementation (static
/// per-state data such as light, flags and the default shapes comes from `mc_data`, which already reflects the Java
/// overrides evaluated with an empty level).
///
/// Deviations from the Java signatures (forced by ownership, no observable effect):
/// * `tick`/`randomTick`/`updateShape` take no `RandomSource`: vanilla always passes `level.getRandom()`, use
///   `level.random()` (the same stream, so call interleaving is identical).
/// * `Level` is `ServerLevel` (the dedicated server never runs client levels); `LevelAccessor`-taking methods get
///   `&mut dyn LevelAccessor` (works for `ServerLevel` and world generation regions).
/// * Purely client-side methods (`skipRendering`, `getRenderShape`, `animateTick`, `getShadeBrightness`, ...) are not
///   part of the trait; the data they derive from is in `mc_data`.
pub trait BlockBehaviour: Send + Sync + 'static {
    // ------------------------------------------------------------------------------------------- BlockBehaviour

    /// `updateIndirectNeighbourShapes`.
    fn update_indirect_neighbour_shapes(
        &self,
        _state: BlockState,
        _level: &mut dyn LevelAccessor,
        _pos: BlockPos,
        _update_flags: u32,
        _update_limit: i32,
    ) {
    }
    /// `isPathfindable`.
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        state.is_pathfindable(ty)
    }
    /// `updateShape`: called when a neighbor's shape changes; returns the (possibly changed) state.
    fn update_shape(
        &self,
        state: BlockState,
        _level: &mut dyn LevelAccessor,
        _pos: BlockPos,
        _direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        state
    }
    /// `neighborChanged`.
    fn neighbor_changed(
        &self,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
    }
    /// `onPlace`.
    fn on_place(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {}
    /// `affectNeighborsAfterRemoval`.
    fn affect_neighbors_after_removal(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _moved_by_piston: bool) {}
    /// `triggerEvent` (block events: `level.blockEvent`).
    fn trigger_event(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _b0: i32, _b1: i32) -> bool {
        false
    }
    /// `shouldChangedStateKeepBlockEntity`.
    fn should_changed_state_keep_block_entity(&self, _old_state: BlockState) -> bool {
        false
    }
    /// `rotate`.
    fn rotate(&self, state: BlockState, _rotation: Rotation) -> BlockState {
        state
    }
    /// `mirror`.
    fn mirror(&self, state: BlockState, _mirror: Mirror) -> BlockState {
        state
    }
    /// `canBeReplaced(state, fluid)`.
    fn can_be_replaced_by_fluid(&self, state: BlockState, _fluid: Fluid) -> bool {
        state.can_be_replaced() || !state.is_solid()
    }
    /// `canBeReplaced(state, context)`.
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let held = context.use_on.get_item_in_hand();
        state.can_be_replaced() && (held.is_empty() || Some(held.item()) != state.block().item())
    }
    /// `Block.getStateForPlacement(context)` (`None` = cannot be placed here).
    fn get_state_for_placement(&self, block: Block, _context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state())
    }
    /// `getSeed`.
    fn get_seed(&self, _state: BlockState, pos: BlockPos) -> i64 {
        mth::get_seed(&pos)
    }
    /// `getBlockSupportShape`: the per-state shape extracted from vanilla (`EmptyBlockGetter`), i.e. the base class's
    /// collision shape or the override of `LeavesBlock`, `MudBlock`, `SoulSandBlock`, `SnowLayerBlock`, `FenceGateBlock`, ....
    fn get_block_support_shape(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> VoxelShape {
        voxel_shape(state.block_support_shape())
    }
    /// `getInteractionShape`.
    fn get_interaction_shape(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> VoxelShape {
        Shapes::empty()
    }
    /// `canSurvive`.
    fn can_survive(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos) -> bool {
        true
    }
    /// `getAnalogOutputSignal`.
    fn get_analog_output_signal(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }
    /// `getShape` (outline).
    fn get_shape(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _context: &CollisionContext) -> VoxelShape {
        voxel_shape(state.shape())
    }
    /// `getCollisionShape`.
    fn get_collision_shape(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _context: &CollisionContext) -> VoxelShape {
        voxel_shape(state.collision_shape())
    }
    /// `getEntityInsideCollisionShape`.
    fn get_entity_inside_collision_shape(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _entity: &dyn Entity) -> VoxelShape {
        Shapes::block()
    }
    /// `isCollisionShapeFullBlock`.
    fn is_collision_shape_full_block(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_collision_shape_full_block()
    }
    /// `getVisualShape` (used by `ClipContext.Block.VISUAL`).
    fn get_visual_shape(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        self.get_collision_shape(state, level, pos, context)
    }
    /// `tick` (scheduled tick; the random is `level.random()`).
    fn tick(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos) {}
    /// `randomTick` (the random is `level.random()`).
    fn random_tick(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos) {}
    /// `entityInside` (the entity is not part of the level's entity slot while it is being ticked).
    fn entity_inside(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _entity: &mut dyn Entity) {}
    /// `shouldRedstoneWireConnectTo`.
    fn should_redstone_wire_connect_to(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Option<Direction>) -> bool {
        state.is_signal_source() && direction.is_some()
    }
    /// `getSignal`.
    fn get_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, _direction: Direction) -> i32 {
        self.own_signal(state, level, pos)
    }
    /// `ownSignal`.
    fn own_signal(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        0
    }
    /// `getDirectSignal`.
    fn get_direct_signal(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }

    // ------------------------------------------------------------------------------------------------------ Block

    /// `Block.stepOn`.
    fn step_on(&self, _level: &mut ServerLevel, _pos: BlockPos, _on_state: BlockState, _entity: &mut dyn Entity) {}
    /// `BlockBehaviour.useItemOn(stack, state, level, pos, player, hand, hitResult)`: default `TRY_WITH_EMPTY_HAND`.
    fn use_item_on(
        &self,
        _stack: &crate::world::item::ItemStack,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _player: &mut crate::world::entity::ServerPlayer,
        _hand: crate::world::item::InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> crate::world::item::InteractionResult {
        crate::world::item::InteractionResult::TRY_WITH_EMPTY_HAND
    }
    /// `BlockBehaviour.useWithoutItem(state, level, pos, player, hitResult)`: default `PASS`.
    fn use_without_item(
        &self,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> crate::world::item::InteractionResult {
        crate::world::item::InteractionResult::PASS
    }
    /// `BlockBehaviour.getCloneItemStack(level, pos, state, includeData)` (pick block): default `new ItemStack(this)`;
    /// blocks whose item differs (crops, fire, beds, ...) override.
    fn get_clone_item_stack(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> crate::world::item::ItemStack {
        match state.block().item() {
            Some(item) => crate::world::item::ItemStack::new(item),
            None => crate::world::item::ItemStack::empty(),
        }
    }
    /// `Block.setPlacedBy(level, pos, state, placer, stack)`: called by `BlockItem.place` after the block was set.
    fn set_placed_by(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _placer: Option<&mut crate::world::entity::ServerPlayer>, _stack: &crate::world::item::ItemStack) {}
    /// `BlockBehaviour.attack(state, level, pos, player)` (start of digging).
    fn attack(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _player: &mut crate::world::entity::ServerPlayer) {}
    /// `Block.playerWillDestroy`: the destroy particles/game event are done by the caller; returns the state to use.
    fn player_will_destroy(&self, state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _player: &mut crate::world::entity::ServerPlayer) -> BlockState {
        state
    }
    /// `Block.playerDestroy` (stats, exhaustion, drops). Loot drops are a later wave.
    fn player_destroy(
        &self,
        _level: &mut ServerLevel,
        _player: &mut crate::world::entity::ServerPlayer,
        _pos: BlockPos,
        _state: BlockState,
        _destroyed_with: &crate::world::item::ItemStack,
    ) {
        _player.player.food_data.add_exhaustion(0.005);
    }
    /// `Block.destroy` (called by `LevelChunk`-independent callers such as `ServerPlayerGameMode`).
    fn destroy(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState) {}
    /// `Block.handlePrecipitation`.
    fn handle_precipitation(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _precipitation: Precipitation) {}
    /// `Block.isPossibleToRespawnInThis`.
    fn is_possible_to_respawn_in_this(&self, state: BlockState) -> bool {
        !state.is_solid() && !state.liquid()
    }
    /// The block's part of `ServerPlayer.findRespawnAndUseSpawnBlock` (respawn anchors and beds override it): `None` when
    /// the block is not a spawn block, else `Some(stand-up position)` (`Some(None)` = it is one but unusable). A usable
    /// anchor consumes a charge when `consume_spawn_block && !forced`.
    fn find_respawn_position(
        &self,
        _level: &mut ServerLevel,
        _state: BlockState,
        _pos: BlockPos,
        _forced: bool,
        _consume_spawn_block: bool,
        _yaw: f32,
    ) -> Option<Option<mc_core::phys::Vec3>> {
        None
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
