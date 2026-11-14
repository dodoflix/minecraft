//! `WardenAi`: the warden's brain (activities, activity switching, dig cooldown, disturbance location).

use super::Warden;
use crate::world::entity::ai::behavior::warden::digging::Digging;
use crate::world::entity::ai::behavior::warden::emerging::Emerging;
use crate::world::entity::ai::behavior::warden::force_unmount::ForceUnmount;
use crate::world::entity::ai::behavior::warden::roar::Roar;
use crate::world::entity::ai::behavior::warden::sniffing::Sniffing;
use crate::world::entity::ai::behavior::warden::sonic_boom::SonicBoom;
use crate::world::entity::ai::behavior::warden::{
    set_roar_target, set_warden_look_target, try_to_sniff, warden, warden_mut,
};
use mc_core::pos::BlockPos;
use mc_data::Attribute;
use mc_game_ai::world::entity::ai::behavior::behaviors::boxed;
use mc_game_ai::world::entity::ai::behavior::{
    go_to_target_location, melee_attack, random_stroll,
    set_walk_target_from_attack_target_if_target_out_of_reach, stop_attacking_if_target_invalid,
};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::RunOne;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, EntityTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorBuilder, DoNothing, OneShot, look_at_target_sink::LookAtTargetSink,
    move_to_target_sink::MoveToTargetSink,
};
use mc_game_core::world::entity::ai::brain::{ActivityData, Brain};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, DIG_COOLDOWN, DISTURBANCE_LOCATION, IS_EMERGING, IS_SNIFFING, LOOK_TARGET,
    NEAREST_VISIBLE_LIVING_ENTITIES, ROAR_TARGET, SNIFF_COOLDOWN, WALK_TARGET,
};
use mc_game_core::world::entity::ai::memory::{MemoryStatus, Unit};
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::{Entity, EntityId};

const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.5;
const SPEED_MULTIPLIER_WHEN_INVESTIGATING: f32 = 0.7;
const SPEED_MULTIPLIER_WHEN_FIGHTING: f32 = 1.2;
const MELEE_ATTACK_COOLDOWN: i32 = 18;
/// `DIGGING_DURATION = Mth.ceil(100.0F)`.
const DIGGING_DURATION: i32 = 100;
/// `EMERGE_DURATION = Mth.ceil(133.59999F)`.
pub const EMERGE_DURATION: i32 = 134;
/// `ROAR_DURATION = Mth.ceil(84.0F)`.
pub const ROAR_DURATION: i32 = 84;
/// `SNIFFING_DURATION = Mth.ceil(83.2F)`.
const SNIFFING_DURATION: i32 = 84;
/// `DIGGING_COOLDOWN`.
pub const DIGGING_COOLDOWN: i32 = 1200;
const DISTURBANCE_LOCATION_EXPIRY_TIME: i64 = 100;

/// `DIG_COOLDOWN_SETTER`.
fn dig_cooldown_setter() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let cooldown = i.registered(&DIG_COOLDOWN);
    i.create(move |c| {
        if c.try_get(&cooldown).is_some() {
            c.set_with_expiry(&cooldown, Unit, i64::from(DIGGING_COOLDOWN));
        }
        true
    })
}

/// `getActivities(body)`.
pub fn get_activities(body: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_emerge_activity(),
        init_digging_activity(),
        init_idle_activity(),
        init_roar_activity(),
        init_fight_activity(body),
        init_investigate_activity(),
        init_sniffing_activity(),
    ]
}

/// `updateActivity(brain)`.
pub fn update_activity(brain: &mut Brain) {
    brain.set_active_activity_to_first_valid(&[
        Activity::EMERGE,
        Activity::DIG,
        Activity::ROAR,
        Activity::FIGHT,
        Activity::INVESTIGATE,
        Activity::SNIFF,
        Activity::IDLE,
    ]);
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(set_warden_look_target::create()),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
        ],
    )
}

fn init_emerge_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::EMERGE,
        5,
        vec![boxed(Emerging::new(EMERGE_DURATION))],
        IS_EMERGING.id(),
    )
}

fn init_digging_activity() -> ActivityData {
    ActivityData::create_pairs_with_conditions(
        Activity::DIG,
        vec![
            (0, boxed(ForceUnmount::new())),
            (1, boxed(Digging::new(DIGGING_DURATION))),
        ],
        vec![
            (ROAR_TARGET.id(), MemoryStatus::ValueAbsent),
            (DIG_COOLDOWN.id(), MemoryStatus::ValueAbsent),
        ],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            boxed(set_roar_target::create(Warden::get_entity_angry_at)),
            boxed(try_to_sniff::create()),
            boxed(RunOne::with_entry_condition(
                entry_condition![IS_SNIFFING => ValueAbsent],
                vec![
                    (
                        boxed(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
                        2,
                    ),
                    (boxed(DoNothing::new(30, 60)), 1),
                ],
            )),
        ],
    )
}

fn init_investigate_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::INVESTIGATE,
        5,
        vec![
            boxed(set_roar_target::create(Warden::get_entity_angry_at)),
            boxed(go_to_target_location::create(
                DISTURBANCE_LOCATION,
                2,
                SPEED_MULTIPLIER_WHEN_INVESTIGATING,
            )),
        ],
        DISTURBANCE_LOCATION.id(),
    )
}

fn init_sniffing_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::SNIFF,
        5,
        vec![
            boxed(set_roar_target::create(Warden::get_entity_angry_at)),
            boxed(Sniffing::new(SNIFFING_DURATION)),
        ],
        IS_SNIFFING.id(),
    )
}

fn init_roar_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::ROAR,
        10,
        vec![boxed(Roar::new())],
        ROAR_TARGET.id(),
    )
}

/// `SetEntityLookTarget.create(entity -> isTarget(body, entity), followRange)`: the predicate reads the warden's own
/// `ATTACK_TARGET`, so the behavior is written out here.
fn set_attack_target_look_target(max_dist: f32) -> OneShot {
    let max_dist_sqr = f64::from(max_dist * max_dist);
    let mut i = BehaviorBuilder::instance();
    let look_target = i.absent(&LOOK_TARGET);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let body: &dyn Entity = &*c.body;
        let attack_target = body.brain().get_memory(&ATTACK_TARGET).map(|r| r.0);
        let target = c
            .get(&nearest_entities)
            .find_closest(&*c.level, body, |mob| {
                Some(mob.id()) == attack_target
                    && mc_game_core::world::entity::ai::sensing::distance_to_sqr(mob, body)
                        <= max_dist_sqr
                    && !body.base().passengers.contains(&mob.id())
            });
        let Some(target) = target else { return false };
        let Some(tracker) = c
            .level
            .get_entity_or_recently_removed(target.0)
            .map(|e| EntityTracker::new(e, true))
        else {
            return false;
        };
        c.set(&look_target, PositionTracker::Entity(tracker));
        true
    })
}

fn init_fight_activity(body: &dyn Entity) -> ActivityData {
    let follow_range =
        body.living()
            .map_or(0.0, |l| l.get_attribute_value(Attribute::FOLLOW_RANGE)) as f32;
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        10,
        vec![
            boxed(dig_cooldown_setter()),
            boxed(stop_attacking_if_target_invalid::create_full(
                |level: &ServerLevel, body: &dyn Entity, target: &dyn Entity| {
                    let warden = warden(body);
                    !warden.get_anger_level(level).is_angry()
                        || !warden.can_target_entity_ref(level, target)
                },
                on_target_invalid,
                false,
            )),
            boxed(set_attack_target_look_target(follow_range)),
            boxed(
                set_walk_target_from_attack_target_if_target_out_of_reach::create(
                    SPEED_MULTIPLIER_WHEN_FIGHTING,
                ),
            ),
            boxed(SonicBoom::new()),
            boxed(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
        ],
        ATTACK_TARGET.id(),
    )
}

/// `onTargetInvalid(level, body, attackTarget)`.
fn on_target_invalid(level: &mut ServerLevel, body: &mut dyn Entity, attack_target: EntityId) {
    let warden = warden_mut(body);
    if !warden.can_target_id(level, Some(attack_target)) {
        warden.clear_anger(attack_target);
    }
    set_dig_cooldown(warden);
}

/// `setDigCooldown(body)`.
pub fn set_dig_cooldown(body: &mut dyn Entity) {
    if body.brain().has_memory_value(DIG_COOLDOWN) {
        body.brain_mut()
            .set_memory_with_expiry(&DIG_COOLDOWN, Unit, i64::from(DIGGING_COOLDOWN));
    }
}

/// `setDisturbanceLocation(body, position)`.
pub fn set_disturbance_location(level: &ServerLevel, body: &mut Warden, position: BlockPos) {
    if level.world_border().is_within_bounds_block(&position)
        && body.get_entity_angry_at(level).is_none()
        && body.brain().get_memory(&ATTACK_TARGET).is_none()
    {
        set_dig_cooldown(body);
        let brain = body.brain_mut();
        brain.set_memory_with_expiry(&SNIFF_COOLDOWN, Unit, 100);
        brain.set_memory_with_expiry(
            &LOOK_TARGET,
            PositionTracker::Block(BlockPosTracker::new(position)),
            DISTURBANCE_LOCATION_EXPIRY_TIME,
        );
        brain.set_memory_with_expiry(
            &DISTURBANCE_LOCATION,
            position,
            DISTURBANCE_LOCATION_EXPIRY_TIME,
        );
        brain.erase_memory(&WALK_TARGET);
    }
}
