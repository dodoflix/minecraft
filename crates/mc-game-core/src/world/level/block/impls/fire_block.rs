//! `net.minecraft.world.level.block.FireBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use mc_core::random::RandomSource;

pub struct FireBlock;

/// `FireBlock.getIgniteOdds(state)` (`bootStrap` table; waterlogged blocks do not burn).
fn ignite_odds(state: BlockState) -> i32 {
    if state.has_property(props::WATERLOGGED) && state.get(props::WATERLOGGED) {
        0
    } else {
        mc_data::misc::fire_odds(state.block()).map_or(0, |(ignite, _)| ignite)
    }
}

/// `FireBlock.getBurnOdds(state)`.
fn burn_odds(state: BlockState) -> i32 {
    if state.has_property(props::WATERLOGGED) && state.get(props::WATERLOGGED) {
        0
    } else {
        mc_data::misc::fire_odds(state.block()).map_or(0, |(_, burn)| burn)
    }
}

/// `FireBlock.getFireTickDelay(random)`.
pub fn get_fire_tick_delay(random: &mut dyn mc_core::random::RandomSource) -> i32 {
    30 + random.next_int_bounded(10)
}

/// `FireBlock.isNearRain(level, testPos)`.
fn is_near_rain(level: &ServerLevel, test_pos: BlockPos) -> bool {
    level.is_raining_at(test_pos)
        || level.is_raining_at(test_pos.west())
        || level.is_raining_at(test_pos.east())
        || level.is_raining_at(test_pos.north())
        || level.is_raining_at(test_pos.south())
}

/// `FireBlock.getIgniteOdds(level, pos)`: the best ignite odds of the neighbours of an empty position.
fn get_ignite_odds_at(level: &dyn LevelReader, pos: BlockPos) -> i32 {
    if !level.is_empty_block(pos) {
        return 0;
    }
    let mut odds = 0;
    for direction in Direction::ALL {
        odds = odds.max(ignite_odds(level.get_block_state(pos.relative(direction))));
    }
    odds
}

/// `FireBlock.checkBurnOut(level, pos, chance, random, age)`.
fn check_burn_out(level: &mut ServerLevel, pos: BlockPos, chance: i32, age: i32) {
    let odds = burn_odds(level.get_block_state(pos));
    if LevelAccessor::random(level).next_int_bounded(chance) < odds {
        let old_state = level.get_block_state(pos);
        if LevelAccessor::random(level).next_int_bounded(age + 10) < 5 && !level.is_raining_at(pos)
        {
            let new_age = (age + LevelAccessor::random(level).next_int_bounded(5) / 4).min(15);
            let state = get_state_with_age(&*level, pos, new_age);
            level.set_block_and_update(pos, state);
        } else {
            level.remove_block(pos, false);
        }
        if helpers::is_instance_of(old_state, "TntBlock") {
            super::tnt_block::prime(level, pos);
        }
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
            result = result.set(
                helpers::pipe_property(direction),
                can_burn(level.get_block_state(pos.relative(direction))),
            );
        }
    }
    result
}

/// `FireBlock.isValidFireLocation`.
fn is_valid_fire_location(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    Direction::ALL
        .into_iter()
        .any(|direction| can_burn(level.get_block_state(pos.relative(direction))))
}

/// `FireBlock.getStateWithAge`.
fn get_state_with_age(level: &dyn BlockGetter, pos: BlockPos, age: i32) -> BlockState {
    let state = super::base_fire_block::get_state(level, pos);
    if state.is_block(Block::FIRE) {
        state.set(props::AGE_15, age)
    } else {
        state
    }
}

impl BlockBehaviour for FireBlock {
    fn get_state_for_placement(
        &self,
        _block: Block,
        level: &mut ServerLevel,
        context: &crate::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(get_state_for_placement(&*level, context.get_clicked_pos()))
    }
    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
    ) -> BlockState {
        super::base_fire_block::player_will_destroy(state, level, pos, player)
    }
    fn tick(&self, mut state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let delay = get_fire_tick_delay(LevelAccessor::random(level));
        helpers::schedule_block_tick(level, pos, Block::FIRE, delay);
        if !level.can_spread_fire_around(pos) {
            return;
        }
        if !state.can_survive(&*level, pos) {
            level.remove_block(pos, false);
        }
        let below_state = level.get_block_state(pos.below());
        let infini_burn = level.is_infiniburn(below_state);
        let age = state.get(props::AGE_15);
        if !infini_burn
            && level.is_raining()
            && is_near_rain(level, pos)
            && LevelAccessor::random(level).next_float() < 0.2 + age as f32 * 0.03
        {
            level.remove_block(pos, false);
            return;
        }
        let new_age = 15.min(age + LevelAccessor::random(level).next_int_bounded(3) / 2);
        if age != new_age {
            state = state.set(props::AGE_15, new_age);
            level.set_block_flags(pos, state, 260);
        }
        if !infini_burn {
            if !is_valid_fire_location(&*level, pos) {
                let below = pos.below();
                if !level
                    .get_block_state(below)
                    .is_face_sturdy_at(&*level, below, Direction::Up)
                    || age > 3
                {
                    level.remove_block(pos, false);
                }
                return;
            }
            if age == 15
                && LevelAccessor::random(level).next_int_bounded(4) == 0
                && !can_burn(level.get_block_state(pos.below()))
            {
                level.remove_block(pos, false);
                return;
            }
        }
        let increased_burnout = level.attribute_bool(
            crate::world::attribute::attributes::INCREASED_FIRE_BURNOUT,
            pos,
        );
        let extra = if increased_burnout { -50 } else { 0 };
        check_burn_out(level, pos.east(), 300 + extra, age);
        check_burn_out(level, pos.west(), 300 + extra, age);
        check_burn_out(level, pos.below(), 250 + extra, age);
        check_burn_out(level, pos.above(), 250 + extra, age);
        check_burn_out(level, pos.north(), 300 + extra, age);
        check_burn_out(level, pos.south(), 300 + extra, age);
        for xx in -1..=1 {
            for zz in -1..=1 {
                for yy in -1..=4 {
                    if xx == 0 && yy == 0 && zz == 0 {
                        continue;
                    }
                    let mut rate = 100;
                    if yy > 1 {
                        rate += (yy - 1) * 100;
                    }
                    let test_pos = BlockPos::new(pos.x() + xx, pos.y() + yy, pos.z() + zz);
                    let ignite_odds = get_ignite_odds_at(&*level, test_pos);
                    if ignite_odds > 0 {
                        let mut odds =
                            (ignite_odds + 40 + level.difficulty().get_id() * 7) / (age + 30);
                        if increased_burnout {
                            odds /= 2;
                        }
                        if odds > 0
                            && LevelAccessor::random(level).next_int_bounded(rate) <= odds
                            && (!level.is_raining() || !is_near_rain(level, test_pos))
                        {
                            let spread_age =
                                15.min(age + LevelAccessor::random(level).next_int_bounded(5) / 4);
                            let spread = get_state_with_age(&*level, test_pos, spread_age);
                            level.set_block_and_update(test_pos, spread);
                        }
                    }
                }
            }
        }
    }
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
    /// `BaseFireBlock.onPlace` (portal ignition) + `FireBlock.onPlace`.
    fn on_place(
        &self,
        state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        super::base_fire_block::on_place(state, level, pos, old_state);
        let delay = 30 + level.random().next_int_bounded(10);
        helpers::schedule_block_tick(level, pos, state.block(), delay);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        level
            .get_block_state(below)
            .is_face_sturdy_at(level, below, Direction::Up)
            || is_valid_fire_location(level, pos)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if self.can_survive(state, &*level, pos) {
                get_state_with_age(&*level, pos, state.get(props::AGE_15))
            } else {
                air()
            }
        }
    );
}

register_block_class!("FireBlock", |_| Box::new(FireBlock));
