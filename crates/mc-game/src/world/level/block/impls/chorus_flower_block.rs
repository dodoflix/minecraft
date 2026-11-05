//! `net.minecraft.world.level.block.ChorusFlowerBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use super::chorus_plant_block::get_state_with_connections;
use mc_core::direction::Plane;
use mc_core::random::RandomSource;

pub struct ChorusFlowerBlock;

fn all_neighbors_empty(level: &dyn LevelReader, pos: BlockPos, ignore: Option<Direction>) -> bool {
    Direction::HORIZONTAL.into_iter().all(|d| Some(d) == ignore || level.is_empty_block(pos.relative(d)))
}

/// `ChorusFlowerBlock.generatePlant`.
pub fn generate_plant(level: &mut dyn LevelAccessor, target: BlockPos, random: &mut dyn RandomSource, max_horizontal_spread: i32) {
    let state = get_state_with_connections(&*level, target, Block::CHORUS_PLANT.default_state());
    level.set_block_flags(target, state, 2);
    grow_tree_recursive(level, target, random, target, max_horizontal_spread, 0);
}

fn place_chorus(level: &mut dyn LevelAccessor, pos: BlockPos) {
    let state = get_state_with_connections(&*level, pos, Block::CHORUS_PLANT.default_state());
    level.set_block_flags(pos, state, 2);
}

fn grow_tree_recursive(level: &mut dyn LevelAccessor, current: BlockPos, random: &mut dyn RandomSource, start_pos: BlockPos, max_horizontal_spread: i32, depth: i32) {
    let mut height = random.next_int_bounded(4) + 1;
    if depth == 0 {
        height += 1;
    }
    for i in 0..height {
        let target = current.above_n(i + 1);
        if !all_neighbors_empty(&*level, target, None) {
            return;
        }
        place_chorus(level, target);
        place_chorus(level, target.below());
    }
    let mut placed_stem = false;
    if depth < 4 {
        let mut stems = random.next_int_bounded(4);
        if depth == 0 {
            stems += 1;
        }
        for _ in 0..stems {
            let direction = Plane::Horizontal.get_random_direction(random);
            let target = current.above_n(height).relative(direction);
            if (target.x() - start_pos.x()).abs() < max_horizontal_spread
                && (target.z() - start_pos.z()).abs() < max_horizontal_spread
                && level.is_empty_block(target)
                && level.is_empty_block(target.below())
                && all_neighbors_empty(&*level, target, Some(direction.get_opposite()))
            {
                placed_stem = true;
                place_chorus(level, target);
                place_chorus(level, target.relative(direction.get_opposite()));
                grow_tree_recursive(level, target, random, start_pos, max_horizontal_spread, depth + 1);
            }
        }
    }
    if !placed_stem {
        level.set_block_flags(current.above_n(height), Block::CHORUS_FLOWER.default_state().set(props::AGE_5, 5), 2);
    }
}

/// `placeGrownFlower`.
fn place_grown_flower(level: &mut ServerLevel, pos: BlockPos, age: i32) {
    level.set_block_flags(pos, Block::CHORUS_FLOWER.default_state().set(props::AGE_5, age), 2);
    level.level_event(None, 1033, pos, 0);
}

/// `placeDeadFlower`.
fn place_dead_flower(level: &mut ServerLevel, pos: BlockPos) {
    level.set_block_flags(pos, Block::CHORUS_FLOWER.default_state().set(props::AGE_5, 5), 2);
    level.level_event(None, 1034, pos, 0);
}

impl BlockBehaviour for ChorusFlowerBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let above = pos.above();
        if !(level.is_empty_block(above) && above.y() <= level.get_max_y()) {
            return;
        }
        let current_age = state.get(props::AGE_5);
        if current_age >= 5 {
            return;
        }
        let mut grow_upwards = false;
        let mut pillar_on_support_block = false;
        let below_state = level.get_block_state(pos.below());
        if below_state.is_tag("supports_chorus_flower") {
            grow_upwards = true;
        } else if below_state.is_block(Block::CHORUS_PLANT) {
            let mut height = 1;
            for _ in 0..4 {
                let test_state = level.get_block_state(pos.below_n(height + 1));
                if !test_state.is_block(Block::CHORUS_PLANT) {
                    if test_state.is_tag("supports_chorus_flower") {
                        pillar_on_support_block = true;
                    }
                    break;
                }
                height += 1;
            }
            if height < 2 || height <= level.random().next_int_bounded(if pillar_on_support_block { 5 } else { 4 }) {
                grow_upwards = true;
            }
        } else if below_state.is_air() {
            grow_upwards = true;
        }
        if grow_upwards && all_neighbors_empty(&*level, above, None) && level.is_empty_block(pos.above_n(2)) {
            let plant = get_state_with_connections(&*level, pos, Block::CHORUS_PLANT.default_state());
            level.set_block_flags(pos, plant, 2);
            place_grown_flower(level, above, current_age);
        } else if current_age < 4 {
            let mut num_branch_attempts = level.random().next_int_bounded(4);
            if pillar_on_support_block {
                num_branch_attempts += 1;
            }
            let mut created_branch = false;
            for _ in 0..num_branch_attempts {
                let direction = Plane::Horizontal.get_random_direction(level.random());
                let target = pos.relative(direction);
                if level.is_empty_block(target) && level.is_empty_block(target.below()) && all_neighbors_empty(&*level, target, Some(direction.get_opposite())) {
                    place_grown_flower(level, target, current_age + 1);
                    created_branch = true;
                }
            }
            if created_branch {
                let plant = get_state_with_connections(&*level, pos, Block::CHORUS_PLANT.default_state());
                level.set_block_flags(pos, plant, 2);
            } else {
                place_dead_flower(level, pos);
            }
        } else {
            place_dead_flower(level, pos);
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        if !below_state.is_block(Block::CHORUS_PLANT) && !below_state.is_tag("supports_chorus_flower") {
            if !below_state.is_air() {
                return false;
            }
            let mut one_neighbor = false;
            for direction in Direction::HORIZONTAL {
                let neighbor = level.get_block_state(pos.relative(direction));
                if neighbor.is_block(Block::CHORUS_PLANT) {
                    if one_neighbor {
                        return false;
                    }
                    one_neighbor = true;
                } else if !neighbor.is_air() {
                    return false;
                }
            }
            one_neighbor
        } else {
            true
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction != Direction::Up && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
}

register_block_class!("ChorusFlowerBlock", |_| Box::new(ChorusFlowerBlock));
