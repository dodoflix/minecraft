//! `net.minecraft.world.level.block.PistonBaseBlock` (`piston` package; `minecraft:piston` and `minecraft:sticky_piston`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::{BlockPlaceContext, ItemStack};
use crate::world::level::block::piston::{PistonStructureResolver, push_reaction};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter::has_signal;
use mc_data::GameEvent;
use mc_data::SoundEvent;
use mc_data::enums::{PistonType, PushReaction};
use mc_protocol::packets::play::level::SoundSource;
use crate::world::block_entity::PistonMovingBlockEntity;
use mc_core::random::RandomSource;

pub use crate::world::level::block::piston::is_pushable;

/// `PistonBaseBlock.TRIGGER_*`.
pub const TRIGGER_EXTEND: i32 = 0;
pub const TRIGGER_CONTRACT: i32 = 1;
pub const TRIGGER_DROP: i32 = 2;

pub struct PistonBaseBlock {
    pub is_sticky: bool,
}

/// `getNeighborSignal(level, pos, pushDirection)`: the piston is powered by any side but the front, by itself from
/// below, or "quasi-connectively" by the block above (`above.relative(d)` for every `d != DOWN`).
fn get_neighbor_signal(level: &ServerLevel, pos: BlockPos, push_direction: Direction) -> bool {
    for direction in Direction::ALL {
        if direction != push_direction && has_signal(level, pos.relative(direction), direction) {
            return true;
        }
    }
    if has_signal(level, pos, Direction::Down) {
        return true;
    }
    let above = pos.above();
    for direction in Direction::ALL {
        if direction != Direction::Down && has_signal(level, above.relative(direction), direction) {
            return true;
        }
    }
    false
}

impl PistonBaseBlock {
    fn check_if_extend(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        let direction = state.get(props::FACING);
        let extend = get_neighbor_signal(level, pos, direction);
        if extend && !state.get(props::EXTENDED) {
            if PistonStructureResolver::new(level, pos, direction, true).resolve() {
                level.block_event(pos, state.block(), TRIGGER_EXTEND, direction.get_3d_data_value() as i32);
            }
        } else if !extend && state.get(props::EXTENDED) {
            let pushed_pos = pos.relative_n(direction, 2);
            let pushed_state = level.get_block_state(pushed_pos);
            let mut event = TRIGGER_CONTRACT;
            if pushed_state.is_block(Block::MOVING_PISTON)
                && pushed_state.get(props::FACING) == direction
                && let Some(piston) = level.moving_piston_info(pushed_pos)
                && piston.extending
                && (piston.progress < 0.5 || level.game_time() == piston.last_ticked || level.tick_state.handling_tick)
            {
                event = TRIGGER_DROP;
            }
            level.block_event(pos, state.block(), event, direction.get_3d_data_value() as i32);
        }
    }

    /// `moveBlocks(level, pistonPos, direction, extending)`.
    fn move_blocks(&self, level: &mut ServerLevel, piston_pos: BlockPos, direction: Direction, extending: bool) -> bool {
        let arm_pos = piston_pos.relative(direction);
        if !extending && level.get_block_state(arm_pos).is_block(Block::PISTON_HEAD) {
            level.set_block_flags(arm_pos, Block::AIR.default_state(), 276);
        }

        let mut resolver = PistonStructureResolver::new(level, piston_pos, direction, extending);
        if !resolver.resolve() {
            return false;
        }
        let to_push: Vec<BlockPos> = resolver.get_to_push().to_vec();
        let to_destroy: Vec<BlockPos> = resolver.get_to_destroy().to_vec();
        let resolver_push_direction = resolver.get_push_direction();

        // `Maps.newHashMap()`: iterated below, so keep Java's table order
        let mut delete_after_move = JavaPosMap::default();
        let mut to_push_shapes: Vec<BlockState> = Vec::with_capacity(to_push.len());
        for pos in &to_push {
            let state = level.get_block_state(*pos);
            to_push_shapes.push(state);
            delete_after_move.put(*pos, state);
        }

        let mut to_update: Vec<BlockState> = Vec::with_capacity(to_push.len() + to_destroy.len());
        let push_direction = if extending { direction } else { direction.get_opposite() };

        for pos in to_destroy.iter().rev() {
            let state = level.get_block_state(*pos);
            helpers::drop_resources(level, state, *pos, None);
            level.set_block_flags(*pos, Block::AIR.default_state(), 18);
            level.game_event_at_block(GameEvent::BLOCK_DESTROY, *pos, &GameEventContext::of_state(Some(state)));
            to_update.push(state);
        }

        for (i, pos) in to_push.iter().enumerate().rev() {
            let block_state = level.get_block_state(*pos);
            let pos = pos.relative(push_direction);
            delete_after_move.remove(pos);
            let actual_state = Block::MOVING_PISTON.default_state().set(props::FACING, direction);
            level.set_block_flags(pos, actual_state, 324);
            level.add_and_register_block_entity(Box::new(PistonMovingBlockEntity::new_moving(pos, actual_state, to_push_shapes[i], direction, extending, false)));
            to_update.push(block_state);
        }

        if extending {
            let ty = if self.is_sticky { PistonType::Sticky } else { PistonType::Default };
            let state = Block::PISTON_HEAD.default_state().set(props::FACING, direction).set(props::PISTON_TYPE, ty);
            let block_state = Block::MOVING_PISTON.default_state().set(props::FACING, direction).set(props::PISTON_TYPE, ty);
            delete_after_move.remove(arm_pos);
            level.set_block_flags(arm_pos, block_state, 324);
            level.add_and_register_block_entity(Box::new(PistonMovingBlockEntity::new_moving(arm_pos, block_state, state, direction, true, true)));
        }

        let air = Block::AIR.default_state();
        let remaining = delete_after_move.into_entries();
        for (pos, _) in &remaining {
            level.set_block_flags(*pos, air, 82);
        }
        for (pos, old_state) in &remaining {
            old_state.update_indirect_neighbour_shapes(level, *pos, 2, 512);
            air.update_neighbour_shapes(level, *pos, 2, 512);
            air.update_indirect_neighbour_shapes(level, *pos, 2, 512);
        }

        let orientation: Option<Orientation> = level.initial_orientation(Some(resolver_push_direction), None);
        let mut update_index = 0;
        for pos in to_destroy.iter().rev() {
            let state = to_update[update_index];
            update_index += 1;
            state.affect_neighbors_after_removal(level, *pos, false);
            state.update_indirect_neighbour_shapes(level, *pos, 2, 512);
            level.update_neighbors_at_with_orientation(*pos, state.block(), orientation);
        }
        for pos in to_push.iter().rev() {
            let state = to_update[update_index];
            update_index += 1;
            level.update_neighbors_at_with_orientation(*pos, state.block(), orientation);
        }
        if extending {
            level.update_neighbors_at_with_orientation(arm_pos, Block::PISTON_HEAD, orientation);
        }
        true
    }
}

/// The part of `java.util.HashMap<BlockPos, BlockState>` `moveBlocks` needs: `put`/`remove` and iteration in table
/// order (default capacity 16, doubling at 0.75 load, never shrinking; bucket = spread hash; insertion order inside a
/// bucket, which resizes preserve).
#[derive(Default)]
struct JavaPosMap {
    entries: Vec<(BlockPos, BlockState)>,
    capacity: usize,
}

impl JavaPosMap {
    fn put(&mut self, pos: BlockPos, state: BlockState) {
        if self.capacity == 0 {
            self.capacity = 16;
        }
        if let Some(entry) = self.entries.iter_mut().find(|(p, _)| *p == pos) {
            entry.1 = state;
            return;
        }
        self.entries.push((pos, state));
        if self.entries.len() > self.capacity * 3 / 4 {
            self.capacity *= 2;
        }
    }

    fn remove(&mut self, pos: BlockPos) {
        self.entries.retain(|(p, _)| *p != pos);
    }

    fn into_entries(mut self) -> Vec<(BlockPos, BlockState)> {
        let mask = self.capacity.max(1) - 1;
        let bucket = |pos: &BlockPos| {
            let h = pos.hash_code();
            ((h ^ ((h as u32) >> 16) as i32) as u32 as usize) & mask
        };
        self.entries.sort_by_key(|(pos, _)| bucket(pos));
        self.entries
    }
}

impl BlockBehaviour for PistonBaseBlock {
    fn set_placed_by(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _placer: Option<&mut crate::world::entity::ServerPlayer>, _stack: &ItemStack) {
        self.check_if_extend(level, pos, state);
    }

    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        self.check_if_extend(level, pos, state);
    }

    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !old_state.is_block(state.block()) && level.get_block_entity_immediate(pos).is_none() {
            self.check_if_extend(level, pos, state);
        }
    }

    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::FACING, context.get_nearest_looking_direction().get_opposite()).set(props::EXTENDED, false))
    }

    fn trigger_event(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        let direction = state.get(props::FACING);
        let extended_state = state.set(props::EXTENDED, true);
        let extend = get_neighbor_signal(level, pos, direction);
        if extend && (b0 == 1 || b0 == 2) {
            level.set_block_flags(pos, extended_state, 2);
            return false;
        }
        if !extend && b0 == 0 {
            return false;
        }

        if b0 == 0 {
            if !self.move_blocks(level, pos, direction, true) {
                return false;
            }
            level.set_block_flags(pos, extended_state, 67);
            let pitch = level.tick_state.random.next_float() * 0.25 + 0.6;
            level.play_sound(None, pos, SoundEvent::BLOCK_PISTON_EXTEND, SoundSource::Blocks, 0.5, pitch);
            level.game_event_at_block(GameEvent::BLOCK_ACTIVATE, pos, &GameEventContext::of_state(Some(extended_state)));
        } else if b0 == 1 || b0 == 2 {
            level.final_tick_moving_piston(pos.relative(direction));

            let ty = if self.is_sticky { PistonType::Sticky } else { PistonType::Default };
            let moving_piston_state = Block::MOVING_PISTON.default_state().set(props::FACING, direction).set(props::PISTON_TYPE, ty);
            level.set_block_flags(pos, moving_piston_state, 276);
            level.add_and_register_block_entity(Box::new(PistonMovingBlockEntity::new_moving(
                pos,
                moving_piston_state,
                state.block().default_state().set(props::FACING, Direction::from_3d_data_value(b1 & 7)),
                direction,
                false,
                true,
            )));
            level.update_neighbors_at(pos, moving_piston_state.block());
            moving_piston_state.update_neighbour_shapes(level, pos, 2, 512);
            if self.is_sticky {
                let two_pos = pos.offset(direction.get_step_x() * 2, direction.get_step_y() * 2, direction.get_step_z() * 2);
                let moving_state = level.get_block_state(two_pos);
                let mut piston_piece = false;
                if moving_state.is_block(Block::MOVING_PISTON)
                    && let Some(entity) = level.moving_piston_info(two_pos)
                    && entity.direction == direction
                    && entity.extending
                {
                    level.final_tick_moving_piston(two_pos);
                    piston_piece = true;
                }

                if !piston_piece {
                    if b0 != 1
                        || moving_state.is_air()
                        || !is_pushable(moving_state, level, two_pos, direction.get_opposite(), false, direction)
                        || push_reaction(moving_state) != PushReaction::PushPull && !moving_state.is_block(Block::PISTON) && !moving_state.is_block(Block::STICKY_PISTON)
                    {
                        level.remove_block(pos.relative(direction), false);
                    } else {
                        self.move_blocks(level, pos, direction, false);
                    }
                }
            } else {
                level.remove_block(pos.relative(direction), false);
            }

            let pitch = level.tick_state.random.next_float() * 0.15 + 0.6;
            level.play_sound(None, pos, SoundEvent::BLOCK_PISTON_CONTRACT, SoundSource::Blocks, 0.5, pitch);
            level.game_event_at_block(GameEvent::BLOCK_DEACTIVATE, pos, &GameEventContext::of_state(Some(moving_piston_state)));
        }
        true
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("PistonBaseBlock", |block| Box::new(PistonBaseBlock { is_sticky: block == Block::STICKY_PISTON }));
