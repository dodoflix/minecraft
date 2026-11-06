//! `net.minecraft.world.entity.animal.frog.ShootTongue`: the frog walks up to its target, shoots its tongue and eats it.

use super::frog::{erase_tongue_target, get_tongue_target, set_tongue_target};
use mc_data::SoundEvent;
use mc_data::enums::Pose;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase, behavior_utils};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, IS_PANICKING, LOOK_TARGET, UNREACHABLE_TONGUE_TARGETS, WALK_TARGET,
};
use mc_game_core::world::entity::ai::sensing::mob_sensors::frog_can_eat;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::{combat, navigation};
use mc_game_core::world::entity::{EntityId, RemovalReason};
use mc_protocol::packets::play::SoundSource;
use uuid::Uuid;

/// `TIME_OUT_DURATION`.
pub const TIME_OUT_DURATION: i32 = 100;
/// `CATCH_ANIMATION_DURATION`.
pub const CATCH_ANIMATION_DURATION: i32 = 6;
/// `TONGUE_ANIMATION_DURATION`.
pub const TONGUE_ANIMATION_DURATION: i32 = 10;
/// `EATING_DISTANCE`.
const EATING_DISTANCE: f32 = 1.75;
/// `EATING_MOVEMENT_FACTOR`.
const EATING_MOVEMENT_FACTOR: f64 = 0.75;
/// `UNREACHABLE_TONGUE_TARGETS_COOLDOWN_DURATION`.
pub const UNREACHABLE_TONGUE_TARGETS_COOLDOWN_DURATION: i64 = 100;
/// `MAX_UNREACHBLE_TONGUE_TARGETS_IN_MEMORY`.
pub const MAX_UNREACHBLE_TONGUE_TARGETS_IN_MEMORY: usize = 5;

/// `ShootTongue.State`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    MoveToTarget,
    CatchAnimation,
    EatAnimation,
    Done,
}

pub struct ShootTongue {
    eat_animation_timer: i32,
    calculate_path_counter: i32,
    tongue_sound: SoundEvent,
    eat_sound: SoundEvent,
    state: State,
}

impl ShootTongue {
    /// `new ShootTongue(tongueSound, eatSound)`.
    pub fn new(tongue_sound: SoundEvent, eat_sound: SoundEvent) -> BehaviorBase<ShootTongue> {
        BehaviorBase::with_timeout(
            entry_condition![WALK_TARGET => ValueAbsent, LOOK_TARGET => Registered, ATTACK_TARGET => ValuePresent, IS_PANICKING => ValueAbsent],
            TIME_OUT_DURATION,
            ShootTongue {
                eat_animation_timer: 0,
                calculate_path_counter: 0,
                tongue_sound,
                eat_sound,
                state: State::Done,
            },
        )
    }

    /// `eatEntity(level, body)`.
    fn eat_entity(&self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let body_id = body.id();
        let position = body.position();
        level.play_sound_from_entity_at(
            None,
            body_id,
            position,
            self.eat_sound,
            SoundSource::Neutral,
            2.0,
            1.0,
        );
        if let Some(target) = get_tongue_target(&*body, level)
            && level.get_entity(target).is_some_and(|t| t.is_alive())
        {
            combat::do_hurt_target(body, level, target);
            level.with_entity(target, |_, t| {
                if !t.is_alive() {
                    t.remove(RemovalReason::Killed);
                }
            });
        }
    }
}

/// `target.distanceTo(body)` (float arithmetic like `Entity.distanceTo`).
fn distance_to(a: &dyn Entity, b: &dyn Entity) -> f32 {
    let xd = (a.base().x() - b.base().x()) as f32;
    let yd = (a.base().y() - b.base().y()) as f32;
    let zd = (a.base().z() - b.base().z()) as f32;
    (xd * xd + yd * yd + zd * zd).sqrt()
}

/// `getBrain().getMemory(ATTACK_TARGET)`.
fn attack_target(body: &dyn Entity) -> Option<EntityId> {
    body.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
}

/// `canPathfindToTarget(body, target)`.
fn can_pathfind_to_target(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    target: EntityId,
) -> bool {
    let Some(target_pos) = level.get_entity(target).map(|t| t.block_position()) else {
        return false;
    };
    let path = navigation::create_path_entity(body, level, target_pos, 0);
    path.is_some_and(|p| p.get_dist_to_target() < EATING_DISTANCE)
}

/// `addUnreachableTargetToMemory(body, entity)`.
fn add_unreachable_target_to_memory(level: &ServerLevel, body: &mut dyn Entity, target: EntityId) {
    let Some(uuid) = level.get_entity(target).map(|t| t.uuid()) else {
        return;
    };
    let mut unreachable_targets: Vec<Uuid> = body
        .brain()
        .get_memory(&UNREACHABLE_TONGUE_TARGETS)
        .cloned()
        .unwrap_or_default();
    let should_add_unreachable_target = !unreachable_targets.contains(&uuid);
    if unreachable_targets.len() == MAX_UNREACHBLE_TONGUE_TARGETS_IN_MEMORY
        && should_add_unreachable_target
    {
        unreachable_targets.remove(0);
    }
    if should_add_unreachable_target {
        unreachable_targets.push(uuid);
    }
    body.brain_mut().set_memory_with_expiry(
        &UNREACHABLE_TONGUE_TARGETS,
        unreachable_targets,
        UNREACHABLE_TONGUE_TARGETS_COOLDOWN_DURATION,
    );
}

impl Behavior for ShootTongue {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let Some(target) = attack_target(&*body) else {
            return false;
        };
        let can_pathfind = can_pathfind_to_target(level, body, target);
        if !can_pathfind {
            body.brain_mut().erase_memory(&ATTACK_TARGET);
            add_unreachable_target_to_memory(level, body, target);
        }
        can_pathfind
            && body.base().get_pose() != Pose::Croaking
            && level.get_entity(target).is_some_and(frog_can_eat)
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().has_memory_value(ATTACK_TARGET.id())
            && self.state != State::Done
            && !body.brain().has_memory_value(IS_PANICKING.id())
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = attack_target(&*body) else {
            return;
        };
        let Some(target_position) = level.get_entity(target).map(|t| {
            behavior_utils::look_at_entity(body, t);
            t.position()
        }) else {
            return;
        };
        set_tongue_target(body, target);
        body.brain_mut()
            .set_memory(&WALK_TARGET, WalkTarget::from_vec3(target_position, 2.0, 0));
        self.calculate_path_counter = 10;
        self.state = State::MoveToTarget;
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&ATTACK_TARGET);
        erase_tongue_target(body);
        body.set_pose(Pose::Standing);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = attack_target(&*body) else {
            return;
        };
        set_tongue_target(body, target);
        match self.state {
            State::MoveToTarget => {
                let Some((close, target_position)) = level
                    .get_entity(target)
                    .map(|t| (distance_to(t, &*body) < EATING_DISTANCE, t.position()))
                else {
                    return;
                };
                if close {
                    let body_id = body.id();
                    let body_position = body.position();
                    level.play_sound_from_entity_at(
                        None,
                        body_id,
                        body_position,
                        self.tongue_sound,
                        SoundSource::Neutral,
                        2.0,
                        1.0,
                    );
                    body.set_pose(Pose::UsingTongue);
                    let movement = target_position
                        .vector_to(body_position)
                        .normalize()
                        .scale(EATING_MOVEMENT_FACTOR);
                    level.with_entity(target, |_, t| t.base_mut().set_delta_movement(movement));
                    self.eat_animation_timer = 0;
                    self.state = State::CatchAnimation;
                } else if self.calculate_path_counter <= 0 {
                    body.brain_mut()
                        .set_memory(&WALK_TARGET, WalkTarget::from_vec3(target_position, 2.0, 0));
                    self.calculate_path_counter = 10;
                } else {
                    self.calculate_path_counter -= 1;
                }
            }
            State::CatchAnimation => {
                let timer = self.eat_animation_timer;
                self.eat_animation_timer += 1;
                if timer >= CATCH_ANIMATION_DURATION {
                    self.state = State::EatAnimation;
                    self.eat_entity(level, body);
                }
            }
            State::EatAnimation => {
                if self.eat_animation_timer >= TONGUE_ANIMATION_DURATION {
                    self.state = State::Done;
                } else {
                    self.eat_animation_timer += 1;
                }
            }
            State::Done => {}
        }
    }
}
