//! `net.minecraft.world.level.block.ChorusFlowerBlock`.
use super::prelude::*;
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

impl BlockBehaviour for ChorusFlowerBlock {
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
