//! World mutation of `Level`/`ServerLevel`: `setBlock` and everything it triggers (neighbor and shape updates, client
//! notification, scheduled ticks), `destroyBlock`/`removeBlock`, and the `LevelAccessor` / `NeighborUpdateHost` /
//! `CollisionGetter` implementations of `ServerLevel`.

use super::ServerLevel;
use super::FullChunkStatus;
use crate::world::entity::EntityId;
use crate::world::level::block::helpers::{drop_resources, update_or_destroy_limited};
use crate::world::level::block::BlockStateExt;
use crate::world::level::flags::*;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::material::FluidStateExt as _;
use crate::world::level::redstone::{CollectingNeighborUpdater, NeighborUpdateHost, Orientation, SideBias};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::{Direction, Plane};
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::GameEvent;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_world::level_storage::FeatureFlagSet;
use mc_world::ticks::{LevelTickAccess, ScheduledTick, TickAccess, TickPriority};

impl ServerLevel {
    /// `isInValidBounds(pos)`.
    pub fn is_in_valid_bounds(&self, pos: BlockPos) -> bool {
        !self.is_outside_build_height(pos.y()) && ChunkPos::is_valid_xz(pos.x() >> 4, pos.z() >> 4)
    }

    /// `isInWorldBounds(pos)`.
    pub fn is_in_world_bounds(&self, pos: BlockPos) -> bool {
        !self.is_outside_build_height(pos.y()) && pos.x() >= -30_000_000 && pos.z() >= -30_000_000 && pos.x() < 30_000_000 && pos.z() < 30_000_000
    }

    /// Make sure the full chunk of `pos` is loaded (vanilla `getChunkAt(pos)` blocks until it is).
    pub fn ensure_chunk_loaded(&mut self, pos: BlockPos) {
        let (cx, cz) = (pos.x() >> 4, pos.z() >> 4);
        if self.chunk_now(cx, cz).is_none() {
            self.chunk_blocking_mut(cx, cz);
        }
    }

    /// `Level.setBlock(pos, state, flags, updateLimit)`.
    pub fn set_block_impl(&mut self, pos: BlockPos, block_state: BlockState, flags: u32, update_limit: i32) -> bool {
        if !self.is_in_valid_bounds(pos) {
            return false;
        }
        if self.is_debug {
            return false;
        }
        self.ensure_chunk_loaded(pos);
        let block = block_state.block();
        let Some(old_state) = self.chunk_set_block_state(pos, block_state, flags) else {
            return false;
        };
        let new_state = BlockGetter::get_block_state(self, pos);
        if new_state == block_state {
            // `setBlocksDirty` is a client-only hook.
            if flags & UPDATE_CLIENTS != 0 && self.full_chunk_status(pos.x() >> 4, pos.z() >> 4).is_or_after(FullChunkStatus::BlockTicking) {
                self.send_block_updated(pos, old_state, block_state, flags);
            }
            if flags & UPDATE_NEIGHBORS != 0 {
                self.update_neighbors_at(pos, old_state.block());
                if block_state.has_analog_output_signal() {
                    self.update_neighbour_for_output_signal(pos, block);
                }
            }
            if flags & UPDATE_KNOWN_SHAPE == 0 && update_limit > 0 {
                let neighbour_update_flags = flags & !34;
                old_state.update_indirect_neighbour_shapes(self, pos, neighbour_update_flags, update_limit - 1);
                block_state.update_neighbour_shapes(self, pos, neighbour_update_flags, update_limit - 1);
                block_state.update_indirect_neighbour_shapes(self, pos, neighbour_update_flags, update_limit - 1);
            }
            self.update_poi_on_block_state_change(pos, old_state, new_state);
        }
        true
    }

    /// `ServerLevel.sendBlockUpdated(pos, old, current, flags)`.
    pub fn send_block_updated(&mut self, pos: BlockPos, _old: BlockState, _current: BlockState, _flags: u32) {
        self.block_changed(pos);
        // Path type cache invalidation and navigation recomputation of mobs follow with the AI slice
        // (`pathTypesByPosCache`, `navigatingMobs`).
    }

    /// `updateNeighborsAt(pos, sourceBlock)`.
    pub fn update_neighbors_at_impl(&mut self, pos: BlockPos, source_block: Block) {
        let orientation = self.initial_orientation(None, None);
        self.update_neighbors_at_with_orientation_impl(pos, source_block, orientation);
    }

    /// `updateNeighborsAt(pos, sourceBlock, orientation)`.
    pub fn update_neighbors_at_with_orientation_impl(&mut self, pos: BlockPos, source_block: Block, orientation: Option<Orientation>) {
        CollectingNeighborUpdater::update_neighbors_at_except_from_facing(self, pos, source_block, None, orientation);
    }

    /// `updateNeighborsAtExceptFromFacing(pos, block, skipDirection, orientation)`.
    pub fn update_neighbors_at_except_from_facing(&mut self, pos: BlockPos, block: Block, skip_direction: Direction, orientation: Option<Orientation>) {
        CollectingNeighborUpdater::update_neighbors_at_except_from_facing(self, pos, block, Some(skip_direction), orientation);
    }

    /// `neighborChanged(pos, changedBlock, orientation)`.
    pub fn neighbor_changed(&mut self, pos: BlockPos, changed_block: Block, orientation: Option<Orientation>) {
        CollectingNeighborUpdater::neighbor_changed(self, pos, changed_block, orientation);
    }

    /// `neighborChanged(state, pos, changedBlock, orientation, movedByPiston)`.
    pub fn neighbor_changed_with_state(&mut self, state: BlockState, pos: BlockPos, changed_block: Block, orientation: Option<Orientation>, moved_by_piston: bool) {
        CollectingNeighborUpdater::neighbor_changed_with_state(self, state, pos, changed_block, orientation, moved_by_piston);
    }

    /// `Level.updateNeighbourForOutputSignal(pos, changedBlock)`.
    pub fn update_neighbour_for_output_signal(&mut self, pos: BlockPos, changed_block: Block) {
        for direction in Plane::Horizontal.faces().iter().copied() {
            let mut relative = pos.relative(direction);
            if self.is_loaded(relative) {
                let mut state = BlockGetter::get_block_state(self, relative);
                if state.block() == Block::COMPARATOR {
                    self.neighbor_changed_with_state(state, relative, changed_block, None, false);
                } else if state.is_redstone_conductor() {
                    relative = relative.relative(direction);
                    state = BlockGetter::get_block_state(self, relative);
                    if state.block() == Block::COMPARATOR {
                        self.neighbor_changed_with_state(state, relative, changed_block, None, false);
                    }
                }
            }
        }
    }

    /// `ServerLevel.updateNeighboursOnBlockSet(pos, oldState)`.
    pub fn update_neighbours_on_block_set(&mut self, pos: BlockPos, old_state: BlockState) {
        let block_state = BlockGetter::get_block_state(self, pos);
        let new_block = block_state.block();
        let block_changed = old_state.block() != new_block;
        if block_changed {
            old_state.affect_neighbors_after_removal(self, pos, false);
        }
        self.update_neighbors_at_impl(pos, new_block);
        if block_state.has_analog_output_signal() {
            self.update_neighbour_for_output_signal(pos, new_block);
        }
    }

    /// `ExperimentalRedstoneUtils.initialOrientation(level, front, up)`.
    pub fn initial_orientation(&mut self, front: Option<Direction>, up: Option<Direction>) -> Option<Orientation> {
        if !self.enabled_features().contains_all(FeatureFlagSet::REDSTONE_EXPERIMENTS) {
            return None;
        }
        let mut orientation = Orientation::random(&mut self.tick_state.random).with_side_bias(SideBias::Left);
        if let Some(up) = up {
            orientation = orientation.with_up(up);
        }
        if let Some(front) = front {
            orientation = orientation.with_front(front);
        }
        Some(orientation)
    }

    /// `Level.destroyBlock(pos, dropResources, breaker, updateLimit)`.
    pub fn destroy_block_impl(&mut self, pos: BlockPos, drop_block: bool, breaker: Option<EntityId>, update_limit: i32) -> bool {
        let block_state = BlockGetter::get_block_state(self, pos);
        if block_state.is_air() {
            return false;
        }
        // `Block.spawnDestroyParticles`: levelEvent 2001 with the state id.
        self.level_event_impl(None, 2001, pos, block_state.id() as i32);
        if drop_block {
            drop_resources(self, block_state, pos, breaker);
        }
        let fluid_state = self.get_fluid_state(pos);
        let destroyed = self.set_block_impl(pos, fluid_state.create_legacy_block(), UPDATE_ALL, update_limit);
        if destroyed {
            self.game_event_at_block(GameEvent::BLOCK_DESTROY, pos, &GameEventContext::of(breaker, Some(block_state)));
        }
        destroyed
    }

    /// `Level.removeBlock(pos, movedByPiston)`.
    pub fn remove_block_impl(&mut self, pos: BlockPos, moved_by_piston: bool) -> bool {
        let fluid_state = self.get_fluid_state(pos);
        self.set_block_impl(pos, fluid_state.create_legacy_block(), UPDATE_ALL | if moved_by_piston { UPDATE_MOVE_BY_PISTON } else { 0 }, 512)
    }

    /// `updatePOIOnBlockStateChange(pos, old, new)`: compares the point of interest types and tells the POI hook.
    pub fn update_poi_on_block_state_change(&mut self, pos: BlockPos, old_state: BlockState, new_state: BlockState) {
        let (old_type, new_type) = (poi_type_for_state(old_state), poi_type_for_state(new_state));
        if old_type != new_type
            && let Some(hook) = self.tick_state.poi_hook
        {
            hook(self, pos, old_type, new_type);
        }
    }

    /// `ScheduledTickAccess.scheduleTick(pos, block, delay)` (normal priority).
    pub fn schedule_tick_block(&mut self, pos: BlockPos, block: Block, delay: i32) {
        self.schedule_block_tick(pos, block, delay, TickPriority::Normal);
    }

    /// `ScheduledTickAccess.scheduleTick(pos, fluid, delay)` (normal priority).
    pub fn schedule_tick_fluid(&mut self, pos: BlockPos, fluid: Fluid, delay: i32) {
        self.schedule_fluid_tick(pos, fluid, delay, TickPriority::Normal);
    }

    /// `blockTicks.willTickThisTick(pos, block)`.
    pub fn will_tick_block_this_tick(&self, pos: BlockPos, block: Block) -> bool {
        self.tick_state.ticks.lock().block.will_tick_this_tick(&pos, block)
    }

    /// `fluidTicks.willTickThisTick(pos, fluid)`.
    pub fn will_tick_fluid_this_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.tick_state.ticks.lock().fluid.will_tick_this_tick(&pos, fluid)
    }

    fn create_tick<T>(&mut self, pos: BlockPos, ty: T, delay: i32, priority: TickPriority) -> ScheduledTick<T> {
        ScheduledTick { ty, pos, trigger_tick: self.game_time().wrapping_add(i64::from(delay)), priority, sub_tick_order: self.next_sub_tick() }
    }

    fn next_sub_tick(&mut self) -> i64 {
        let current = self.tick_state.sub_tick_count;
        self.tick_state.sub_tick_count = current.wrapping_add(1);
        current
    }
}

/// `PoiTypes.forState(state)`: index into `mc_data::misc::POI_TYPES` of the point of interest the state is, if any.
pub fn poi_type_for_state(state: BlockState) -> Option<usize> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_STATE: OnceLock<HashMap<u16, usize>> = OnceLock::new();
    let map = BY_STATE.get_or_init(|| {
        let mut map = HashMap::new();
        for (index, poi) in mc_data::misc::POI_TYPES.iter().enumerate() {
            for &id in poi.matching_states {
                map.entry(id).or_insert(index);
            }
        }
        map
    });
    map.get(&(state.id() as u16)).copied()
}

impl LevelAccessor for ServerLevel {
    fn is_unobstructed_block(&self, pos: BlockPos) -> bool {
        use crate::world::level::collision::CollisionGetter as _;
        self.is_unobstructed(None, &mc_core::phys::shapes::Shapes::block().move_vec3i(*pos))
    }

    fn set_block(&mut self, pos: BlockPos, state: BlockState, flags: u32, update_limit: i32) -> bool {
        self.set_block_impl(pos, state, flags, update_limit)
    }

    fn remove_block(&mut self, pos: BlockPos, moved_by_piston: bool) -> bool {
        self.remove_block_impl(pos, moved_by_piston)
    }

    fn destroy_block(&mut self, pos: BlockPos, drop_block: bool, breaker: Option<EntityId>, update_limit: i32) -> bool {
        self.destroy_block_impl(pos, drop_block, breaker, update_limit)
    }

    fn get_game_time(&self) -> i64 {
        self.game_time()
    }

    fn next_sub_tick_count(&mut self) -> i64 {
        self.next_sub_tick()
    }

    fn random(&mut self) -> &mut dyn RandomSource {
        &mut self.tick_state.random
    }

    fn schedule_block_tick(&mut self, pos: BlockPos, block: Block, delay: i32, priority: TickPriority) {
        let tick = self.create_tick(pos, block, delay, priority);
        self.tick_state.ticks.lock().block.schedule(tick);
    }

    fn schedule_fluid_tick(&mut self, pos: BlockPos, fluid: Fluid, delay: i32, priority: TickPriority) {
        let tick = self.create_tick(pos, fluid, delay, priority);
        self.tick_state.ticks.lock().fluid.schedule(tick);
    }

    fn has_scheduled_block_tick(&self, pos: BlockPos, block: Block) -> bool {
        self.tick_state.ticks.lock().block.has_scheduled_tick(&pos, block)
    }

    fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.tick_state.ticks.lock().fluid.has_scheduled_tick(&pos, fluid)
    }

    fn update_neighbors_at(&mut self, pos: BlockPos, source_block: Block) {
        self.update_neighbors_at_impl(pos, source_block);
    }

    fn update_neighbors_at_with_orientation(&mut self, pos: BlockPos, source_block: Block, orientation: Option<Orientation>) {
        self.update_neighbors_at_with_orientation_impl(pos, source_block, orientation);
    }

    fn neighbor_shape_changed(&mut self, direction: Direction, pos: BlockPos, neighbor_pos: BlockPos, neighbor_state: BlockState, update_flags: u32, update_limit: i32) {
        CollectingNeighborUpdater::shape_update(self, direction, neighbor_state, pos, neighbor_pos, update_flags, update_limit);
    }

    fn game_event(&mut self, event: GameEvent, position: Vec3, context: &GameEventContext) {
        self.post_game_event(event, position, context);
    }

    fn level_event(&mut self, source: Option<EntityId>, event_type: i32, pos: BlockPos, data: i32) {
        self.level_event_impl(source, event_type, pos, data);
    }

    fn play_sound(&mut self, except: Option<EntityId>, pos: BlockPos, sound: mc_data::SoundEvent, source: mc_protocol::packets::play::level::SoundSource, volume: f32, pitch: f32) {
        ServerLevel::play_sound(self, except, pos, sound, source, volume, pitch);
    }

    fn should_tick_blocks_at_pos(&self, pos: BlockPos) -> bool {
        ServerLevel::should_tick_blocks_at_pos(self, pos)
    }

    fn push_entities_up(&mut self, state: BlockState, new_state: BlockState, pos: BlockPos) {
        crate::world::level::block::helpers::push_entities_up(self, state, new_state, pos);
    }
}

impl NeighborUpdateHost for ServerLevel {
    fn neighbor_updater(&mut self) -> &mut CollectingNeighborUpdater {
        &mut self.tick_state.neighbor_updater
    }

    fn host_block_state(&self, pos: BlockPos) -> BlockState {
        BlockGetter::get_block_state(self, pos)
    }

    fn execute_shape_update(&mut self, direction: Direction, pos: BlockPos, neighbor_pos: BlockPos, neighbor_state: BlockState, update_flags: u32, update_limit: i32) {
        // `NeighborUpdater.executeShapeUpdate`
        let current_state = BlockGetter::get_block_state(self, pos);
        if update_flags & UPDATE_SKIP_SHAPE_UPDATE_ON_WIRE == 0 || current_state.block() != Block::REDSTONE_WIRE {
            let new_state = current_state.update_shape(self, pos, direction, neighbor_pos, neighbor_state);
            update_or_destroy_limited(current_state, new_state, self, pos, update_flags, update_limit);
        }
    }

    fn execute_update(&mut self, state: BlockState, pos: BlockPos, changed_block: Block, orientation: Option<Orientation>, moved_by_piston: bool) {
        state.handle_neighbor_changed(self, pos, changed_block, orientation, moved_by_piston);
    }

    fn redstone_experiments(&self) -> bool {
        self.enabled_features().contains_all(FeatureFlagSet::REDSTONE_EXPERIMENTS)
    }

    fn initial_orientation(&mut self, front: Option<Direction>, up: Option<Direction>) -> Option<Orientation> {
        ServerLevel::initial_orientation(self, front, up)
    }
}
