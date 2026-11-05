//! `LevelChunk` behaviour that needs the level: `setBlockState` with all its side effects, block entity lifecycle,
//! tickers, game event listener registration, post-load registration and generation post-processing.
//!
//! The chunk lives inside the level's chunk map, so these are `ServerLevel` methods addressed by block position; each
//! step re-borrows the chunk (block behaviour code run in between may touch any chunk).

use super::ServerLevel;
use crate::world::block_entity::{BlockEntity, create_block_entity, new_block_entity};
use crate::world::level::block::BlockStateExt;
use crate::world::level::chunk::{LevelChunk, TickerCell};
use crate::world::level::flags::*;
use crate::world::level::gameevent::{ListenerOwner, RegisteredListener};
use crate::world::level::material::FluidStateExt;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_nbt::CompoundTag;
use mc_world::chunk::unpack_offset_coordinates;
use mc_world::ticks::TickPriority;
use mc_world::upgrade_data::UpgradeChunk;
use std::rc::Rc;

/// `LevelChunk.EntityCreationType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityCreationType {
    Immediate,
    Queued,
    Check,
}

fn chunk_of(pos: BlockPos) -> (i32, i32) {
    (pos.x() >> 4, pos.z() >> 4)
}

fn is_rail_block(block: Block) -> bool {
    block.data().classes.contains(&"BaseRailBlock")
}

impl ServerLevel {
    fn chunk_at(&self, pos: BlockPos) -> Option<&LevelChunk> {
        let (cx, cz) = chunk_of(pos);
        self.chunk_now(cx, cz)
    }

    fn chunk_at_mut(&mut self, pos: BlockPos) -> Option<&mut LevelChunk> {
        let (cx, cz) = chunk_of(pos);
        self.chunk_now_mut(cx, cz)
    }

    /// `LevelChunk.setBlockState(pos, state, flags)`: `None` is vanilla's `null` (nothing changed / aborted).
    pub(super) fn chunk_set_block_state(&mut self, pos: BlockPos, state: BlockState, flags: u32) -> Option<BlockState> {
        let raw = self.chunk_at_mut(pos)?.data.set_block_state_raw(pos, state)?;
        let old_state = raw.old_state;
        let new_block = state.block();
        if raw.section_was_empty != raw.section_is_empty {
            self.light_engine_mut().update_section_status(SectionPos::of_block_pos(&pos), raw.section_is_empty);
        }
        if raw.light_properties_changed {
            self.light_engine_mut().check_block(pos);
        }

        let block_changed = old_state.block() != new_block;
        let moved_by_piston = flags & UPDATE_MOVE_BY_PISTON != 0;
        let side_effects = flags & UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS == 0;
        if block_changed && old_state.has_block_entity() && !state.should_changed_state_keep_block_entity(old_state) {
            if let Some(taken) = self.tick_state.taken_block_entities.iter_mut().rev().find(|t| t.pos == pos) {
                // the block entity is out of its chunk running its own code: it is dropped when that code returns
                taken.removed = true;
            } else if side_effects && let Some(mut block_entity) = self.take_block_entity(pos) {
                block_entity.pre_remove_side_effects(self, pos, old_state);
                self.put_back_block_entity(pos, block_entity);
            }
            self.remove_block_entity_at(pos);
        }

        if (block_changed || is_rail_block(new_block)) && (flags & UPDATE_NEIGHBORS != 0 || moved_by_piston) {
            old_state.affect_neighbors_after_removal(self, pos, moved_by_piston);
        }

        if BlockGetter::get_block_state(self, pos).block() != new_block {
            return None;
        }
        if flags & UPDATE_SKIP_ON_PLACE == 0 {
            state.on_place(self, pos, old_state, moved_by_piston);
        }
        if let Some(taken) = self.tick_state.taken_block_entities.iter_mut().rev().find(|t| t.pos == pos) {
            // `setBlock` from inside the block entity's own code (brewing stand `HAS_BOTTLE`, furnace `LIT`, ...): the live
            // block entity keeps living, it just learns its new state when it is put back
            if !taken.removed && state.has_block_entity() {
                taken.new_state = Some(state);
            }
        } else if state.has_block_entity() && BlockGetter::get_block_state(self, pos).block() == new_block {
            let mut present = self.get_block_entity_creating(pos, EntityCreationType::Check);
            if present {
                let valid = self.get_block_entity(pos).is_some_and(|be| be.is_valid_block_state(state));
                if !valid {
                    if let Some(be) = self.get_block_entity(pos) {
                        log::warn!("Found mismatched block entity @ {}: type = {}, state = {}", pos.to_short_string(), be.get_type().name(), state);
                    }
                    self.remove_block_entity_at(pos);
                    present = false;
                }
            }
            if !present {
                if let Some(block_entity) = new_block_entity(pos, state) {
                    self.add_and_register_block_entity(block_entity);
                }
            } else {
                if let Some(chunk) = self.chunk_at_mut(pos)
                    && let Some(be) = chunk.block_entities.get_mut(&pos)
                {
                    be.set_block_state(state);
                }
                self.update_block_entity_ticker(pos);
            }
        }
        if let Some(chunk) = self.chunk_at_mut(pos) {
            chunk.data.access.mark_unsaved();
        }
        Some(old_state)
    }

    // ------------------------------------------------------------------------------------------- block entities

    /// Take the block entity at `pos` out of its chunk (pair with `put_back_block_entity`).
    pub fn take_block_entity(&mut self, pos: BlockPos) -> Option<Box<dyn BlockEntity>> {
        self.chunk_at_mut(pos)?.block_entities.remove(&pos)
    }

    /// Put a taken block entity back unless it was replaced or removed (`setRemoved`) in the meantime.
    pub fn put_back_block_entity(&mut self, pos: BlockPos, block_entity: Box<dyn BlockEntity>) {
        if block_entity.is_removed() {
            return;
        }
        if let Some(chunk) = self.chunk_at_mut(pos) {
            chunk.block_entities.entry(pos).or_insert(block_entity);
        }
    }

    /// Run `f` with the block entity at `pos` taken out (it can use the level freely), then put it back. `setBlock` on `pos`
    /// from inside `f` keeps this block entity (see `TakenBlockEntity`).
    pub fn with_block_entity<R>(&mut self, pos: BlockPos, f: impl FnOnce(&mut ServerLevel, &mut dyn BlockEntity) -> R) -> Option<R> {
        let mut block_entity = self.take_block_entity(pos)?;
        self.tick_state.taken_block_entities.push(super::level_tick_state::TakenBlockEntity { pos, new_state: None, removed: false });
        let result = f(self, block_entity.as_mut());
        if let Some(taken) = self.tick_state.taken_block_entities.pop() {
            if taken.removed {
                block_entity.set_removed();
            } else {
                if let Some(state) = taken.new_state {
                    block_entity.set_block_state(state);
                }
                self.put_back_block_entity(pos, block_entity);
            }
        }
        Some(result)
    }

    /// `LevelChunk.getBlockEntity(pos, creationType)` (which may promote pending NBT or create the entity): whether a
    /// live block entity exists at `pos` afterwards (read it through `get_block_entity`).
    pub fn get_block_entity_creating(&mut self, pos: BlockPos, creation: EntityCreationType) -> bool {
        let Some(chunk) = self.chunk_at_mut(pos) else { return false };
        if !chunk.block_entities.contains_key(&pos) {
            if let Some(tag) = chunk.data.access.block_entities.remove(&pos)
                && self.promote_pending_block_entity(pos, tag)
            {
                return true;
            }
            if creation == EntityCreationType::Immediate {
                let state = BlockGetter::get_block_state(self, pos);
                if state.has_block_entity()
                    && let Some(block_entity) = new_block_entity(pos, state)
                {
                    self.add_and_register_block_entity(block_entity);
                    return true;
                }
            }
            return false;
        }
        let chunk = self.chunk_at_mut(pos).expect("chunk checked above");
        if chunk.block_entities.get(&pos).is_some_and(|be| be.is_removed()) {
            chunk.block_entities.remove(&pos);
            return false;
        }
        true
    }

    /// `Level.getBlockEntity(pos)` with `EntityCreationType.IMMEDIATE`.
    pub fn get_block_entity_immediate(&mut self, pos: BlockPos) -> Option<&mut dyn BlockEntity> {
        if !self.is_in_valid_bounds(pos) || !self.get_block_entity_creating(pos, EntityCreationType::Immediate) {
            return None;
        }
        self.chunk_at_mut(pos)?.block_entities.get_mut(&pos).map(|b| &mut **b)
    }

    /// `LevelChunk.promotePendingBlockEntity`: the pending NBT (or `DUMMY` stub) becomes a live block entity. Types
    /// without an implementation keep their raw NBT (returns `false` and leaves the tag pending) so nothing is lost on
    /// save.
    fn promote_pending_block_entity(&mut self, pos: BlockPos, tag: CompoundTag) -> bool {
        let state = BlockGetter::get_block_state(self, pos);
        let id = tag.get_string_or("id", "");
        let block_entity: Option<Box<dyn BlockEntity>> = if id == "DUMMY" {
            if state.has_block_entity() {
                let created = new_block_entity(pos, state);
                if created.is_none() {
                    // no implementation for this block's type yet: keep the stub (like the typed branch below), not a failure
                    if let Some(chunk) = self.chunk_at_mut(pos) {
                        chunk.data.access.block_entities.insert(pos, tag);
                    }
                    return false;
                }
                created
            } else {
                log::warn!("Tried to load a DUMMY block entity @ {} but found not block entity block {} at location", pos.to_short_string(), state);
                None
            }
        } else {
            match BlockEntityType::by_name(&id) {
                None => {
                    log::error!("Skipping block entity with invalid type: {}", tag.get("id").map(|t| t.to_string()).unwrap_or_default());
                    None
                }
                Some(ty) => match create_block_entity(ty, pos, state) {
                    Some(mut be) => {
                        be.load_additional(&tag);
                        Some(be)
                    }
                    None => {
                        // no implementation for this type yet: keep the data
                        if let Some(chunk) = self.chunk_at_mut(pos) {
                            chunk.data.access.block_entities.insert(pos, tag);
                        }
                        return false;
                    }
                },
            }
        };
        match block_entity {
            Some(be) => {
                self.add_and_register_block_entity(be);
                true
            }
            None => {
                log::warn!("Tried to load a block entity for block {} but failed at location {}", state, pos.to_short_string());
                false
            }
        }
    }

    /// `Level.setBlockEntity(blockEntity)` / `LevelChunk.addAndRegisterBlockEntity`.
    pub fn add_and_register_block_entity(&mut self, block_entity: Box<dyn BlockEntity>) {
        let pos = block_entity.get_block_pos();
        if !self.is_in_valid_bounds(pos) {
            return;
        }
        if !self.set_block_entity_in_chunk(block_entity) {
            return;
        }
        let in_level = self.chunk_at(pos).is_some_and(|c| c.data.is_loaded());
        if in_level {
            self.add_game_event_listener(pos);
            self.update_block_entity_ticker(pos);
        }
    }

    /// `LevelChunk.setBlockEntity(blockEntity)` (validity checks, replaces a previous entry).
    fn set_block_entity_in_chunk(&mut self, mut block_entity: Box<dyn BlockEntity>) -> bool {
        let pos = block_entity.get_block_pos();
        let block_state = BlockGetter::get_block_state(self, pos);
        if !block_state.has_block_entity() {
            log::warn!("Trying to set block entity {} at position {}, but state {} does not allow it", block_entity.get_type().name(), pos.to_short_string(), block_state);
            return false;
        }
        let cached = block_entity.get_block_state();
        if block_state != cached {
            if !block_entity.is_valid_block_state(block_state) {
                log::warn!("Trying to set block entity {} at position {}, but state {} does not allow it", block_entity.get_type().name(), pos.to_short_string(), block_state);
                return false;
            }
            if block_state.block() != cached.block() {
                log::warn!("Block state mismatch on block entity {} in position {}, {} != {}, updating", block_entity.get_type().name(), pos.to_short_string(), block_state, cached);
            }
            block_entity.set_block_state(block_state);
        }
        block_entity.clear_removed();
        let Some(chunk) = self.chunk_at_mut(pos) else { return false };
        let previous = chunk.block_entities.insert(pos, block_entity);
        if let Some(mut previous) = previous {
            previous.set_removed_in_level(self);
        }
        true
    }

    /// `Level.removeBlockEntity(pos)`.
    pub fn remove_block_entity(&mut self, pos: BlockPos) {
        if self.is_in_valid_bounds(pos) {
            self.remove_block_entity_at(pos);
        }
    }

    /// `LevelChunk.removeBlockEntity(pos)`.
    pub(super) fn remove_block_entity_at(&mut self, pos: BlockPos) {
        let in_level = self.chunk_at(pos).is_some_and(|c| c.data.is_loaded());
        if in_level {
            self.remove_game_event_listener(pos);
            let removed = self.chunk_at_mut(pos).and_then(|chunk| chunk.block_entities.remove(&pos));
            if let Some(mut removed) = removed {
                removed.set_removed_in_level(self);
            }
        }
        self.remove_block_entity_ticker(pos);
    }

    /// `Level.blockEntityChanged(pos)`.
    pub fn block_entity_changed(&mut self, pos: BlockPos) {
        if self.is_loaded(pos)
            && let Some(chunk) = self.chunk_at_mut(pos)
        {
            chunk.data.access.mark_unsaved();
        }
    }

    // ----------------------------------------------------------------------------------------- tickers / listeners

    /// `LevelChunk.updateBlockEntityTicker(blockEntity)`.
    pub(super) fn update_block_entity_ticker(&mut self, pos: BlockPos) {
        let ticks = self.chunk_at(pos).and_then(|c| c.block_entities.get(&pos)).is_some_and(|be| be.is_ticking());
        if !ticks {
            self.remove_block_entity_ticker(pos);
            return;
        }
        let in_level = self.chunk_at(pos).is_some_and(|c| c.data.is_loaded());
        let Some(chunk) = self.chunk_at_mut(pos) else { return };
        if let Some(existing) = chunk.tickers_in_level.get(&pos) {
            existing.0.set(Some(pos));
        } else if in_level {
            let cell = Rc::new(TickerCell(std::cell::Cell::new(Some(pos))));
            chunk.tickers_in_level.insert(pos, cell.clone());
            // `Level.addBlockEntityTicker`
            if self.tick_state.ticking_block_entities {
                self.tick_state.pending_block_entity_tickers.push(cell);
            } else {
                self.tick_state.block_entity_tickers.push(cell);
            }
        }
    }

    /// `LevelChunk.removeBlockEntityTicker(pos)` (rebinds to `NULL_TICKER`).
    fn remove_block_entity_ticker(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_at_mut(pos)
            && let Some(cell) = chunk.tickers_in_level.remove(&pos)
        {
            cell.0.set(None);
        }
    }

    fn game_event_registration(&mut self, pos: BlockPos) -> Option<RegisteredListener> {
        let chunk = self.chunk_at(pos)?;
        let listener = chunk.block_entities.get(&pos)?.game_event_listener()?;
        Some(RegisteredListener { owner: ListenerOwner::BlockEntity(pos), source: listener.source, radius: listener.radius, delivery: listener.delivery })
    }

    /// `LevelChunk.addGameEventListener(blockEntity, level)`.
    fn add_game_event_listener(&mut self, pos: BlockPos) {
        if let Some(registration) = self.game_event_registration(pos) {
            let section_y = pos.y() >> 4;
            if let Some(chunk) = self.chunk_at_mut(pos) {
                chunk.game_event_registries.entry(section_y).or_default().register(registration);
            }
        }
    }

    /// `LevelChunk.removeGameEventListener(blockEntity, level)`.
    fn remove_game_event_listener(&mut self, pos: BlockPos) {
        let section_y = pos.y() >> 4;
        if let Some(chunk) = self.chunk_at_mut(pos)
            && let Some(registry) = chunk.game_event_registries.get_mut(&section_y)
            && registry.unregister(ListenerOwner::BlockEntity(pos))
        {
            chunk.game_event_registries.remove(&section_y);
        }
    }

    // ------------------------------------------------------------------------------------------ chunk lifecycle

    /// `LevelChunk.registerAllBlockEntitiesAfterLevelLoad()`.
    pub fn register_all_block_entities_after_level_load(&mut self, pos: ChunkPos) {
        let Some(chunk) = self.chunk_now(pos.x, pos.z) else { return };
        let positions: Vec<BlockPos> = chunk.block_entities.keys().copied().collect();
        for p in positions {
            self.add_game_event_listener(p);
            self.update_block_entity_ticker(p);
        }
    }

    /// `registerTickContainerInLevel(level)`: hands the chunk's scheduled ticks to the level.
    pub fn register_tick_container_in_level(&mut self, pos: ChunkPos) {
        let Some(chunk) = self.chunk_now_mut(pos.x, pos.z) else { return };
        let block = std::mem::take(&mut chunk.data.block_ticks);
        let fluid = std::mem::take(&mut chunk.data.fluid_ticks);
        let mut ticks = self.tick_state.ticks.lock();
        ticks.block.add_container(pos, block);
        ticks.fluid.add_container(pos, fluid);
    }

    /// `ServerLevel.startTickingChunk(chunk)`: `unpackTicks(gameTime)` (the container is detached while its saved ticks
    /// become scheduled ones so the level re-reads the next trigger tick).
    pub fn start_ticking_chunk(&mut self, pos: ChunkPos) {
        let game_time = self.game_time();
        let mut ticks = self.tick_state.ticks.lock();
        if let Some(mut block) = ticks.block.remove_container(pos) {
            block.unpack(game_time);
            ticks.block.add_container(pos, block);
        }
        if let Some(mut fluid) = ticks.fluid.remove_container(pos) {
            fluid.unpack(game_time);
            ticks.fluid.add_container(pos, fluid);
        }
    }

    /// `LevelChunk.clearAllBlockEntities()`.
    pub fn clear_all_block_entities(&mut self, chunk: &mut LevelChunk) {
        for block_entity in chunk.block_entities.values_mut() {
            block_entity.set_removed();
        }
        chunk.block_entities.clear();
        for ticker in chunk.tickers_in_level.values() {
            ticker.0.set(None);
        }
        chunk.tickers_in_level.clear();
    }

    /// `unregisterTickContainerFromLevel(level)`: the scheduled ticks go back into the chunk.
    pub fn unregister_tick_container_from_level(chunk: &mut LevelChunk, ticks: &super::level_tick_state::SharedTickContainers) {
        let pos = chunk.pos();
        let mut ticks = ticks.lock();
        if let Some(block) = ticks.block.remove_container(pos) {
            chunk.data.block_ticks = block;
        }
        if let Some(fluid) = ticks.fluid.remove_container(pos) {
            chunk.data.fluid_ticks = fluid;
        }
    }

    /// `LevelChunk.postProcessGeneration(level)`.
    pub fn post_process_generation(&mut self, pos: ChunkPos) {
        let Some(chunk) = self.chunk_now_mut(pos.x, pos.z) else { return };
        let min_section_y = chunk.data.access.level_height.min_section_y();
        let mut lists: Vec<(usize, Vec<i16>)> = Vec::new();
        for (index, list) in chunk.data.access.post_processing.iter_mut().enumerate() {
            if let Some(list) = list {
                lists.push((index, std::mem::take(list)));
            }
        }
        for (index, offsets) in lists {
            for packed in offsets {
                let block_pos = unpack_offset_coordinates(packed, min_section_y + index as i32, pos);
                let block_state = BlockGetter::get_block_state(self, block_pos);
                let fluid_state = block_state.get_fluid_state();
                if !fluid_state.is_empty() {
                    fluid_state.tick(self, block_pos, block_state);
                }
                if block_state.block().data().classes.contains(&"LiquidBlock") {
                    block_state.tick(self, block_pos);
                } else {
                    let new_state = crate::world::level::block::helpers::update_from_neighbour_shapes(block_state, self, block_pos);
                    if new_state != block_state {
                        self.set_block_impl(block_pos, new_state, 276, 512);
                    }
                }
            }
        }
        let pending: Vec<BlockPos> = self.chunk_now(pos.x, pos.z).map(|c| c.data.access.block_entities.keys().copied().collect()).unwrap_or_default();
        for p in pending {
            self.get_block_entity_creating(p, EntityCreationType::Check);
        }
        // Types without an implementation stay pending (`pendingBlockEntities.clear()` would lose their data).
        let mut upgrade = match self.chunk_now_mut(pos.x, pos.z) {
            Some(chunk) => std::mem::replace(&mut chunk.data.access.upgrade_data, mc_world::upgrade_data::UpgradeData::empty()),
            None => return,
        };
        let result = upgrade.upgrade(&mut ChunkUpgrade { level: self, pos });
        if let Err(e) = result {
            log::error!("Failed to upgrade chunk {pos}: {e}");
        }
        if let Some(chunk) = self.chunk_now_mut(pos.x, pos.z) {
            chunk.data.access.upgrade_data = upgrade;
        }
    }
}

/// `UpgradeChunk` view of a chunk + its level for `UpgradeData.upgrade`.
struct ChunkUpgrade<'a> {
    level: &'a mut ServerLevel,
    pos: ChunkPos,
}

impl UpgradeChunk for ChunkUpgrade<'_> {
    fn get_pos(&self) -> ChunkPos {
        self.pos
    }
    fn get_min_y(&self) -> i32 {
        self.level.get_min_y()
    }
    fn get_max_y(&self) -> i32 {
        self.level.get_max_y()
    }
    fn get_section_state(&self, section_index: usize, index: i32) -> Option<BlockState> {
        if !(0..4096).contains(&index) {
            return None;
        }
        let chunk = self.level.chunk_now(self.pos.x, self.pos.z)?;
        let section = chunk.data.access.sections.get(section_index)?;
        Some(section.get_block_state(index & 15, index >> 8 & 15, index >> 4 & 15))
    }
    fn get_block_state(&self, pos: &BlockPos) -> BlockState {
        BlockGetter::get_block_state(&*self.level, *pos)
    }
    fn get_fluid_type(&self, pos: &BlockPos) -> Fluid {
        BlockGetter::get_fluid_state(&*self.level, *pos).fluid()
    }
    fn set_block(&mut self, pos: &BlockPos, state: BlockState, flags: i32, update_limit: i32) -> bool {
        self.level.set_block_impl(*pos, state, flags as u32, update_limit)
    }
    fn destroy_block(&mut self, pos: &BlockPos, drop_resources: bool, update_limit: i32) -> bool {
        self.level.destroy_block_impl(*pos, drop_resources, None, update_limit)
    }
    fn update_shape(&mut self, state: BlockState, direction: Direction, pos: &BlockPos, neighbour_pos: &BlockPos, neighbour_state: BlockState) -> BlockState {
        state.update_shape(&mut *self.level, *pos, direction, *neighbour_pos, neighbour_state)
    }
    fn swap_chest_contents(&mut self, _a: &BlockPos, _b: &BlockPos) {
        // `ChestBlockEntity.swapContents`: handled by the chest block entity slice (no chest implementation registered).
    }
    fn schedule_block_tick(&mut self, pos: &BlockPos, block: Block, delay: i32, priority: TickPriority) {
        LevelAccessor::schedule_block_tick(&mut *self.level, *pos, block, delay, priority);
    }
    fn schedule_fluid_tick(&mut self, pos: &BlockPos, fluid: Fluid, delay: i32, priority: TickPriority) {
        LevelAccessor::schedule_fluid_tick(&mut *self.level, *pos, fluid, delay, priority);
    }
}


/// Bring the chunk's scheduled ticks back from the level's containers (for saving); `true` if they were registered.
pub fn take_ticks_back(chunk: &mut LevelChunk, ticks: &super::level_tick_state::SharedTickContainers) -> bool {
    let pos = chunk.pos();
    let mut ticks = ticks.lock();
    let block = ticks.block.remove_container(pos);
    let fluid = ticks.fluid.remove_container(pos);
    let registered = block.is_some() || fluid.is_some();
    if let Some(block) = block {
        chunk.data.block_ticks = block;
    }
    if let Some(fluid) = fluid {
        chunk.data.fluid_ticks = fluid;
    }
    registered
}

/// Hand the chunk's scheduled ticks to the level again after [`take_ticks_back`].
pub fn give_ticks(chunk: &mut LevelChunk, ticks: &super::level_tick_state::SharedTickContainers) {
    let pos = chunk.pos();
    let mut ticks = ticks.lock();
    ticks.block.add_container(pos, std::mem::take(&mut chunk.data.block_ticks));
    ticks.fluid.add_container(pos, std::mem::take(&mut chunk.data.fluid_ticks));
}
