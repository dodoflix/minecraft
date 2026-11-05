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
use crate::world::level::block::bonemeal::{BonemealSource, BonemealType};
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
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, _context: &BlockPlaceContext) -> Option<BlockState> {
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
    /// `entityInside(state, level, pos, entity, effectApplier, isPrecise)` (the entity is not part of the level's entity
    /// slot while it is being ticked).
    fn entity_inside(
        &self,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
    }
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
    /// `Block.fallOn(level, state, pos, entity, fallDistance)`.
    fn fall_on(&self, level: &mut ServerLevel, state: BlockState, _pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        let reduced_fall_distance = fall_distance * f64::from(1.0f32 - state.block().data().fall_distance_reduction);
        let source = level.damage_sources().fall();
        entity.cause_fall_damage(level, reduced_fall_distance, 1.0, &source);
    }
    /// `Block.bounceOn(level, state, pos, entity, fallDistance)`.
    fn bounce_on(&self, _level: &mut ServerLevel, _state: BlockState, _pos: BlockPos, _entity: &mut dyn Entity, _fall_distance: f64) {}
    /// `BlockBehaviour.onExplosionHit(state, level, pos, explosion, onHit)`: default = [`default_on_explosion_hit`].
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn crate::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(crate::world::item::ItemStack, BlockPos),
    ) {
        crate::world::level::explosion::default_on_explosion_hit(state, level, pos, explosion, on_hit);
    }
    /// `Block.wasExploded(level, pos, explosion)`.
    fn was_exploded(&self, _level: &mut ServerLevel, _pos: BlockPos, _explosion: &dyn crate::world::level::explosion::Explosion) {}
    /// `Block.dropFromExplosion(explosion)`.
    fn drop_from_explosion(&self, _explosion: &dyn crate::world::level::explosion::Explosion) -> bool {
        true
    }
    /// `BlockBehaviour.onProjectileHit(level, state, blockHit, projectile)`.
    fn on_projectile_hit(&self, _level: &mut ServerLevel, _state: BlockState, _hit: &mc_core::phys::BlockHitResult, _projectile: &mut dyn crate::world::entity::projectile::Projectile) {}
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
    /// `Block.playerWillDestroy`: destroy particles, `BLOCK_DESTROY` game event; returns the state to use (overrides call
    /// `server::level::block_break::player_will_destroy` as their `super`).
    fn player_will_destroy(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer) -> BlockState {
        crate::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    /// `Block.playerDestroy` (stats, exhaustion, drops); `block_entity` is the destroyed block's entity (a snapshot).
    fn player_destroy(
        &self,
        level: &mut ServerLevel,
        player: &mut crate::world::entity::ServerPlayer,
        pos: BlockPos,
        state: BlockState,
        block_entity: Option<&dyn crate::world::block_entity::BlockEntity>,
        destroyed_with: &crate::world::item::ItemStack,
    ) {
        crate::server::level::block_break::player_destroy(level, player, pos, state, block_entity, destroyed_with);
    }
    /// `Block.destroy` (called by `LevelChunk`-independent callers such as `ServerPlayerGameMode`).
    fn destroy(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState) {}
    /// `BlockBehaviour.spawnAfterBreak(state, level, pos, tool, dropExperience)` (experience from ores, spawners, ...).
    fn spawn_after_break(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _tool: &crate::world::item::ItemStack, _drop_experience: bool) {}
    /// `Block.handlePrecipitation`.
    fn handle_precipitation(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _precipitation: Precipitation) {}
    /// `block instanceof Portal portal`: the block's static `Portal` (nether/end portals and end gateways).
    fn as_portal(&self) -> Option<&'static dyn crate::world::entity::portal::Portal> {
        None
    }
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

    // ==== [Items] additions: `Fallable` (FallingBlock, BrushableBlock, SpeleothemBlock) ====
    /// `FallingBlock.falling(entity)`: configures the entity before it enters the level.
    fn falling(&self, _entity: &mut crate::world::entity::falling_block_entity::FallingBlockEntity) {}
    /// `Fallable.onLand(level, pos, state, replacedBlock, entity)`.
    fn on_land(
        &self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _state: BlockState,
        _replaced: BlockState,
        _entity: &crate::world::entity::falling_block_entity::FallingBlockEntity,
    ) {
    }
    /// `Fallable.onBrokenAfterFall(level, pos, entity)`.
    fn on_broken_after_fall(&self, _level: &mut ServerLevel, _pos: BlockPos, _entity: &crate::world::entity::falling_block_entity::FallingBlockEntity) {}
    /// `Fallable.getFallDamageSource(entity)`.
    fn get_fall_damage_source(&self, level: &ServerLevel, entity: &crate::world::entity::falling_block_entity::FallingBlockEntity) -> crate::world::damagesource::DamageSource {
        level.damage_sources().falling_block(entity)
    }

    // ============================================================ [Blocks] BonemealableBlock / liquid containers
    /// `this instanceof BonemealableBlock`.
    fn is_bonemealable(&self) -> bool {
        false
    }
    /// `BonemealableBlock.isValidBonemealTarget(level, pos, state, source)`.
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        false
    }
    /// `BonemealableBlock.isBonemealSuccess(level, random, pos, state, source)` (random = `level.random()`).
    fn is_bonemeal_success(&self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        true
    }
    /// `BonemealableBlock.performBonemeal(level, random, pos, state, source)` (random = `level.random()`).
    fn perform_bonemeal(&self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) {}
    /// `BonemealableBlock.getType()`.
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::Grower
    }
    /// `BonemealableBlock.getParticlePos(pos)`.
    fn get_bonemeal_particle_pos(&self, pos: BlockPos) -> BlockPos {
        match self.get_bonemeal_type() {
            BonemealType::NeighborSpreader => pos.above(),
            BonemealType::Grower => pos,
        }
    }
    /// `this instanceof LiquidBlockContainer` (`SimpleWaterloggedBlock`, `KelpBlock`, ...).
    fn is_liquid_block_container(&self) -> bool {
        false
    }
    /// `LiquidBlockContainer.canPlaceLiquid(user, level, pos, state, type)`.
    fn can_place_liquid(&self, _user: Option<&dyn Entity>, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState, _fluid: Fluid) -> bool {
        false
    }
    /// `LiquidBlockContainer.placeLiquid(level, pos, state, fluidState)`.
    fn place_liquid(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState, _fluid_state: mc_data::fluid::FluidState) -> bool {
        false
    }
    /// `this instanceof BucketPickup` (`LiquidBlock`, `SimpleWaterloggedBlock`, `PowderSnowBlock`).
    fn is_bucket_pickup(&self) -> bool {
        false
    }
    /// `BucketPickup.pickupBlock(user, level, pos, state)` (`ItemStack.EMPTY` when nothing was picked up).
    fn pickup_block(&self, _user: Option<&dyn Entity>, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState) -> crate::world::item::ItemStack {
        crate::world::item::ItemStack::empty()
    }
    /// `BucketPickup.getPickupSound()`.
    fn get_pickup_sound(&self) -> Option<mc_data::SoundEvent> {
        None
    }

    // ==== [Blocks:Crops] BonemealableBlock on a server level, SuspiciousEffectHolder ====
    /// `BonemealableBlock.isValidBonemealTarget(level, pos, state, source)` for implementors that need the server level
    /// (`level instanceof ServerLevel`, the feature registry, `level.getRandom()`: saplings, mushrooms, azalea). Callers use
    /// this entry point; the default delegates to [`BlockBehaviour::is_valid_bonemeal_target`].
    fn is_valid_bonemeal_target_server(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, source: BonemealSource) -> bool {
        self.is_valid_bonemeal_target(&*level, pos, state, source)
    }
    /// `SuspiciousEffectHolder.getSuspiciousEffects()` (`None` when the block is not a holder; flowers).
    fn get_suspicious_effects(&self) -> Option<mc_protocol::components::SuspiciousStewEffects> {
        None
    }
    /// `FlowerBlock.getBeeInteractionEffect()` (wither rose: wither, eyeblossom: poison; plain flowers `null`).
    fn get_bee_interaction_effect(&self) -> Option<crate::world::entity::effect::MobEffectInstance> {
        None
    }
    // ==== [Blocks:Dispensers] WorldlyContainerHolder ====
    /// `this instanceof WorldlyContainerHolder` (composter): hoppers use [`BlockBehaviour::get_worldly_container`].
    fn is_worldly_container_holder(&self) -> bool {
        false
    }
    /// `WorldlyContainerHolder.getContainer(state, level, pos)`: a transient container over the block; its effects on the
    /// world (vanilla does them inside `setItem`/`removeItem`/`setChanged`, which have the level there) are applied by
    /// [`BlockBehaviour::finish_worldly_container`] after the transfer that used it.
    fn get_worldly_container(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos) -> Option<Box<dyn crate::world::container::Container>> {
        None
    }
    /// Applies what the container returned by `get_worldly_container` deferred (composter fill / output emptying).
    fn finish_worldly_container(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _container: Box<dyn crate::world::container::Container>) {}


    // ==== [Blocks:SpeleothemCopper] AbstractCauldronBlock stalactite drips ====
    /// `AbstractCauldronBlock.canReceiveStalactiteDrip(fluid)`.
    fn can_receive_stalactite_drip(&self, _fluid: Fluid) -> bool {
        false
    }
    /// `AbstractCauldronBlock.receiveStalactiteDrip(state, level, pos, fluid)`.
    fn receive_stalactite_drip(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos, _fluid: Fluid) {}

    // ==== [Blocks:Pistons] ====
    /// The state whose loot table `BlockBehaviour.getDrops` uses: `state` itself except `MovingPistonBlock` (the drops
    /// of the block entity's moved state, nothing without a block entity). Consumed by `helpers::drop_resources`.
    fn get_drops_state(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> BlockState {
        state
    }
    // ==== [/Blocks:Pistons] ====
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
