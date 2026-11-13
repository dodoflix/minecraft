//! `net.minecraft.world.level.block.piston`: `PistonMath` and `PistonStructureResolver` (the block classes are in
//! `impls/piston_*_block.rs`, the block entity is `world::block_entity::PistonMovingBlockEntity`).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::direction::{Direction, Direction::*};
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::PushReaction;
use mc_data::props;

/// `PistonStructureResolver.MAX_PUSH_DEPTH`.
pub const MAX_PUSH_DEPTH: usize = 12;

/// `PistonMath.getMovementArea(aabb, direction, amount)`.
pub fn get_movement_area(aabb: &AABB, direction: Direction, amount: f64) -> AABB {
    let delta = amount * f64::from(direction.get_axis_direction().get_step());
    let min = delta.min(0.0);
    let max = delta.max(0.0);
    match direction {
        West => AABB::new(
            aabb.min_x + min,
            aabb.min_y,
            aabb.min_z,
            aabb.min_x + max,
            aabb.max_y,
            aabb.max_z,
        ),
        East => AABB::new(
            aabb.max_x + min,
            aabb.min_y,
            aabb.min_z,
            aabb.max_x + max,
            aabb.max_y,
            aabb.max_z,
        ),
        Down => AABB::new(
            aabb.min_x,
            aabb.min_y + min,
            aabb.min_z,
            aabb.max_x,
            aabb.min_y + max,
            aabb.max_z,
        ),
        Up => AABB::new(
            aabb.min_x,
            aabb.max_y + min,
            aabb.min_z,
            aabb.max_x,
            aabb.max_y + max,
            aabb.max_z,
        ),
        North => AABB::new(
            aabb.min_x,
            aabb.min_y,
            aabb.min_z + min,
            aabb.max_x,
            aabb.max_y,
            aabb.min_z + max,
        ),
        South => AABB::new(
            aabb.min_x,
            aabb.min_y,
            aabb.max_z + min,
            aabb.max_x,
            aabb.max_y,
            aabb.max_z + max,
        ),
    }
}

/// `PistonBaseBlock.isPushable(state, level, pos, direction, allowDestroyable, connectionDirection)`.
pub fn is_pushable(
    state: BlockState,
    level: &ServerLevel,
    pos: BlockPos,
    direction: Direction,
    allow_destroyable: bool,
    connection_direction: Direction,
) -> bool {
    if pos.y() < level.get_min_y()
        || pos.y() > level.get_max_y()
        || !level.is_within_world_border(pos)
    {
        return false;
    }
    if state.is_air() {
        return true;
    }
    if direction == Down && pos.y() == level.get_min_y() {
        return false;
    }
    if direction == Up && pos.y() == level.get_max_y() {
        return false;
    }
    if !state.is_block(Block::PISTON) && !state.is_block(Block::STICKY_PISTON) {
        if state.get_destroy_speed() == -1.0 {
            return false;
        }
        match state.block().data().push_reaction {
            PushReaction::Immoveable => return false,
            PushReaction::Popped => return allow_destroyable,
            PushReaction::Push => return direction == connection_direction,
            PushReaction::PushPull | PushReaction::IgnoreEntity => {}
        }
    } else if state.get(props::EXTENDED) {
        return false;
    }
    !state.has_block_entity()
}

/// `state.getPistonPushReaction()`.
pub fn push_reaction(state: BlockState) -> PushReaction {
    state.block().data().push_reaction
}

fn is_sticky(state: BlockState) -> bool {
    state.is_block(Block::SLIME_BLOCK) || state.is_block(Block::HONEY_BLOCK)
}

fn can_stick_to_each_other(state1: BlockState, state2: BlockState) -> bool {
    if (state1.is_block(Block::HONEY_BLOCK) && state2.is_block(Block::SLIME_BLOCK))
        || (state1.is_block(Block::SLIME_BLOCK) && state2.is_block(Block::HONEY_BLOCK))
    {
        false
    } else {
        is_sticky(state1) || is_sticky(state2)
    }
}

/// `PistonStructureResolver`.
pub struct PistonStructureResolver<'a> {
    level: &'a ServerLevel,
    piston_pos: BlockPos,
    extending: bool,
    start_pos: BlockPos,
    push_direction: Direction,
    to_push: Vec<BlockPos>,
    to_destroy: Vec<BlockPos>,
    piston_direction: Direction,
}

impl<'a> PistonStructureResolver<'a> {
    pub fn new(
        level: &'a ServerLevel,
        piston_pos: BlockPos,
        direction: Direction,
        extending: bool,
    ) -> Self {
        let (push_direction, start_pos) = if extending {
            (direction, piston_pos.relative(direction))
        } else {
            (
                direction.get_opposite(),
                piston_pos.relative_n(direction, 2),
            )
        };
        Self {
            level,
            piston_pos,
            extending,
            start_pos,
            push_direction,
            to_push: Vec::new(),
            to_destroy: Vec::new(),
            piston_direction: direction,
        }
    }

    fn state_at(&self, pos: BlockPos) -> BlockState {
        BlockGetter::get_block_state(self.level, pos)
    }

    pub fn resolve(&mut self) -> bool {
        self.to_push.clear();
        self.to_destroy.clear();
        let next_state = self.state_at(self.start_pos);
        if !is_pushable(
            next_state,
            self.level,
            self.start_pos,
            self.push_direction,
            false,
            self.piston_direction,
        ) {
            if self.extending && push_reaction(next_state) == PushReaction::Popped {
                self.to_destroy.push(self.start_pos);
                true
            } else {
                false
            }
        } else if !self.add_block_line(self.start_pos, self.push_direction) {
            false
        } else {
            let mut i = 0;
            while i < self.to_push.len() {
                let pos = self.to_push[i];
                if is_sticky(self.state_at(pos)) && !self.add_branching_blocks(pos) {
                    return false;
                }
                i += 1;
            }
            true
        }
    }

    fn add_block_line(&mut self, start: BlockPos, direction: Direction) -> bool {
        let mut next_state = self.state_at(start);
        if next_state.is_air() {
            return true;
        }
        if !is_pushable(
            next_state,
            self.level,
            start,
            self.push_direction,
            false,
            direction,
        ) {
            return true;
        }
        if start == self.piston_pos {
            return true;
        }
        if self.to_push.contains(&start) {
            return true;
        }
        let mut block_count = 1usize;
        if block_count + self.to_push.len() > MAX_PUSH_DEPTH {
            return false;
        }
        while is_sticky(next_state) {
            let pos = start.relative_n(self.push_direction.get_opposite(), block_count as i32);
            let previous_state = next_state;
            next_state = self.state_at(pos);
            if next_state.is_air()
                || !can_stick_to_each_other(previous_state, next_state)
                || !is_pushable(
                    next_state,
                    self.level,
                    pos,
                    self.push_direction,
                    false,
                    self.push_direction.get_opposite(),
                )
                || pos == self.piston_pos
            {
                break;
            }
            block_count += 1;
            if block_count + self.to_push.len() > MAX_PUSH_DEPTH {
                return false;
            }
        }

        let mut blocks_added = 0usize;
        for i in (0..block_count).rev() {
            self.to_push
                .push(start.relative_n(self.push_direction.get_opposite(), i as i32));
            blocks_added += 1;
        }

        let mut i = 1;
        loop {
            let pos = start.relative_n(self.push_direction, i);
            if let Some(collision_pos) = self.to_push.iter().position(|p| *p == pos) {
                self.reorder_list_at_collision(blocks_added, collision_pos);
                for j in 0..=collision_pos + blocks_added {
                    let block_pos = self.to_push[j];
                    if is_sticky(self.state_at(block_pos)) && !self.add_branching_blocks(block_pos)
                    {
                        return false;
                    }
                }
                return true;
            }

            next_state = self.state_at(pos);
            if next_state.is_air() {
                return true;
            }
            if !is_pushable(
                next_state,
                self.level,
                pos,
                self.push_direction,
                true,
                self.push_direction,
            ) || pos == self.piston_pos
            {
                return false;
            }
            if push_reaction(next_state) == PushReaction::Popped {
                self.to_destroy.push(pos);
                return true;
            }
            if self.to_push.len() >= MAX_PUSH_DEPTH {
                return false;
            }
            self.to_push.push(pos);
            blocks_added += 1;
            i += 1;
        }
    }

    fn reorder_list_at_collision(&mut self, blocks_added: usize, collision_pos: usize) {
        let len = self.to_push.len();
        let mut reordered = Vec::with_capacity(len);
        reordered.extend_from_slice(&self.to_push[..collision_pos]);
        reordered.extend_from_slice(&self.to_push[len - blocks_added..]);
        reordered.extend_from_slice(&self.to_push[collision_pos..len - blocks_added]);
        self.to_push = reordered;
    }

    fn add_branching_blocks(&mut self, from_pos: BlockPos) -> bool {
        let from_state = self.state_at(from_pos);
        for direction in Direction::ALL {
            if direction.get_axis() != self.push_direction.get_axis() {
                let neighbour_pos = from_pos.relative(direction);
                let neighbour_state = self.state_at(neighbour_pos);
                if can_stick_to_each_other(neighbour_state, from_state)
                    && !self.add_block_line(neighbour_pos, direction)
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn get_push_direction(&self) -> Direction {
        self.push_direction
    }

    pub fn get_to_push(&self) -> &[BlockPos] {
        &self.to_push
    }

    pub fn get_to_destroy(&self) -> &[BlockPos] {
        &self.to_destroy
    }
}

/// `Entity.getPistonPushReaction()`: the trait method, plus the types whose Java class overrides it to `IGNORE_ENTITY` but
/// whose entity implementation does not exist yet (`Display`).
pub fn entity_push_reaction(entity: &dyn Entity) -> PushReaction {
    let ty = entity.get_type();
    if ty == EntityType::BLOCK_DISPLAY
        || ty == EntityType::ITEM_DISPLAY
        || ty == EntityType::TEXT_DISPLAY
    {
        PushReaction::IgnoreEntity
    } else {
        entity.get_piston_push_reaction()
    }
}
