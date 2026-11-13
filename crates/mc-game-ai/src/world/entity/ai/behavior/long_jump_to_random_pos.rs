//! `LongJumpToRandomPos` and its subclass `LongJumpToPreferredBlock` (a configuration of the same behavior here).

use super::long_jump_util;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{Attribute, SoundEvent};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    LONG_JUMP_COOLDOWN_TICKS, LONG_JUMP_MID_JUMP, LOOK_TARGET,
};
use mc_game_core::world::entity::ai::util::unseeded_shuffle::shuffle_unseeded;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_protocol::packets::play::SoundSource;

pub const FIND_JUMP_TRIES: i32 = 20;
const PREPARE_JUMP_DURATION: i64 = 40;
pub const MIN_PATHFIND_DISTANCE_TO_VALID_JUMP: i32 = 8;
pub const TIME_OUT_DURATION: i32 = 200;
const ALLOWED_ANGLES: [i32; 4] = [65, 70, 75, 80];

/// `LongJumpToRandomPos.PossibleJump`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PossibleJump {
    pub target_pos: BlockPos,
    pub weight: i32,
}

/// `BiPredicate<E, BlockPos> acceptableLandingSpot`.
pub type LandingSpotPredicate = Box<dyn Fn(&mut dyn Entity, &ServerLevel, BlockPos) -> bool>;

/// `Function<E, SoundEvent> getJumpSound`.
type JumpSoundGetter = Box<dyn Fn(&dyn Entity) -> SoundEvent>;

/// `LongJumpToPreferredBlock`'s extra state.
struct Preferred {
    preferred_block_tag: &'static str,
    preferred_blocks_chance: f32,
    not_preferred_jump_candidates: Vec<PossibleJump>,
    currently_wanting_preferred_ones: bool,
}

pub struct LongJumpToRandomPos {
    time_between_long_jumps: (i32, i32),
    max_long_jump_height: i32,
    max_long_jump_width: i32,
    max_jump_velocity_multiplier: f32,
    jump_candidates: Vec<PossibleJump>,
    initial_position: Option<Vec3>,
    chosen_jump: Option<Vec3>,
    find_jump_tries: i32,
    prepare_jump_start: i64,
    get_jump_sound: JumpSoundGetter,
    acceptable_landing_spot: LandingSpotPredicate,
    preferred: Option<Preferred>,
}

/// `defaultAcceptableLandingSpot(body, targetPos)`.
pub fn default_acceptable_landing_spot(
    body: &mut dyn Entity,
    level: &ServerLevel,
    target_pos: BlockPos,
) -> bool {
    let below = target_pos.below();
    let ty = navigation::path_type_static(body, level, target_pos);
    level.get_block_state(below).is_solid_render() && body.get_pathfinding_malus(level, ty) == 0.0
}

impl LongJumpToRandomPos {
    /// `new LongJumpToRandomPos(timeBetweenLongJumps, maxLongJumpHeight, maxLongJumpWidth, maxJumpVelocityMultiplier, getJumpSound)`.
    pub fn new(
        time_between_long_jumps: (i32, i32),
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity_multiplier: f32,
        get_jump_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
    ) -> BehaviorBase<LongJumpToRandomPos> {
        Self::with_landing_spot(
            time_between_long_jumps,
            max_long_jump_height,
            max_long_jump_width,
            max_jump_velocity_multiplier,
            get_jump_sound,
            default_acceptable_landing_spot,
        )
    }

    /// `new LongJumpToRandomPos(..., acceptableLandingSpot)`.
    pub fn with_landing_spot(
        time_between_long_jumps: (i32, i32),
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity_multiplier: f32,
        get_jump_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
        acceptable_landing_spot: impl Fn(&mut dyn Entity, &ServerLevel, BlockPos) -> bool + 'static,
    ) -> BehaviorBase<LongJumpToRandomPos> {
        Self::build(
            time_between_long_jumps,
            max_long_jump_height,
            max_long_jump_width,
            max_jump_velocity_multiplier,
            Box::new(get_jump_sound),
            Box::new(acceptable_landing_spot),
            None,
        )
    }

    /// `new LongJumpToPreferredBlock(timeBetweenLongJumps, maxLongJumpHeight, maxLongJumpWidth, maxJumpVelocity, getJumpSound,
    /// preferredBlockTag, preferredBlocksChance, acceptableLandingSpot)`.
    pub fn preferred_block(
        time_between_long_jumps: (i32, i32),
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity: f32,
        get_jump_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
        preferred_block_tag: &'static str,
        preferred_blocks_chance: f32,
        acceptable_landing_spot: impl Fn(&mut dyn Entity, &ServerLevel, BlockPos) -> bool + 'static,
    ) -> BehaviorBase<LongJumpToRandomPos> {
        Self::build(
            time_between_long_jumps,
            max_long_jump_height,
            max_long_jump_width,
            max_jump_velocity,
            Box::new(get_jump_sound),
            Box::new(acceptable_landing_spot),
            Some(Preferred {
                preferred_block_tag,
                preferred_blocks_chance,
                not_preferred_jump_candidates: Vec::new(),
                currently_wanting_preferred_ones: false,
            }),
        )
    }

    fn build(
        time_between_long_jumps: (i32, i32),
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity_multiplier: f32,
        get_jump_sound: JumpSoundGetter,
        acceptable_landing_spot: LandingSpotPredicate,
        preferred: Option<Preferred>,
    ) -> BehaviorBase<LongJumpToRandomPos> {
        BehaviorBase::with_timeout(
            entry_condition![LOOK_TARGET => Registered, LONG_JUMP_COOLDOWN_TICKS => ValueAbsent, LONG_JUMP_MID_JUMP => ValueAbsent],
            TIME_OUT_DURATION,
            LongJumpToRandomPos {
                time_between_long_jumps,
                max_long_jump_height,
                max_long_jump_width,
                max_jump_velocity_multiplier,
                jump_candidates: Vec::new(),
                initial_position: None,
                chosen_jump: None,
                find_jump_tries: 0,
                prepare_jump_start: 0,
                get_jump_sound,
                acceptable_landing_spot,
                preferred,
            },
        )
    }

    fn sample_time_between(&self, level: &mut ServerLevel) -> i32 {
        level.tick_state.random.next_int_between_inclusive(
            self.time_between_long_jumps.0,
            self.time_between_long_jumps.1,
        )
    }

    /// `getJumpCandidate(level)` (the `LongJumpToRandomPos` version: weighted random pick, removed from the list).
    fn get_jump_candidate_base(&mut self, level: &mut ServerLevel) -> Option<PossibleJump> {
        let total: i64 = self
            .jump_candidates
            .iter()
            .map(|j| i64::from(j.weight))
            .sum();
        assert!(
            total <= i64::from(i32::MAX),
            "Sum of weights must be <= 2147483647"
        );
        let total = total as i32;
        if total == 0 {
            return None;
        }
        let mut selection = level.tick_state.random.next_int_bounded(total);
        let mut picked = None;
        for (index, item) in self.jump_candidates.iter().enumerate() {
            selection -= item.weight;
            if selection < 0 {
                picked = Some(index);
                break;
            }
        }
        picked.map(|index| self.jump_candidates.remove(index))
    }

    fn get_jump_candidate(&mut self, level: &mut ServerLevel) -> Option<PossibleJump> {
        let wants_preferred = self
            .preferred
            .as_ref()
            .is_some_and(|p| p.currently_wanting_preferred_ones);
        if self.preferred.is_none() || !wants_preferred {
            return self.get_jump_candidate_base(level);
        }
        while !self.jump_candidates.is_empty() {
            if let Some(possible_jump) = self.get_jump_candidate_base(level) {
                let preferred = self.preferred.as_mut().expect("preferred");
                let below =
                    level.get_block_state(possible_jump.target_pos.relative(Direction::Down));
                if below.is_tag(preferred.preferred_block_tag) {
                    return Some(possible_jump);
                }
                preferred.not_preferred_jump_candidates.push(possible_jump);
            }
        }
        let preferred = self.preferred.as_mut().expect("preferred");
        if preferred.not_preferred_jump_candidates.is_empty() {
            None
        } else {
            Some(preferred.not_preferred_jump_candidates.remove(0))
        }
    }

    fn is_acceptable_landing_position(
        &self,
        level: &ServerLevel,
        body: &mut dyn Entity,
        target_pos: BlockPos,
    ) -> bool {
        let body_pos = body.block_position();
        if body_pos.x() == target_pos.x() && body_pos.z() == target_pos.z() {
            false
        } else {
            (self.acceptable_landing_spot)(body, level, target_pos)
        }
    }

    fn calculate_optimal_jump_vector(
        &self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target_pos: Vec3,
    ) -> Option<Vec3> {
        let mut allowed_angles = ALLOWED_ANGLES.to_vec();
        shuffle_unseeded(&mut allowed_angles);
        let max_jump_velocity = (body
            .living()
            .expect("living")
            .get_attribute_value(Attribute::JUMP_STRENGTH)
            * f64::from(self.max_jump_velocity_multiplier)) as f32;
        for angle in allowed_angles {
            if let Some(v) = long_jump_util::calculate_jump_vector_for_angle(
                level,
                body,
                target_pos,
                max_jump_velocity,
                angle,
                true,
            ) {
                return Some(v);
            }
        }
        None
    }

    fn pick_candidate(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        while !self.jump_candidates.is_empty() {
            let Some(position) = self.get_jump_candidate(level) else {
                continue;
            };
            let target_pos = position.target_pos;
            if self.is_acceptable_landing_position(level, body, target_pos) {
                let target_position = Vec3::at_center_of(&target_pos);
                if let Some(jump_vector) =
                    self.calculate_optimal_jump_vector(level, body, target_position)
                {
                    body.brain_mut().set_memory(
                        &LOOK_TARGET,
                        PositionTracker::Block(BlockPosTracker::new(target_pos)),
                    );
                    let path = navigation::with_navigation(body, level, |n, m, l| {
                        n.create_path_pos_len(
                            m,
                            &*l,
                            target_pos,
                            0,
                            MIN_PATHFIND_DISTANCE_TO_VALID_JUMP as f32,
                        )
                    })
                    .flatten();
                    if path.is_none_or(|p| !p.can_reach()) {
                        self.chosen_jump = Some(jump_vector);
                        self.prepare_jump_start = timestamp;
                        return;
                    }
                }
            }
        }
    }
}

impl Behavior for LongJumpToRandomPos {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let can_start = body.base().on_ground()
            && !body.is_in_water()
            && !body.is_in_lava()
            && !level
                .get_block_state(body.block_position())
                .is_block(Block::HONEY_BLOCK);
        if !can_start {
            let cooldown = self.sample_time_between(level) / 2;
            body.brain_mut()
                .set_memory(&LONG_JUMP_COOLDOWN_TICKS, cooldown);
        }
        can_start
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        let is_valid = self.initial_position.is_some_and(|p| p == body.position())
            && self.find_jump_tries > 0
            && !body.is_in_water()
            && (self.chosen_jump.is_some() || !self.jump_candidates.is_empty());
        if !is_valid && body.brain().get_memory(&LONG_JUMP_MID_JUMP).is_none() {
            let cooldown = self.sample_time_between(level) / 2;
            body.brain_mut()
                .set_memory(&LONG_JUMP_COOLDOWN_TICKS, cooldown);
            body.brain_mut().erase_memory(&LOOK_TARGET);
        }
        is_valid
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.chosen_jump = None;
        self.find_jump_tries = FIND_JUMP_TRIES;
        self.initial_position = Some(body.position());
        let mob_pos = body.block_position();
        let (mob_x, mob_y, mob_z) = (mob_pos.x(), mob_pos.y(), mob_pos.z());
        self.jump_candidates = BlockPos::between_closed_xyz(
            mob_x - self.max_long_jump_width,
            mob_y - self.max_long_jump_height,
            mob_z - self.max_long_jump_width,
            mob_x + self.max_long_jump_width,
            mob_y + self.max_long_jump_height,
            mob_z + self.max_long_jump_width,
        )
        .filter(|pos| *pos != mob_pos)
        .map(|pos| PossibleJump {
            target_pos: pos,
            weight: mth::ceil_f64(mob_pos.dist_sqr(&pos)),
        })
        .collect();
        if let Some(p) = self.preferred.as_mut() {
            p.not_preferred_jump_candidates.clear();
            p.currently_wanting_preferred_ones =
                body.base_mut().random.next_float() < p.preferred_blocks_chance;
        }
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if let Some(chosen_jump) = self.chosen_jump {
            if timestamp - self.prepare_jump_start >= PREPARE_JUMP_DURATION {
                let y_body_rot = body.living().expect("living").y_body_rot;
                body.base_mut().set_y_rot(y_body_rot);
                body.living_mut().expect("living").discard_friction = true;
                let org_length = chosen_jump.length();
                let length_with_jump_boost = org_length
                    + f64::from(
                        mc_game_core::world::entity::living_entity::jump::get_jump_boost_power(
                            &*body,
                        ),
                    );
                body.base_mut()
                    .set_delta_movement(chosen_jump.scale(length_with_jump_boost / org_length));
                body.brain_mut().set_memory(&LONG_JUMP_MID_JUMP, true);
                behavior_utils::play_sound_from_entity(
                    level,
                    &*body,
                    (self.get_jump_sound)(&*body),
                    SoundSource::Neutral,
                    1.0,
                    1.0,
                );
            }
        } else {
            self.find_jump_tries -= 1;
            self.pick_candidate(level, body, timestamp);
        }
    }
}
