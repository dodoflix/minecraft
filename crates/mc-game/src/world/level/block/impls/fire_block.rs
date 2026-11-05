//! `net.minecraft.world.level.block.FireBlock`.
use super::prelude::*;

pub struct FireBlock;

/// `FireBlock.getIgniteOdds(state)` (`bootStrap` table; waterlogged blocks do not burn).
fn ignite_odds(state: BlockState) -> i32 {
    if state.has_property(props::WATERLOGGED) && state.get(props::WATERLOGGED) {
        0
    } else {
        mc_data::misc::fire_odds(state.block()).map_or(0, |(ignite, _)| ignite)
    }
}

/// `FireBlock.canBurn`.
pub fn can_burn(state: BlockState) -> bool {
    ignite_odds(state) > 0
}

/// `FireBlock.getStateForPlacement(level, pos)`.
pub fn get_state_for_placement(level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    let below = pos.below();
    let below_state = level.get_block_state(below);
    let fire = Block::FIRE.default_state();
    if can_burn(below_state) || below_state.is_face_sturdy_at(level, below, Direction::Up) {
        return fire;
    }
    let mut result = fire;
    for direction in Direction::ALL {
        if direction != Direction::Down {
            result = result.set(helpers::pipe_property(direction), can_burn(level.get_block_state(pos.relative(direction))));
        }
    }
    result
}

/// `FireBlock.isValidFireLocation`.
fn is_valid_fire_location(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    Direction::ALL.into_iter().any(|direction| can_burn(level.get_block_state(pos.relative(direction))))
}

/// `FireBlock.getStateWithAge`.
fn get_state_with_age(level: &dyn BlockGetter, pos: BlockPos, age: i32) -> BlockState {
    let state = super::base_fire_block::get_state(level, pos);
    if state.is_block(Block::FIRE) { state.set(props::AGE_15, age) } else { state }
}

impl BlockBehaviour for FireBlock {
    fn entity_inside(
        &self,
        _state: BlockState,
        _level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn crate::world::entity::Entity,
        effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        super::base_fire_block::entity_inside(effect_applier, 1.0);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        level.get_block_state(below).is_face_sturdy_at(level, below, Direction::Up) || is_valid_fire_location(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if self.can_survive(state, &*level, pos) { get_state_with_age(&*level, pos, state.get(props::AGE_15)) } else { air() }
    });
}

register_block_class!("FireBlock", |_| Box::new(FireBlock));
