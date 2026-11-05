//! `PrepareRamNearestTarget`.

use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, RAM_COOLDOWN_TICKS, RAM_TARGET, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::navigation;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

/// `TIME_OUT_DURATION`.
pub const TIME_OUT_DURATION: i32 = 160;

/// `PrepareRamNearestTarget.RamCandidate`.
#[derive(Clone)]
pub struct RamCandidate {
    start_position: BlockPos,
    target_position: BlockPos,
    target: EntityId,
}

pub struct PrepareRamNearestTarget {
    get_cooldown_on_fail: Box<dyn Fn(&dyn Entity) -> i32>,
    min_ram_distance: i32,
    max_ram_distance: i32,
    walk_speed: f32,
    ram_targeting: TargetingConditions,
    ram_prepare_time: i32,
    get_prepare_ram_sound: Box<dyn Fn(&dyn Entity) -> SoundEvent>,
    reached_ram_position_timestamp: Option<i64>,
    ram_candidate: Option<RamCandidate>,
}

impl PrepareRamNearestTarget {
    /// `new PrepareRamNearestTarget(getCooldownOnFail, minRamDistance, maxRamDistance, walkSpeed, ramTargeting, ramPrepareTime, getPrepareRamSound)`.
    pub fn new(
        get_cooldown_on_fail: impl Fn(&dyn Entity) -> i32 + 'static,
        min_ram_distance: i32,
        max_ram_distance: i32,
        walk_speed: f32,
        ram_targeting: TargetingConditions,
        ram_prepare_time: i32,
        get_prepare_ram_sound: impl Fn(&dyn Entity) -> SoundEvent + 'static,
    ) -> BehaviorBase<PrepareRamNearestTarget> {
        BehaviorBase::with_timeout(
            entry_condition![LOOK_TARGET => Registered, RAM_COOLDOWN_TICKS => ValueAbsent, NEAREST_VISIBLE_LIVING_ENTITIES => ValuePresent, RAM_TARGET => ValueAbsent],
            TIME_OUT_DURATION,
            PrepareRamNearestTarget {
                get_cooldown_on_fail: Box::new(get_cooldown_on_fail),
                min_ram_distance,
                max_ram_distance,
                walk_speed,
                ram_targeting,
                ram_prepare_time,
                get_prepare_ram_sound: Box::new(get_prepare_ram_sound),
                reached_ram_position_timestamp: None,
                ram_candidate: None,
            },
        )
    }

    fn get_edge_of_block(start_ram_pos: BlockPos, target_pos: BlockPos) -> Vec3 {
        let x_offset = 0.5 * f64::from((target_pos.x() - start_ram_pos.x()).signum());
        let z_offset = 0.5 * f64::from((target_pos.z() - start_ram_pos.z()).signum());
        Vec3::at_bottom_center_of(&target_pos).add_xyz(x_offset, 0.0, z_offset)
    }

    fn is_walkable_block(level: &ServerLevel, body: &mut dyn Entity, target_pos: BlockPos) -> bool {
        let stable = navigation::is_stable_destination(body, level, target_pos);
        stable && {
            let ty = navigation::path_type_static(body, level, target_pos);
            body.get_pathfinding_malus(level, ty) == 0.0
        }
    }

    fn calculate_ramming_start_position(&self, level: &mut ServerLevel, body: &mut dyn Entity, ramable_target: EntityId) -> Option<BlockPos> {
        let target_pos = level.get_entity(ramable_target)?.block_position();
        if !Self::is_walkable_block(level, body, target_pos) {
            return None;
        }
        let mut possible_ram_positions: Vec<BlockPos> = Vec::new();
        for direction in Direction::HORIZONTAL {
            let mut walkable_pos_furthest_away_from_target = target_pos;
            for _ in 0..self.max_ram_distance {
                let next = walkable_pos_furthest_away_from_target.relative(direction);
                if !Self::is_walkable_block(level, body, next) {
                    walkable_pos_furthest_away_from_target = walkable_pos_furthest_away_from_target.relative(direction.get_opposite());
                    break;
                }
                walkable_pos_furthest_away_from_target = next;
            }
            if walkable_pos_furthest_away_from_target.dist_manhattan(&target_pos) >= self.min_ram_distance {
                possible_ram_positions.push(walkable_pos_furthest_away_from_target);
            }
        }
        let body_pos = body.block_position();
        possible_ram_positions.sort_by(|a, b| body_pos.dist_sqr(a).total_cmp(&body_pos.dist_sqr(b)));
        possible_ram_positions.into_iter().find(|&pos| navigation::create_path_pos(body, level, pos, 0).is_some_and(|p| p.can_reach()))
    }

    fn choose_ram_position(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, ramable_target: EntityId) {
        self.reached_ram_position_timestamp = None;
        self.ram_candidate = self.calculate_ramming_start_position(level, body, ramable_target).and_then(|pos| {
            level.get_entity(ramable_target).map(|t| RamCandidate { start_position: pos, target_position: t.block_position(), target: ramable_target })
        });
    }
}

impl Behavior for PrepareRamNearestTarget {
    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let closest = body
            .brain()
            .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
            .and_then(|e| e.find_closest(&*level, &*body, |entity| self.ram_targeting.test(&*level, Some(&*body), entity)));
        if let Some(living_entity) = closest {
            self.choose_ram_position(level, body, living_entity.0);
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if !body.brain().has_memory_value(RAM_TARGET) {
            level.broadcast_entity_event(body.id(), 59);
            let cooldown = (self.get_cooldown_on_fail)(&*body);
            body.brain_mut().set_memory(&RAM_COOLDOWN_TICKS, cooldown);
        }
    }

    fn can_still_use(&mut self, level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) -> bool {
        self.ram_candidate.as_ref().is_some_and(|c| level.get_entity(c.target).is_some_and(|t| t.is_alive()))
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(candidate) = self.ram_candidate.clone() else { return };
        let Some(target) = level.get_entity(candidate.target) else { return };
        let look = EntityTracker::new(target, true);
        let did_target_move = target.block_position() != candidate.target_position;
        body.brain_mut().set_memory(&WALK_TARGET, WalkTarget::from_block_pos(candidate.start_position, self.walk_speed, 0));
        body.brain_mut().set_memory(&LOOK_TARGET, PositionTracker::Entity(look));
        if did_target_move {
            level.broadcast_entity_event(body.id(), 59);
            navigation::stop(body, level);
            self.choose_ram_position(level, body, candidate.target);
        } else {
            let start_ram_pos = body.block_position();
            if start_ram_pos == candidate.start_position {
                level.broadcast_entity_event(body.id(), 58);
                if self.reached_ram_position_timestamp.is_none() {
                    self.reached_ram_position_timestamp = Some(timestamp);
                }
                if timestamp - self.reached_ram_position_timestamp.expect("set above") >= i64::from(self.ram_prepare_time) {
                    body.brain_mut().set_memory(&RAM_TARGET, Self::get_edge_of_block(start_ram_pos, candidate.target_position));
                    let sound = (self.get_prepare_ram_sound)(&*body);
                    let pitch = body.get_voice_pitch();
                    behavior_utils::play_sound_from_entity(level, body.id(), sound, SoundSource::Neutral, 1.0, pitch);
                    self.ram_candidate = None;
                }
            }
        }
    }
}
