//! `CreakingAi`.

use super::creaking::{ATTACK_INTERVAL, Creaking, SPEED_MULTIPLIER_WHEN_IDLING, can_move};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::behavior::behaviors::boxed;
use crate::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use crate::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use crate::world::entity::ai::behavior::swim::Swim;
use crate::world::entity::ai::behavior::{
    Behavior, BehaviorBase, DoNothing, RunOne, melee_attack, random_stroll, set_entity_look_target_sometimes, set_walk_target_from_attack_target_if_target_out_of_reach,
    set_walk_target_from_look_target, start_attacking, stop_attacking_if_target_invalid,
};
use crate::world::entity::ai::brain::ActivityData;
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, NEAREST_VISIBLE_ATTACKABLE_PLAYER, NEAREST_VISIBLE_ATTACKABLE_PLAYERS};
use crate::world::entity::ai::memory::{EntityRef, MemoryStatus};
use crate::world::entity::schedule::Activity;
use crate::world::entity::{Entity, EntityId};

/// `new Swim<Creaking>(0.8F) { checkExtraStartConditions: body.canMove() && super... }`.
struct CreakingSwim(Swim);

impl Behavior for CreakingSwim {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        can_move(body) && self.0.check_extra_start_conditions(level, body)
    }
    fn can_still_use(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) -> bool {
        // `Swim.canStillUse` is the (overridden) `checkExtraStartConditions`
        let _ = timestamp;
        self.check_extra_start_conditions(level, body)
    }
    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        self.0.tick(level, body, timestamp);
    }
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(BehaviorBase::new(entry_condition![], CreakingSwim(Swim::new(0.8).inner))),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
        ],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            boxed(start_attacking::create_with(
                |_level: &ServerLevel, mob: &dyn Entity| mob.as_any().downcast_ref::<Creaking>().is_some_and(Creaking::is_active),
                |_level: &ServerLevel, mob: &dyn Entity| mob.brain().get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYER).map(|r| r.0),
            )),
            boxed(set_entity_look_target_sometimes::create(8.0, 30, 60)),
            boxed(RunOne::new(vec![
                (boxed(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)), 2),
                (boxed(set_walk_target_from_look_target::create(SPEED_MULTIPLIER_WHEN_IDLING, 3)), 2),
                (boxed(DoNothing::new(30, 60)), 1),
            ])),
        ],
    )
}

fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_conditions(
        Activity::FIGHT,
        10,
        vec![
            boxed(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            boxed(melee_attack::create_with(|body: &dyn Entity| can_move(body), ATTACK_INTERVAL)),
            boxed(stop_attacking_if_target_invalid::create_when(|_level: &ServerLevel, body: &dyn Entity, target: &dyn Entity| !is_attack_target_still_reachable(body, target.id()))),
        ],
        vec![(ATTACK_TARGET.id(), MemoryStatus::ValuePresent)],
    )
}

/// `isAttackTargetStillReachable(creaking, target)`.
fn is_attack_target_still_reachable(creaking: &dyn Entity, target: EntityId) -> bool {
    creaking.brain().get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYERS).is_some_and(|players| players.contains(&EntityRef(target)))
}

/// `getActivities(creaking)`.
pub fn get_activities(_creaking: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity(), init_fight_activity()]
}

/// `updateActivity(creaking)`.
pub fn update_activity(creaking: &mut Creaking) {
    if !creaking.can_move() {
        creaking.brain_mut().use_default_activity();
    } else {
        creaking.brain_mut().set_active_activity_to_first_valid(&[Activity::FIGHT, Activity::IDLE]);
    }
}
