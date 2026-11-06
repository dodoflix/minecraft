//! `VillagerGoalPackages`: the behavior lists of each villager activity.

use super::do_nothing::DoNothing;
use super::gate_behavior::{GateBehavior, OrderPolicy, RunOne, RunningPolicy};
use super::look_at_target_sink::LookAtTargetSink;
use super::village_util::{PoiPredicate, poi_is};
use super::{BehaviorBuilder, BehaviorControl, trigger_gate};
use super::{
    acquire_poi, assign_profession_from_job_site, celebrate_villagers_survived_raid,
    give_gift_to_hero, go_to_closest_village, go_to_potential_job_site, go_to_wanted_item,
    harvest_farmland, inside_brownian_walk, interact_with, interact_with_door, jump_on_bed,
    locate_hiding_place, look_and_follow_trading_player_sink, move_to_sky_seeing_spot,
    move_to_target_sink, play_tag_with_other_kids, poi_competitor_scan, react_to_bell,
    reset_profession, reset_raid_status, ring_bell, set_closest_home_as_walk_target,
    set_entity_look_target, set_hidden_state, set_look_and_interact, set_raid_status,
    set_walk_target_away_from, set_walk_target_from_block_memory, set_walk_target_from_look_target,
    show_trades_to_player, sleep_in_bed, socialize_at_bell, stroll_around_poi, stroll_to_poi,
    stroll_to_poi_list, swim, trade_with_villager, update_activity_from_schedule, use_bonemeal,
    validate_nearby_poi, village_bound_random_stroll, villager_calm_down, villager_make_love,
    villager_panic_trigger, wake_up, work_at_composter, work_at_poi, yield_job_site,
};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::goal::target::EntityClass;
use crate::world::entity::ai::memory::MemoryStatus;
use crate::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, HOME, HURT_BY_ENTITY, INTERACTION_TARGET, JOB_SITE, MEETING_POINT,
    NEAREST_HOSTILE, POTENTIAL_JOB_SITE, SECONDARY_JOB_SITE, VISIBLE_VILLAGER_BABIES,
};
use crate::world::entity::ai::village::poi::PoiTypes;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_data::enums::MobCategory;
use mc_data::{EntityType, VillagerProfession};
use std::sync::Arc;

pub const INTERACT_DIST_SQR: i32 = 5;
pub const INTERACT_WALKUP_DIST: i32 = 2;
pub const INTERACT_SPEED_MODIFIER: f32 = 0.5;

type Pairs = Vec<(i32, Box<dyn BehaviorControl>)>;

fn b(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

fn weighted(behavior: impl BehaviorControl, weight: i32) -> (Box<dyn BehaviorControl>, i32) {
    (Box::new(behavior), weight)
}

/// `getCorePackage(profession, speedModifier)`.
pub fn get_core_package(profession: VillagerProfession, speed_modifier: f32) -> Pairs {
    let data = profession_data::data(profession);
    let held: PoiPredicate = Arc::new(move |t| data.held_job_site.test(t));
    let acquirable: PoiPredicate = Arc::new(move |t| data.acquirable_job_site.test(t));
    vec![
        (0, b(swim::Swim::new(0.8))),
        (0, b(interact_with_door::create())),
        (0, b(LookAtTargetSink::new(45, 90))),
        (0, b(villager_panic_trigger::VillagerPanicTrigger::new())),
        (0, b(wake_up::create())),
        (0, b(react_to_bell::create())),
        (0, b(set_raid_status::create())),
        (0, b(validate_nearby_poi::create(held, JOB_SITE))),
        (
            0,
            b(validate_nearby_poi::create(
                acquirable.clone(),
                POTENTIAL_JOB_SITE,
            )),
        ),
        (1, b(move_to_target_sink::MoveToTargetSink::new())),
        (2, b(poi_competitor_scan::create())),
        (
            3,
            b(
                look_and_follow_trading_player_sink::LookAndFollowTradingPlayerSink::new(
                    speed_modifier,
                ),
            ),
        ),
        (5, b(go_to_wanted_item::create(speed_modifier, false, 4))),
        (
            6,
            b(acquire_poi::create_validated(
                acquirable,
                JOB_SITE,
                POTENTIAL_JOB_SITE,
                true,
                None,
                Arc::new(|_, _| true),
            )),
        ),
        (
            7,
            b(go_to_potential_job_site::GoToPotentialJobSite::new(
                speed_modifier,
            )),
        ),
        (8, b(yield_job_site::create(speed_modifier))),
        (
            10,
            b(acquire_poi::create(
                poi_is(PoiTypes::HOME),
                HOME,
                false,
                Some(14),
                Arc::new(validate_bed_poi),
            )),
        ),
        (
            10,
            b(acquire_poi::create_simple(
                poi_is(PoiTypes::MEETING),
                MEETING_POINT,
                true,
                Some(14),
            )),
        ),
        (10, b(assign_profession_from_job_site::create())),
        (10, b(reset_profession::create())),
    ]
}

/// `validateBedPoi(level, blockPos)`.
fn validate_bed_poi(level: &ServerLevel, block_pos: BlockPos) -> bool {
    let state = level.get_block_state(block_pos);
    state.is_tag("villagers_can_sleep_on_bed") && !state.get(mc_data::props::OCCUPIED)
}

/// `getWorkPackage(profession, speedModifier)`.
pub fn get_work_package(profession: VillagerProfession, speed_modifier: f32) -> Pairs {
    let is_farmer = profession == VillagerProfession::FARMER;
    let work_at_poi: Box<dyn BehaviorControl> = if is_farmer {
        b(work_at_composter::new())
    } else {
        b(work_at_poi::WorkAtPoi::new())
    };
    vec![
        get_minimal_look_behavior(),
        (
            5,
            b(RunOne::new(vec![
                (work_at_poi, 7),
                weighted(stroll_around_poi::create(JOB_SITE, 0.4, 4), 2),
                weighted(stroll_to_poi::create(JOB_SITE, 0.4, 1, 10), 5),
                weighted(
                    stroll_to_poi_list::create(SECONDARY_JOB_SITE, speed_modifier, 1, 6, JOB_SITE),
                    5,
                ),
                weighted(
                    harvest_farmland::HarvestFarmland::new(),
                    if is_farmer { 2 } else { 5 },
                ),
                weighted(
                    use_bonemeal::UseBonemeal::new(),
                    if is_farmer { 4 } else { 7 },
                ),
            ])),
        ),
        (
            10,
            b(show_trades_to_player::ShowTradesToPlayer::new(400, 1600)),
        ),
        (10, b(set_look_and_interact::create(EntityType::PLAYER, 4))),
        (
            2,
            b(set_walk_target_from_block_memory::create(
                JOB_SITE,
                speed_modifier,
                9,
                100,
                1200,
            )),
        ),
        (3, b(give_gift_to_hero::GiveGiftToHero::new(100))),
        (99, b(update_activity_from_schedule::create())),
    ]
}

/// `getPlayPackage(speedModifier)`.
pub fn get_play_package(speed_modifier: f32) -> Pairs {
    vec![
        (
            0,
            b(move_to_target_sink::MoveToTargetSink::with_timeout(80, 120)),
        ),
        get_full_look_behavior(),
        (5, b(play_tag_with_other_kids::create())),
        (
            5,
            b(RunOne::with_entry_condition(
                vec![(VISIBLE_VILLAGER_BABIES.id(), MemoryStatus::ValueAbsent)],
                vec![
                    weighted(
                        interact_with::of(
                            EntityType::VILLAGER,
                            8,
                            INTERACTION_TARGET,
                            speed_modifier,
                            2,
                        ),
                        2,
                    ),
                    weighted(
                        interact_with::of(
                            EntityType::CAT,
                            8,
                            INTERACTION_TARGET,
                            speed_modifier,
                            2,
                        ),
                        1,
                    ),
                    weighted(village_bound_random_stroll::create(speed_modifier), 1),
                    weighted(
                        set_walk_target_from_look_target::create(speed_modifier, 2),
                        1,
                    ),
                    weighted(jump_on_bed::JumpOnBed::new(speed_modifier), 2),
                    weighted(DoNothing::new(20, 40), 2),
                ],
            )),
        ),
        (99, b(update_activity_from_schedule::create())),
    ]
}

/// `getRestPackage(speedModifier)`.
pub fn get_rest_package(speed_modifier: f32) -> Pairs {
    vec![
        (
            2,
            b(set_walk_target_from_block_memory::create(
                HOME,
                speed_modifier,
                1,
                150,
                1200,
            )),
        ),
        (
            3,
            b(validate_nearby_poi::create(poi_is(PoiTypes::HOME), HOME)),
        ),
        (3, b(sleep_in_bed::SleepInBed::new())),
        (
            5,
            b(RunOne::with_entry_condition(
                vec![(HOME.id(), MemoryStatus::ValueAbsent)],
                vec![
                    weighted(set_closest_home_as_walk_target::create(speed_modifier), 1),
                    weighted(inside_brownian_walk::create(speed_modifier), 4),
                    weighted(go_to_closest_village::create(speed_modifier, 4), 2),
                    weighted(DoNothing::new(20, 40), 2),
                ],
            )),
        ),
        get_minimal_look_behavior(),
        (99, b(update_activity_from_schedule::create())),
    ]
}

/// `getMeetPackage(speedModifier)`.
pub fn get_meet_package(speed_modifier: f32) -> Pairs {
    vec![
        (
            2,
            b(trigger_gate::trigger_one_shuffled(vec![
                (
                    Box::new(stroll_around_poi::create(MEETING_POINT, 0.4, 40)),
                    2,
                ),
                (Box::new(socialize_at_bell::create()), 2),
            ])),
        ),
        (
            10,
            b(show_trades_to_player::ShowTradesToPlayer::new(400, 1600)),
        ),
        (10, b(set_look_and_interact::create(EntityType::PLAYER, 4))),
        (
            2,
            b(set_walk_target_from_block_memory::create(
                MEETING_POINT,
                speed_modifier,
                6,
                100,
                200,
            )),
        ),
        (3, b(give_gift_to_hero::GiveGiftToHero::new(100))),
        (
            3,
            b(validate_nearby_poi::create(
                poi_is(PoiTypes::MEETING),
                MEETING_POINT,
            )),
        ),
        (
            3,
            b(GateBehavior::new(
                Vec::new(),
                vec![INTERACTION_TARGET.id()],
                OrderPolicy::Ordered,
                RunningPolicy::RunOne,
                vec![weighted(trade_with_villager::TradeWithVillager::new(), 1)],
            )),
        ),
        get_full_look_behavior(),
        (99, b(update_activity_from_schedule::create())),
    ]
}

/// `getIdlePackage(speedModifier)`.
pub fn get_idle_package(speed_modifier: f32) -> Pairs {
    let can_breed: EntityClass = Arc::new(|e| e.can_breed());
    vec![
        (
            2,
            b(RunOne::new(vec![
                weighted(
                    interact_with::of(
                        EntityType::VILLAGER,
                        8,
                        INTERACTION_TARGET,
                        speed_modifier,
                        2,
                    ),
                    2,
                ),
                weighted(
                    interact_with::of_filtered(
                        EntityType::VILLAGER,
                        8,
                        can_breed.clone(),
                        can_breed,
                        BREED_TARGET,
                        speed_modifier,
                        2,
                    ),
                    1,
                ),
                weighted(
                    interact_with::of(EntityType::CAT, 8, INTERACTION_TARGET, speed_modifier, 2),
                    1,
                ),
                weighted(village_bound_random_stroll::create(speed_modifier), 1),
                weighted(
                    set_walk_target_from_look_target::create(speed_modifier, 2),
                    1,
                ),
                weighted(jump_on_bed::JumpOnBed::new(speed_modifier), 1),
                weighted(DoNothing::new(30, 60), 1),
            ])),
        ),
        (3, b(give_gift_to_hero::GiveGiftToHero::new(100))),
        (3, b(set_look_and_interact::create(EntityType::PLAYER, 4))),
        (
            3,
            b(show_trades_to_player::ShowTradesToPlayer::new(400, 1600)),
        ),
        (
            3,
            b(GateBehavior::new(
                Vec::new(),
                vec![INTERACTION_TARGET.id()],
                OrderPolicy::Ordered,
                RunningPolicy::RunOne,
                vec![weighted(trade_with_villager::TradeWithVillager::new(), 1)],
            )),
        ),
        (
            3,
            b(GateBehavior::new(
                Vec::new(),
                vec![BREED_TARGET.id()],
                OrderPolicy::Ordered,
                RunningPolicy::RunOne,
                vec![weighted(villager_make_love::VillagerMakeLove::new(), 1)],
            )),
        ),
        get_full_look_behavior(),
        (99, b(update_activity_from_schedule::create())),
    ]
}

/// `getPanicPackage(speedModifier)`.
pub fn get_panic_package(speed_modifier: f32) -> Pairs {
    let runaway_speed = speed_modifier * 1.5;
    vec![
        (0, b(villager_calm_down::create())),
        (
            1,
            b(set_walk_target_away_from::entity(
                NEAREST_HOSTILE,
                runaway_speed,
                6,
                false,
            )),
        ),
        (
            1,
            b(set_walk_target_away_from::entity(
                HURT_BY_ENTITY,
                runaway_speed,
                6,
                false,
            )),
        ),
        (
            3,
            b(village_bound_random_stroll::create_with(
                runaway_speed,
                2,
                2,
            )),
        ),
        get_minimal_look_behavior(),
    ]
}

/// `getPreRaidPackage(speedModifier)`.
pub fn get_pre_raid_package(speed_modifier: f32) -> Pairs {
    vec![
        (0, b(ring_bell::create())),
        (
            0,
            b(trigger_gate::trigger_one_shuffled(vec![
                (
                    Box::new(set_walk_target_from_block_memory::create(
                        MEETING_POINT,
                        speed_modifier * 1.5,
                        2,
                        150,
                        200,
                    )),
                    6,
                ),
                (
                    Box::new(village_bound_random_stroll::create(speed_modifier * 1.5)),
                    2,
                ),
            ])),
        ),
        get_minimal_look_behavior(),
        (99, b(reset_raid_status::create())),
    ]
}

/// `getRaidPackage(speedModifier)`.
pub fn get_raid_package(speed_modifier: f32) -> Pairs {
    vec![
        (
            0,
            b(BehaviorBuilder::sequence(
                BehaviorBuilder::trigger_if_level(raid_exists_and_not_victory),
                trigger_gate::trigger_one_shuffled(vec![
                    (Box::new(move_to_sky_seeing_spot::create(speed_modifier)), 5),
                    (
                        Box::new(village_bound_random_stroll::create(speed_modifier * 1.1)),
                        2,
                    ),
                ]),
            )),
        ),
        (
            0,
            b(celebrate_villagers_survived_raid::CelebrateVillagersSurvivedRaid::new(600, 600)),
        ),
        (
            2,
            b(BehaviorBuilder::sequence(
                BehaviorBuilder::trigger_if_level(raid_exists_and_active),
                locate_hiding_place::create(24, speed_modifier * 1.4, 1),
            )),
        ),
        get_minimal_look_behavior(),
        (99, b(reset_raid_status::create())),
    ]
}

/// `getHidePackage(speedModifier)`.
pub fn get_hide_package(speed_modifier: f32) -> Pairs {
    vec![
        (0, b(set_hidden_state::create(15, 3))),
        (
            1,
            b(locate_hiding_place::create(32, speed_modifier * 1.25, 2)),
        ),
        get_minimal_look_behavior(),
    ]
}

/// `getFullLookBehavior()`.
fn get_full_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    (
        5,
        b(RunOne::new(vec![
            weighted(set_entity_look_target::create_type(EntityType::CAT, 8.0), 8),
            weighted(
                set_entity_look_target::create_type(EntityType::VILLAGER, 8.0),
                2,
            ),
            weighted(
                set_entity_look_target::create_type(EntityType::PLAYER, 8.0),
                2,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::Creature, 8.0),
                1,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::WaterCreature, 8.0),
                1,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::Axolotls, 8.0),
                1,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::UndergroundWaterCreature, 8.0),
                1,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::WaterAmbient, 8.0),
                1,
            ),
            weighted(
                set_entity_look_target::create_category(MobCategory::Monster, 8.0),
                1,
            ),
            weighted(DoNothing::new(30, 60), 2),
        ])),
    )
}

/// `getMinimalLookBehavior()`.
fn get_minimal_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    (
        5,
        b(RunOne::new(vec![
            weighted(
                set_entity_look_target::create_type(EntityType::VILLAGER, 8.0),
                2,
            ),
            weighted(
                set_entity_look_target::create_type(EntityType::PLAYER, 8.0),
                2,
            ),
            weighted(DoNothing::new(30, 60), 8),
        ])),
    )
}

/// `raidExistsAndActive(level, body)`.
fn raid_exists_and_active(level: &mut ServerLevel, body: &dyn Entity) -> bool {
    level
        .get_raid_at(body.block_position())
        .is_some_and(|raid| raid.is_active() && !raid.is_victory() && !raid.is_loss())
}

/// `raidExistsAndNotVictory(level, body)`.
fn raid_exists_and_not_victory(level: &mut ServerLevel, body: &dyn Entity) -> bool {
    level
        .get_raid_at(body.block_position())
        .is_some_and(|raid| raid.is_victory())
}
