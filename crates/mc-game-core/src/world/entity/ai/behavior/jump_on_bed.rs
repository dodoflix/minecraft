//! `JumpOnBed`: babies bounce on beds.

use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{NEAREST_BED, WALK_TARGET};
use crate::world::entity::mob;
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor as _;
use crate::world::level::block::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

const MAX_TIME_TO_REACH_BED: i32 = 100;
const MIN_JUMPS: i32 = 3;
const COOLDOWN_BETWEEN_JUMPS: i32 = 5;

pub struct JumpOnBed {
    speed_modifier: f32,
    target_bed: Option<BlockPos>,
    remaining_time_to_reach_bed: i32,
    remaining_jumps: i32,
    remaining_cooldown_until_next_jump: i32,
}

impl JumpOnBed {
    pub fn new(speed_modifier: f32) -> BehaviorBase<JumpOnBed> {
        BehaviorBase::with_duration(
            entry_condition![NEAREST_BED => ValuePresent, WALK_TARGET => ValueAbsent],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            JumpOnBed {
                speed_modifier,
                target_bed: None,
                remaining_time_to_reach_bed: 0,
                remaining_jumps: 0,
                remaining_cooldown_until_next_jump: 0,
            },
        )
    }

    fn near_bed(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        self.on_or_over_bed(level, body) || get_nearest_bed(body).is_some()
    }

    fn on_or_over_bed(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        let body_pos = body.block_position();
        is_jumpable(level, body_pos) || is_jumpable(level, body_pos.below())
    }

    fn on_bed_surface(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        is_jumpable(level, body.block_position())
    }

    fn tired_of_walking(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        !self.on_or_over_bed(level, body) && self.remaining_time_to_reach_bed <= 0
    }

    fn tired_of_jumping(&self, level: &ServerLevel, body: &dyn Entity) -> bool {
        self.on_or_over_bed(level, body) && self.remaining_jumps <= 0
    }
}

fn is_jumpable(level: &ServerLevel, pos: BlockPos) -> bool {
    level
        .get_block_state(pos)
        .is_tag("villager_babies_can_jump_on_bed")
}

fn get_nearest_bed(body: &dyn Entity) -> Option<BlockPos> {
    body.brain().get_memory(&NEAREST_BED).copied()
}

impl Behavior for JumpOnBed {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.is_baby() && self.near_bed(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(target_bed) = get_nearest_bed(body) {
            self.target_bed = Some(target_bed);
            self.remaining_time_to_reach_bed = MAX_TIME_TO_REACH_BED;
            self.remaining_jumps = MIN_JUMPS + level.random().next_int_bounded(4);
            self.remaining_cooldown_until_next_jump = 0;
            body.brain_mut().set_memory(
                &WALK_TARGET,
                WalkTarget::from_block_pos(target_bed, self.speed_modifier, 0),
            );
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {
        self.target_bed = None;
        self.remaining_time_to_reach_bed = 0;
        self.remaining_jumps = 0;
        self.remaining_cooldown_until_next_jump = 0;
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.is_baby()
            && self.target_bed.is_some_and(|bed| is_jumpable(level, bed))
            && !self.tired_of_walking(level, body)
            && !self.tired_of_jumping(level, body)
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if !self.on_or_over_bed(level, body) {
            self.remaining_time_to_reach_bed -= 1;
        } else if self.remaining_cooldown_until_next_jump > 0 {
            self.remaining_cooldown_until_next_jump -= 1;
        } else if self.on_bed_surface(level, body) {
            mob::jump_control_jump(body);
            self.remaining_jumps -= 1;
            self.remaining_cooldown_until_next_jump = COOLDOWN_BETWEEN_JUMPS;
        }
    }
}
