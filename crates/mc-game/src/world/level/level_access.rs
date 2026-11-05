//! Level access traits mirroring vanilla's interfaces (`BlockGetter`, `LevelReader`, `LevelAccessor`) so block and
//! feature code runs against both `ServerLevel` and `WorldGenRegion`. This is the *core* every implementor must
//! provide; further vanilla default methods (`clip`, `getBlockStates`, `findBlocksIn`, `isWaterAt`, ...) are added here
//! as default methods by the slices that need them (ServerLevel/worldgen implement only the required ones).

use super::{LevelHeight, LightLayer};
use crate::world::block_entity::BlockEntity;
use crate::world::entity::EntityId;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::GameEvent;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::FeatureFlagSet;
use mc_world::ticks::TickPriority;

/// `BlockGetter` (+ `LevelHeightAccessor`).
pub trait BlockGetter {
    fn get_block_state(&self, pos: BlockPos) -> BlockState;
    fn get_fluid_state(&self, pos: BlockPos) -> FluidState;
    fn get_block_entity(&self, pos: BlockPos) -> Option<&dyn BlockEntity>;
    /// `LevelHeightAccessor` (`getMinY`/`getHeight`).
    fn level_height(&self) -> LevelHeight;

    fn get_min_y(&self) -> i32 {
        self.level_height().min_y
    }
    fn get_height(&self) -> i32 {
        self.level_height().height
    }
    fn get_max_y(&self) -> i32 {
        self.get_min_y() + self.get_height() - 1
    }
    fn is_outside_build_height(&self, y: i32) -> bool {
        self.level_height().is_outside_build_height(y)
    }
    fn get_light_emission(&self, pos: BlockPos) -> i32 {
        i32::from(self.get_block_state(pos).get_light_emission())
    }
}

/// `LevelReader` (the part independent of live entities).
pub trait LevelReader: BlockGetter {
    /// `hasChunk(chunkX, chunkZ)`: a full chunk is available.
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool;
    fn get_height_at(&self, ty: HeightmapType, x: i32, z: i32) -> i32;
    fn get_sea_level(&self) -> i32;
    /// `getBrightness(layer, pos)`.
    fn get_brightness(&self, layer: LightLayer, pos: BlockPos) -> i32;
    /// `getSkyDarken()`.
    fn get_sky_darken(&self) -> i32;
    /// `getRawBrightness(pos, skyDarkening)`.
    fn get_raw_brightness(&self, pos: BlockPos, sky_darkening: i32) -> i32;
    fn can_see_sky(&self, pos: BlockPos) -> bool;
    fn is_empty_block(&self, pos: BlockPos) -> bool {
        self.get_block_state(pos).is_air()
    }
    /// `isLoaded(pos)` (`hasChunkAt`): chunk of `pos` is loaded and `pos` is inside the world bounds.
    fn is_loaded(&self, pos: BlockPos) -> bool {
        !self.is_outside_build_height(pos.y()) && self.has_chunk(pos.x() >> 4, pos.z() >> 4)
    }
    /// `enabledFeatures()`.
    fn enabled_features(&self) -> FeatureFlagSet {
        FeatureFlagSet::DEFAULT_FLAGS
    }
}

/// `LevelAccessor` (+ `ScheduledTickAccess`): mutation entry points shared by `ServerLevel` and `WorldGenRegion`.
pub trait LevelAccessor: LevelReader {
    /// `this instanceof WorldGenLevel`: the generation view of this level when it is one (`WorldGenerationContext.of`,
    /// the biome lookup of block predicates). Implemented by the `WorldGenLevel` implementors.
    fn as_world_gen_level(&self) -> Option<&dyn crate::world::level::levelgen::WorldGenLevel> {
        None
    }
    /// `isUnobstructed(null, Shapes.block().move(pos))`: no entity blocks building in the block at `pos` (a
    /// `WorldGenRegion` has no entities).
    fn is_unobstructed_block(&self, _pos: BlockPos) -> bool {
        true
    }
    /// `setBlock(pos, state, flags, updateLimit)`; returns whether the block changed.
    fn set_block(&mut self, pos: BlockPos, state: BlockState, flags: u32, update_limit: i32) -> bool;
    fn remove_block(&mut self, pos: BlockPos, moved_by_piston: bool) -> bool;
    /// `destroyBlock(pos, dropBlock, breaker, updateLimit)` (breaker = entity id when known).
    fn destroy_block(&mut self, pos: BlockPos, drop_block: bool, breaker: Option<i32>, update_limit: i32) -> bool;
    fn get_game_time(&self) -> i64;
    fn next_sub_tick_count(&mut self) -> i64;
    fn random(&mut self) -> &mut dyn RandomSource;
    /// `ScheduledTickAccess.scheduleTick(pos, block, delay, priority)`.
    fn schedule_block_tick(&mut self, pos: BlockPos, block: Block, delay: i32, priority: TickPriority);
    fn schedule_fluid_tick(&mut self, pos: BlockPos, fluid: Fluid, delay: i32, priority: TickPriority);
    fn has_scheduled_block_tick(&self, pos: BlockPos, block: Block) -> bool;
    fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool;
    /// `updateNeighborsAt(pos, sourceBlock)`.
    fn update_neighbors_at(&mut self, pos: BlockPos, source_block: Block);
    /// `updateNeighborsAt(pos, sourceBlock, orientation)`.
    fn update_neighbors_at_with_orientation(&mut self, pos: BlockPos, source_block: Block, _orientation: Option<Orientation>) {
        self.update_neighbors_at(pos, source_block);
    }
    /// `neighborShapeChanged(direction, pos, neighborPos, neighborState, flags, limit)` (no-op for levels without
    /// neighbor updates, like world generation regions).
    fn neighbor_shape_changed(
        &mut self,
        _direction: Direction,
        _pos: BlockPos,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockState,
        _update_flags: u32,
        _update_limit: i32,
    ) {
    }
    /// `setBlock(pos, state, flags)` (`updateLimit` 512).
    fn set_block_flags(&mut self, pos: BlockPos, state: BlockState, flags: u32) -> bool {
        self.set_block(pos, state, flags, 512)
    }
    /// `setBlockAndUpdate(pos, state)`.
    fn set_block_and_update(&mut self, pos: BlockPos, state: BlockState) -> bool {
        self.set_block(pos, state, 3, 512)
    }
    /// `gameEvent(event, position, context)` (no listeners outside server levels).
    fn game_event(&mut self, _event: GameEvent, _position: Vec3, _context: &GameEventContext) {}
    /// `levelEvent(source, type, pos, data)` (no-op outside server levels).
    fn level_event(&mut self, _source: Option<EntityId>, _event_type: i32, _pos: BlockPos, _data: i32) {}
    /// `playSound(except, pos, sound, source, volume, pitch)` (no-op outside server levels; a world generation region
    /// never plays sounds).
    fn play_sound(&mut self, _except: Option<EntityId>, _pos: BlockPos, _sound: mc_data::SoundEvent, _source: mc_protocol::packets::play::level::SoundSource, _volume: f32, _pitch: f32) {}
    /// `ServerLevel.shouldTickBlocksAt(pos)` (`false` for anything that is not a server level).
    fn should_tick_blocks_at_pos(&self, _pos: BlockPos) -> bool {
        false
    }
}
