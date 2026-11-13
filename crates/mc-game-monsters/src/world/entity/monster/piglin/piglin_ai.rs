//! `PiglinAi`: the brain of the piglin (activities, sensing helpers, bartering, admiring, anger propagation, hunting).
//!
//! Attacker/target entities of the anger functions are passed as `&dyn Entity`: the entity that hurt a piglin is usually
//! the running (taken-out) entity, see [`mc_game_core::world::entity::mob::ext::with_causing_entity`]; entities that are
//! compared by distance are carried as `(id, position)` (`Located`) so that they need not resolve in the level.

use super::abstract_piglin::{can_hunt_entity, is_adult};
use super::piglin::Piglin;
use super::{
    remember_if_hoglin_was_killed, start_admiring_item_if_seen, start_hunting_hoglin,
    stop_admiring_if_item_too_far_away, stop_admiring_if_tired_of_trying_to_reach_item,
    stop_holding_item_if_no_longer_admiring,
};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{EntityType, Item, SoundEvent};
use mc_game_ai::world::entity::ai::behavior::set_entity_look_target_sometimes::Ticker;
use mc_game_ai::world::entity::ai::behavior::{
    back_up_if_too_close, crossbow_attack::CrossbowAttack, dismount_or_skip_mounting,
    go_to_target_location, melee_attack, mount, random_stroll,
    set_walk_target_from_attack_target_if_target_out_of_reach, spear_approach::SpearApproach,
    spear_attack::SpearAttack, spear_retreat::SpearRetreat, start_attacking,
    start_celebrating_if_target_dead, stop_attacking_if_target_invalid,
    stop_being_angry_if_target_dead,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::{
    BehaviorBuilder, BehaviorControl, DoNothing, GateBehavior, OneShot, RunOne, Trigger,
    go_to_wanted_item, interact_with, interact_with_door, set_entity_look_target,
    set_look_and_interact, set_walk_target_away_from, set_walk_target_from_look_target,
    trigger_gate,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::*;
use mc_game_core::world::entity::ai::memory::{
    EntityRef, MemoryData, MemoryModuleType, MemoryTypeId,
};
use mc_game_core::world::entity::ai::sensing::closer_than;
use mc_game_core::world::entity::ai::sensing::repellent_sensors::{
    is_loved_item, is_player_holding_loved_item, is_zombified,
};
use mc_game_core::world::entity::ai::sensing::sensor::{
    is_entity_attackable, is_entity_attackable_ignoring_line_of_sight,
};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::living_entity::damage::make_sound;
use mc_game_core::world::entity::mob::equipment as mob_equipment;
use mc_game_core::world::entity::mob::ext::take;
use mc_game_core::world::entity::mob::interact::with_hand_stack;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use uuid::Uuid;

pub const REPELLENT_DETECTION_RANGE_HORIZONTAL: i32 = 8;
pub const REPELLENT_DETECTION_RANGE_VERTICAL: i32 = 4;
/// `BARTERING_ITEM`.
pub const BARTERING_ITEM: Item = Item::GOLD_INGOT;
const ADMIRE_DURATION: i32 = 119;
const MAX_DISTANCE_TO_WALK_TO_ITEM: i32 = 9;
const MAX_TIME_TO_WALK_TO_ITEM: i32 = 200;
const HOW_LONG_TIME_TO_DISABLE_ADMIRE_WALKING_IF_CANT_REACH_ITEM: i32 = 200;
const CELEBRATION_TIME: i32 = 300;
pub const MAX_TIME_BETWEEN_HUNTS: i32 = 120;
/// `TIME_BETWEEN_HUNTS = TimeUtil.rangeOfSeconds(30, 120)` (inclusive tick bounds).
pub const TIME_BETWEEN_HUNTS: (i32, i32) = (30 * 20, 120 * 20);
const BABY_FLEE_DURATION_AFTER_GETTING_HIT: i64 = 100;
const HIT_BY_PLAYER_MEMORY_TIMEOUT: i64 = 400;
const MAX_WALK_DISTANCE_TO_START_RIDING: i32 = 8;
/// `RIDE_START_INTERVAL = rangeOfSeconds(10, 40)`.
const RIDE_START_INTERVAL: (i32, i32) = (10 * 20, 40 * 20);
/// `RIDE_DURATION = rangeOfSeconds(10, 30)`.
const RIDE_DURATION: (i32, i32) = (10 * 20, 30 * 20);
/// `RETREAT_DURATION = rangeOfSeconds(5, 20)`.
const RETREAT_DURATION: (i32, i32) = (5 * 20, 20 * 20);
const MELEE_ATTACK_COOLDOWN: i32 = 20;
const EAT_COOLDOWN: i64 = 200;
const MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW: i32 = 5;
const SPEED_WHEN_STRAFING_BACK_FROM_TARGET: f32 = 0.75;
const DESIRED_DISTANCE_FROM_ZOMBIFIED: f64 = 6.0;
/// `AVOID_ZOMBIFIED_DURATION` / `BABY_AVOID_NEMESIS_DURATION = rangeOfSeconds(5, 7)`.
const AVOID_ZOMBIFIED_DURATION: (i32, i32) = (5 * 20, 7 * 20);
const BABY_AVOID_NEMESIS_DURATION: (i32, i32) = (5 * 20, 7 * 20);
const PROBABILITY_OF_CELEBRATION_DANCE: f32 = 0.1;
const ANGER_DURATION: i64 = 600;

/// An entity as far as distance comparisons are concerned (`BehaviorUtils.getNearestTarget`).
#[derive(Clone, Copy)]
struct Located {
    id: EntityId,
    pos: Vec3,
}

impl Located {
    fn of(e: &dyn Entity) -> Self {
        Located {
            id: e.id(),
            pos: e.position(),
        }
    }
}

/// `BehaviorUtils.getNearestTarget(body, currentTarget, newTarget)`.
fn get_nearest_target(body: &dyn Entity, current: Option<Located>, new: Located) -> Located {
    match current {
        // `getTargetNearestMe`
        Some(c)
            if body.base().distance_to_sqr_vec(c.pos)
                < body.base().distance_to_sqr_vec(new.pos) =>
        {
            c
        }
        _ => new,
    }
}

/// The entity a `Brain` entity memory refers to; `hint` stands in for an entity that is out of the level.
fn resolve_ref(
    level: &ServerLevel,
    memory: Option<EntityRef>,
    hint: &dyn Entity,
) -> Option<Located> {
    let EntityRef(id) = memory?;
    if id == hint.id() {
        return Some(Located::of(hint));
    }
    level.get_entity(id).map(Located::of)
}

fn item_tag(tag: &str, stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>(tag).is_some_and(|t| t.contains(stack.item()))
}

/// `UniformInt.sample(level.getRandom())` for inclusive tick bounds.
fn sample(level: &mut ServerLevel, (min, max): (i32, i32)) -> i32 {
    level.tick_state.random.next_int_between_inclusive(min, max)
}

// ---- brain construction ------------------------------------------------------------------------------------------

/// `getActivities(piglin)`.
pub fn get_activities(_piglin: &dyn Entity) -> Vec<ActivityData> {
    vec![
        init_core_activity(),
        init_idle_activity(),
        init_admire_item_activity(),
        init_fight_activity(),
        init_celebrate_activity(),
        init_retreat_activity(),
        init_ride_hoglin_activity(),
    ]
}

/// `initMemories(body, random)`.
pub fn init_memories(body: &mut dyn Entity, random: &mut dyn RandomSource) {
    let delay_until_first_hunt =
        random.next_int_between_inclusive(TIME_BETWEEN_HUNTS.0, TIME_BETWEEN_HUNTS.1);
    body.brain_mut().set_memory_with_expiry(
        &HUNTED_RECENTLY,
        true,
        i64::from(delay_until_first_hunt),
    );
}

fn b(behavior: impl BehaviorControl) -> Box<dyn BehaviorControl> {
    Box::new(behavior)
}

/// `EraseMemoryIf.create(predicate, memory)` with a level-aware predicate (`isNearZombified` resolves the entity).
fn erase_memory_if(
    mut predicate: impl FnMut(&ServerLevel, &dyn Entity) -> bool + 'static,
    memory_type: MemoryTypeId,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let memory = i.present_untyped(memory_type);
    i.create(move |c| {
        if predicate(&*c.level, &*c.body) {
            c.erase_untyped(&memory);
            true
        } else {
            false
        }
    })
}

/// `CopyMemoryWithExpiry.create(copyIfTrue, source, target, duration)` with a predicate that may use the level (and
/// its random).
fn copy_memory_with_expiry<U: MemoryData>(
    mut copy_if_true: impl FnMut(&mut ServerLevel, &dyn Entity) -> bool + 'static,
    source_memory: MemoryModuleType<U>,
    target_memory: MemoryModuleType<U>,
    (min, max): (i32, i32),
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let source = i.present(&source_memory);
    let target = i.absent(&target_memory);
    i.create(move |c| {
        if !copy_if_true(&mut *c.level, &*c.body) {
            return false;
        }
        let value = c.get(&source).clone();
        let duration = c
            .level
            .tick_state
            .random
            .next_int_between_inclusive(min, max);
        c.set_with_expiry(&target, value, i64::from(duration));
        true
    })
}

fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            b(LookAtTargetSink::new(45, 90)),
            b(MoveToTargetSink::new()),
            b(interact_with_door::create()),
            b(baby_avoid_nemesis()),
            b(avoid_zombified()),
            b(stop_holding_item_if_no_longer_admiring::create()),
            b(start_admiring_item_if_seen::create(ADMIRE_DURATION)),
            b(start_celebrating_if_target_dead::create(
                CELEBRATION_TIME,
                wants_to_dance,
            )),
            b(stop_being_angry_if_target_dead::create()),
        ],
    )
}

fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        10,
        vec![
            b(set_entity_look_target::create_with(
                is_player_holding_loved_item,
                14.0,
            )),
            b(start_attacking::create_with(
                |_, piglin| is_adult(piglin),
                find_nearest_valid_attack_target,
            )),
            b(BehaviorBuilder::trigger_if(
                can_hunt,
                start_hunting_hoglin::create(),
            )),
            b(avoid_repellent()),
            b(baby_sometimes_ride_baby_hoglin()),
            b(create_idle_look_behaviors()),
            b(create_idle_movement_behaviors()),
            b(set_look_and_interact::create(EntityType::PLAYER, 4)),
        ],
    )
}

fn init_fight_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::FIGHT,
        10,
        vec![
            b(stop_attacking_if_target_invalid::create_when(
                |level, body, target| !is_nearest_valid_attack_target(level, body, target),
            )),
            b(BehaviorBuilder::trigger_if(
                has_crossbow,
                back_up_if_too_close::create(
                    MIN_DESIRED_DIST_FROM_TARGET_WHEN_HOLDING_CROSSBOW,
                    SPEED_WHEN_STRAFING_BACK_FROM_TARGET,
                ),
            )),
            b(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            b(SpearApproach::new(1.0, 10.0)),
            b(SpearAttack::new(1.0, 1.0, 2.0)),
            b(SpearRetreat::new(1.0)),
            b(melee_attack::create(MELEE_ATTACK_COOLDOWN)),
            b(CrossbowAttack::new()),
            b(remember_if_hoglin_was_killed::create()),
            b(erase_memory_if(is_near_zombified, ATTACK_TARGET.id())),
        ],
        ATTACK_TARGET.id(),
    )
}

fn init_celebrate_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::CELEBRATE,
        10,
        vec![
            b(avoid_repellent()),
            b(set_entity_look_target::create_with(
                is_player_holding_loved_item,
                14.0,
            )),
            b(start_attacking::create_with(
                |_, piglin| is_adult(piglin),
                find_nearest_valid_attack_target,
            )),
            b(BehaviorBuilder::trigger_if(
                |piglin| !is_dancing(piglin),
                go_to_target_location::create(CELEBRATE_LOCATION, 2, 1.0),
            )),
            b(BehaviorBuilder::trigger_if(
                is_dancing,
                go_to_target_location::create(CELEBRATE_LOCATION, 4, 0.6),
            )),
            b(RunOne::new(vec![
                (
                    b(set_entity_look_target::create_type(EntityType::PIGLIN, 8.0)),
                    1,
                ),
                (b(random_stroll::stroll_within(0.6, 2, 1)), 1),
                (b(DoNothing::new(10, 20)), 1),
            ])),
        ],
        CELEBRATE_LOCATION.id(),
    )
}

fn init_admire_item_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::ADMIRE_ITEM,
        10,
        vec![
            b(go_to_wanted_item::create_with(
                is_not_holding_loved_item_in_off_hand,
                1.0,
                true,
                MAX_DISTANCE_TO_WALK_TO_ITEM,
            )),
            b(stop_admiring_if_item_too_far_away::create(
                MAX_DISTANCE_TO_WALK_TO_ITEM,
            )),
            b(stop_admiring_if_tired_of_trying_to_reach_item::create(
                MAX_TIME_TO_WALK_TO_ITEM,
                HOW_LONG_TIME_TO_DISABLE_ADMIRE_WALKING_IF_CANT_REACH_ITEM,
            )),
        ],
        ADMIRING_ITEM.id(),
    )
}

fn init_retreat_activity() -> ActivityData {
    ActivityData::create_with_erased_memory(
        Activity::AVOID,
        10,
        vec![
            b(set_walk_target_away_from::entity(
                AVOID_TARGET,
                1.0,
                12,
                true,
            )),
            b(create_idle_look_behaviors()),
            b(create_idle_movement_behaviors()),
            b(erase_memory_if(wants_to_stop_fleeing, AVOID_TARGET.id())),
        ],
        AVOID_TARGET.id(),
    )
}

fn init_ride_hoglin_activity() -> ActivityData {
    let mut triggers: Vec<(Box<dyn Trigger>, i32)> = create_look_behaviors()
        .into_iter()
        .map(|(t, w)| (Box::new(t) as Box<dyn Trigger>, w))
        .collect();
    triggers.push((Box::new(BehaviorBuilder::trigger_if_alone(|_| true)), 1));
    ActivityData::create_with_erased_memory(
        Activity::RIDE,
        10,
        vec![
            b(mount::create(0.8)),
            b(set_entity_look_target::create_with(
                is_player_holding_loved_item,
                8.0,
            )),
            b(BehaviorBuilder::sequence(
                BehaviorBuilder::trigger_if_alone(|e| e.base().is_passenger()),
                trigger_gate::trigger_one_shuffled(triggers),
            )),
            b(dismount_or_skip_mounting::create(
                MAX_WALK_DISTANCE_TO_START_RIDING,
                wants_to_stop_riding,
            )),
        ],
        RIDE_TARGET.id(),
    )
}

fn create_look_behaviors() -> Vec<(OneShot, i32)> {
    vec![
        (
            set_entity_look_target::create_type(EntityType::PLAYER, 8.0),
            1,
        ),
        (
            set_entity_look_target::create_type(EntityType::PIGLIN, 8.0),
            1,
        ),
        (set_entity_look_target::create(8.0), 1),
    ]
}

fn create_idle_look_behaviors() -> GateBehavior {
    let mut behaviors: Vec<(Box<dyn BehaviorControl>, i32)> = create_look_behaviors()
        .into_iter()
        .map(|(o, w)| (b(o), w))
        .collect();
    behaviors.push((b(DoNothing::new(30, 60)), 1));
    RunOne::new(behaviors)
}

fn create_idle_movement_behaviors() -> GateBehavior {
    RunOne::new(vec![
        (b(random_stroll::stroll(0.6)), 2),
        (
            b(interact_with::of(
                EntityType::PIGLIN,
                8,
                INTERACTION_TARGET,
                0.6,
                2,
            )),
            2,
        ),
        (
            b(BehaviorBuilder::trigger_if(
                does_not_see_any_player_holding_loved_item,
                set_walk_target_from_look_target::create(0.6, 3),
            )),
            2,
        ),
        (b(DoNothing::new(30, 60)), 1),
    ])
}

fn avoid_repellent() -> OneShot {
    set_walk_target_away_from::pos(NEAREST_REPELLENT, 1.0, 8, false)
}

fn baby_avoid_nemesis() -> OneShot {
    copy_memory_with_expiry(
        |_, e| e.is_baby(),
        NEAREST_VISIBLE_NEMESIS,
        AVOID_TARGET,
        BABY_AVOID_NEMESIS_DURATION,
    )
}

fn avoid_zombified() -> OneShot {
    copy_memory_with_expiry(
        |level, e| is_near_zombified(level, e),
        NEAREST_VISIBLE_ZOMBIFIED,
        AVOID_TARGET,
        AVOID_ZOMBIFIED_DURATION,
    )
}

fn baby_sometimes_ride_baby_hoglin() -> OneShot {
    let mut ticker = Ticker::new(RIDE_START_INTERVAL.0, RIDE_START_INTERVAL.1);
    copy_memory_with_expiry(
        move |level, e| e.is_baby() && ticker.tick_down_and_check(&mut level.tick_state.random),
        NEAREST_VISIBLE_BABY_HOGLIN,
        RIDE_TARGET,
        RIDE_DURATION,
    )
}

// ---- activity update ---------------------------------------------------------------------------------------------

/// `updateActivity(body)`.
pub fn update_activity(level: &mut ServerLevel, body: &mut Piglin) {
    let old_activity = body.brain().get_active_non_core_activity();
    body.brain_mut().set_active_activity_to_first_valid(&[
        Activity::ADMIRE_ITEM,
        Activity::FIGHT,
        Activity::AVOID,
        Activity::CELEBRATE,
        Activity::RIDE,
        Activity::IDLE,
    ]);
    let new_activity = body.brain().get_active_non_core_activity();
    if old_activity != new_activity
        && let Some(sound) = get_sound_for_current_activity(level, body)
    {
        make_sound(body, level, sound);
    }
    let attacking = body.brain().has_memory_value(ATTACK_TARGET);
    body.mob.set_aggressive(attacking);
    if !body.brain().has_memory_value(RIDE_TARGET) && is_baby_riding_baby(level, body) {
        level.stop_riding(body);
    }
    if !body.brain().has_memory_value(CELEBRATE_LOCATION) {
        body.brain_mut().erase_memory(&DANCING);
    }
    let dancing = body.brain().has_memory_value(DANCING);
    body.set_dancing(dancing);
}

fn is_baby_riding_baby(level: &ServerLevel, body: &Piglin) -> bool {
    if !body.is_baby() {
        return false;
    }
    body.base()
        .vehicle
        .and_then(|v| level.get_entity(v))
        .is_some_and(|vehicle| {
            matches!(vehicle.get_type(), EntityType::PIGLIN | EntityType::HOGLIN)
                && vehicle.is_baby()
        })
}

// ---- items -------------------------------------------------------------------------------------------------------

fn stop_walking(level: &mut ServerLevel, body: &mut dyn Entity) {
    body.brain_mut().erase_memory(&WALK_TARGET);
    mc_game_core::world::entity::mob::navigation::stop(body, level);
}

/// `pickUpItem(level, body, itemEntity)`.
pub fn pick_up_item(level: &mut ServerLevel, body: &mut Piglin, item_id: EntityId) {
    stop_walking(level, body);
    let Some(entity_stack) = level
        .get_entity(item_id)
        .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
        .map(|e| e.get_item())
    else {
        return;
    };
    let taken;
    if entity_stack.item() == Item::GOLD_NUGGET {
        take(level, &*body, item_id, entity_stack.count());
        taken = entity_stack;
        level.with_entity(item_id, |level, e| e.discard_in_level(level));
    } else {
        take(level, &*body, item_id, 1);
        taken = remove_one_item_from_item_entity(level, item_id);
    }
    if is_loved_item(&taken) {
        body.brain_mut()
            .erase_memory(&TIME_TRYING_TO_REACH_ADMIRE_ITEM);
        hold_in_off_hand(level, body, taken);
        admire_gold_item(body);
    } else if is_food(&taken) && !has_eaten_recently(body) {
        eat(body);
    } else {
        let item_equipped =
            !mob_equipment::equip_item_if_possible(body, level, taken.clone()).is_empty();
        if !item_equipped {
            put_in_inventory(level, body, taken);
        }
    }
}

fn hold_in_off_hand(level: &mut ServerLevel, body: &mut Piglin, stack: ItemStack) {
    if is_holding_item_in_off_hand(body) {
        let off_hand = body.get_item_by_slot(EquipmentSlot::OffHand);
        mc_game_core::world::entity::item::spawn_at_location(level, &*body, off_hand, Vec3::ZERO);
    }
    body.hold_in_off_hand(level, stack);
}

fn remove_one_item_from_item_entity(level: &mut ServerLevel, item_id: EntityId) -> ItemStack {
    level
        .with_entity(item_id, |level, e| {
            let item = e
                .as_any_mut()
                .downcast_mut::<ItemEntity>()
                .expect("item entity");
            let mut source = item.get_item();
            let removed = source.split(1);
            if source.is_empty() {
                item.discard_in_level(level);
            } else {
                item.set_item_in_place(source);
            }
            removed
        })
        .unwrap_or_default()
}

/// `stopHoldingOffHandItem(level, body, barteringEnabled)`.
pub fn stop_holding_off_hand_item(
    level: &mut ServerLevel,
    body: &mut Piglin,
    bartering_enabled: bool,
) {
    let item_stack = body.get_item_by_slot(EquipmentSlot::OffHand);
    body.set_item_slot(level, EquipmentSlot::OffHand, ItemStack::empty());
    if is_adult(body) {
        let barter_currency = is_barter_currency(&item_stack);
        if bartering_enabled && barter_currency {
            let items = get_barter_response_items(level, body);
            throw_items(level, body, items);
        } else if !barter_currency {
            let equipped =
                !mob_equipment::equip_item_if_possible(body, level, item_stack.clone()).is_empty();
            if !equipped {
                put_in_inventory(level, body, item_stack);
            }
        }
    } else {
        let equipped =
            !mob_equipment::equip_item_if_possible(body, level, item_stack.clone()).is_empty();
        if !equipped {
            let main_hand_item = body.get_item_by_slot(EquipmentSlot::MainHand);
            if is_loved_item(&main_hand_item) {
                put_in_inventory(level, body, main_hand_item);
            } else {
                throw_items(level, body, vec![main_hand_item]);
            }
            body.hold_in_main_hand(level, item_stack);
        }
    }
}

/// `cancelAdmiring(level, body)`.
pub fn cancel_admiring(level: &mut ServerLevel, body: &mut Piglin) {
    if is_admiring_item(body) && !body.get_item_by_slot(EquipmentSlot::OffHand).is_empty() {
        let off_hand = body.get_item_by_slot(EquipmentSlot::OffHand);
        mc_game_core::world::entity::item::spawn_at_location(level, &*body, off_hand, Vec3::ZERO);
        body.set_item_slot(level, EquipmentSlot::OffHand, ItemStack::empty());
    }
}

fn put_in_inventory(level: &mut ServerLevel, body: &mut Piglin, item_stack: ItemStack) {
    let stuff_that_couldnt_fit = body.add_to_inventory(&item_stack);
    throw_items_toward_random_pos(level, body, vec![stuff_that_couldnt_fit]);
}

fn throw_items(level: &mut ServerLevel, body: &mut Piglin, item_stacks: Vec<ItemStack>) {
    let player = body
        .brain()
        .get_memory(&NEAREST_VISIBLE_PLAYER)
        .and_then(|r| level.get_entity(r.0))
        .map(|p| p.position());
    match player {
        Some(pos) => throw_items_toward_pos(level, body, item_stacks, pos),
        None => throw_items_toward_random_pos(level, body, item_stacks),
    }
}

fn throw_items_toward_random_pos(
    level: &mut ServerLevel,
    body: &mut Piglin,
    item_stacks: Vec<ItemStack>,
) {
    let pos = get_random_nearby_pos(level, body);
    throw_items_toward_pos(level, body, item_stacks, pos);
}

fn throw_items_toward_pos(
    level: &mut ServerLevel,
    body: &mut Piglin,
    item_stacks: Vec<ItemStack>,
    target_pos: Vec3,
) {
    if !item_stacks.is_empty() {
        mc_game_core::world::entity::mob::combat::swing(
            body,
            level,
            InteractionHand::OffHand,
            mc_game_core::world::entity::player::item_use::SWING_ANIMATION_DEFAULT,
        );
        for item_stack in item_stacks {
            behavior_utils::throw_item(
                level,
                &*body,
                item_stack,
                target_pos.add(Vec3::new(0.0, 1.0, 0.0)),
            );
        }
    }
}

/// `getBarterResponseItems(body)`: `gameplay/piglin_bartering` with the `PIGLIN_BARTER` parameter set.
fn get_barter_response_items(level: &mut ServerLevel, body: &Piglin) -> Vec<ItemStack> {
    use mc_game_core::world::level::storage::loot::{LootContextParamSets, LootParams, loading};
    let table = loading::get_loot_table(
        &level.registries,
        &Identifier::parse("minecraft:gameplay/piglin_bartering").expect("valid id"),
    );
    let this: &dyn Entity = body;
    let params = LootParams::builder(level)
        .this_entity(this)
        .create(&LootContextParamSets::PIGLIN_BARTER);
    table.get_random_items(&params)
}

fn get_random_nearby_pos(level: &mut ServerLevel, body: &mut Piglin) -> Vec3 {
    land_random_pos::get_pos(body, level, 4, 2).unwrap_or_else(|| body.position())
}

/// `wantsToDance(body, killedTarget)`.
fn wants_to_dance(level: &ServerLevel, _body: &dyn Entity, killed_target: &dyn Entity) -> bool {
    if killed_target.get_type() != EntityType::HOGLIN {
        return false;
    }
    mc_core::random::create_thread_local_instance(level.game_time()).next_float()
        < PROBABILITY_OF_CELEBRATION_DANCE
}

/// `wantsToPickup(body, itemStack)`.
pub fn wants_to_pickup(level: &ServerLevel, body: &Piglin, item_stack: &ItemStack) -> bool {
    if body.is_baby() && item_tag("ignored_by_piglin_babies", item_stack)
        || item_tag("piglin_repellents", item_stack)
        || is_admiring_disabled(body) && body.brain().has_memory_value(ATTACK_TARGET)
    {
        false
    } else if is_barter_currency(item_stack) {
        is_not_holding_loved_item_in_off_hand(body)
    } else {
        let has_space = body.can_add_to_inventory(item_stack);
        if item_stack.item() == Item::GOLD_NUGGET {
            has_space
        } else if is_food(item_stack) {
            !has_eaten_recently(body) && has_space
        } else if !is_loved_item(item_stack) {
            body.can_replace_current_item_stack(level, item_stack)
        } else {
            is_not_holding_loved_item_in_off_hand(body) && has_space
        }
    }
}

/// `wantsToStopRiding(body, entityBeingRidden)`.
fn wants_to_stop_riding(body: &dyn Entity, entity_being_ridden: &dyn Entity) -> bool {
    if entity_being_ridden.mob().is_none() {
        return false;
    }
    !entity_being_ridden.is_baby()
        || !entity_being_ridden.is_alive()
        || was_hurt_recently(body)
        || was_hurt_recently(entity_being_ridden)
        || (entity_being_ridden.get_type() == EntityType::PIGLIN
            && entity_being_ridden.base().vehicle.is_none())
}

// ---- targets -----------------------------------------------------------------------------------------------------

fn is_nearest_valid_attack_target(
    level: &ServerLevel,
    body: &dyn Entity,
    target: &dyn Entity,
) -> bool {
    find_nearest_valid_attack_target(level, body) == Some(target.id())
}

/// `isNearZombified(body)`.
fn is_near_zombified(level: &ServerLevel, body: &dyn Entity) -> bool {
    match body.brain().get_memory(&NEAREST_VISIBLE_ZOMBIFIED) {
        Some(&EntityRef(id)) => level
            .get_entity(id)
            .is_some_and(|zombified| closer_than(body, zombified, DESIRED_DISTANCE_FROM_ZOMBIFIED)),
        None => false,
    }
}

/// `findNearestValidAttackTarget(level, body)`.
pub fn find_nearest_valid_attack_target(
    level: &ServerLevel,
    body: &dyn Entity,
) -> Option<EntityId> {
    if is_near_zombified(level, body) {
        return None;
    }
    let brain = body.brain();
    let angry_at = behavior_utils::get_living_entity_from_uuid_memory(level, body, &ANGRY_AT);
    if let Some(id) = angry_at
        && level
            .get_entity(id)
            .is_some_and(|t| is_entity_attackable_ignoring_line_of_sight(level, body, t))
    {
        return Some(id);
    }
    if brain.has_memory_value(UNIVERSAL_ANGER)
        && let Some(player) = brain.get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYER)
    {
        return Some(player.0);
    }
    if let Some(nemesis) = brain.get_memory(&NEAREST_VISIBLE_NEMESIS) {
        return Some(nemesis.0);
    }
    let player_not_wearing_gold = brain.get_memory(&NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD)?;
    level
        .get_entity(player_not_wearing_gold.0)
        .filter(|p| is_entity_attackable(level, body, *p))
        .map(|_| player_not_wearing_gold.0)
}

/// `angerNearbyPiglins(level, player, onlyIfTheySeeThePlayer)`. `player` may be out of the level (the acting player).
pub fn anger_nearby_piglins(
    level: &mut ServerLevel,
    player: &dyn Entity,
    only_if_they_see_the_player: bool,
) {
    let bb = player.base().bounding_box().inflate_scalar(16.0);
    let piglins =
        level.get_entities_of_class(&bb, |e| e.get_type() == EntityType::PIGLIN, |_| true);
    for id in piglins {
        let angered = level.get_entity(id).is_some_and(|piglin| {
            is_idle(piglin)
                && (!only_if_they_see_the_player || can_see_entity(level, piglin, player))
        });
        if angered {
            level.with_entity(id, |level, piglin| {
                if level.universal_anger() {
                    set_anger_target_to_nearest_targetable_player_if_found(level, piglin, player);
                } else {
                    set_anger_target(level, piglin, player);
                }
            });
        }
    }
}

#[linkme::distributed_slice(mc_game_core::server::level::block_break::PIGLIN_ANGER)]
static ANGER_NEARBY_PIGLINS: fn(&mut ServerLevel, &dyn Entity, bool) = anger_nearby_piglins;

/// `BehaviorUtils.canSee(body, target)` for a target that may be out of the level.
fn can_see_entity(level: &ServerLevel, body: &dyn Entity, target: &dyn Entity) -> bool {
    body.brain()
        .get_memory(&NEAREST_VISIBLE_LIVING_ENTITIES)
        .is_some_and(|visible| visible.contains_entity(level, body, target))
}

/// `mobInteract(level, body, player, hand)`.
pub fn mob_interact(
    level: &mut ServerLevel,
    body: &mut Piglin,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let player_held = player.get_item_in_hand(hand);
    if can_admire(body, &player_held) {
        let taken = with_hand_stack(player, hand, |stack, player| {
            stack.consume_and_return(1, player.player.abilities.instabuild)
        });
        hold_in_off_hand(level, body, taken);
        admire_gold_item(body);
        stop_walking(level, body);
        InteractionResult::SUCCESS
    } else {
        InteractionResult::PASS
    }
}

/// `canAdmire(body, playerHeldItemStack)`.
pub fn can_admire(body: &Piglin, player_held_item_stack: &ItemStack) -> bool {
    !is_admiring_disabled(body)
        && !is_admiring_item(body)
        && is_adult(body)
        && is_barter_currency(player_held_item_stack)
}

/// `wasHurtBy(level, body, attacker)`.
pub fn was_hurt_by(level: &mut ServerLevel, body: &mut Piglin, attacker: &dyn Entity) {
    if attacker.get_type() == EntityType::PIGLIN {
        return;
    }
    if is_holding_item_in_off_hand(body) {
        stop_holding_off_hand_item(level, body, false);
    }
    let brain = body.brain_mut();
    brain.erase_memory(&CELEBRATE_LOCATION);
    brain.erase_memory(&DANCING);
    brain.erase_memory(&ADMIRING_ITEM);
    if attacker.get_type() == EntityType::PLAYER {
        brain.set_memory_with_expiry(&ADMIRING_DISABLED, true, HIT_BY_PLAYER_MEMORY_TIMEOUT);
    }
    if get_avoid_target_type(level, body, attacker).is_some_and(|ty| ty != attacker.get_type()) {
        body.brain_mut().erase_memory(&AVOID_TARGET);
    }
    if body.is_baby() {
        body.brain_mut().set_memory_with_expiry(
            &AVOID_TARGET,
            EntityRef(attacker.id()),
            BABY_FLEE_DURATION_AFTER_GETTING_HIT,
        );
        if is_entity_attackable_ignoring_line_of_sight(level, body, attacker) {
            broadcast_anger_target(level, body, attacker);
        }
    } else if attacker.get_type() == EntityType::HOGLIN && hoglins_outnumber_piglins(body) {
        set_avoid_target_and_dont_hunt_for_a_while(level, body, Located::of(attacker));
        broadcast_retreat(level, body, attacker);
    } else {
        maybe_retaliate(level, body, attacker);
    }
}

/// The type of the current avoid target (the attacker itself may be it, and is out of the level).
fn get_avoid_target_type(
    level: &ServerLevel,
    body: &dyn Entity,
    attacker: &dyn Entity,
) -> Option<EntityType> {
    let &EntityRef(id) = body.brain().get_memory(&AVOID_TARGET)?;
    if id == attacker.id() {
        Some(attacker.get_type())
    } else {
        level.get_entity(id).map(|e| e.get_type())
    }
}

/// `maybeRetaliate(level, body, attacker)` (`AbstractPiglin`).
pub fn maybe_retaliate(level: &mut ServerLevel, body: &mut dyn Entity, attacker: &dyn Entity) {
    if !body.brain().is_active(Activity::AVOID)
        && is_entity_attackable_ignoring_line_of_sight(level, body, attacker)
        && !behavior_utils::is_other_target_much_further_away_than_current_attack_target(
            level, body, attacker, 4.0,
        )
    {
        if attacker.get_type() == EntityType::PLAYER && level.universal_anger() {
            set_anger_target_to_nearest_targetable_player_if_found(level, body, attacker);
            broadcast_universal_anger(level, body);
        } else {
            set_anger_target(level, body, attacker);
            broadcast_anger_target(level, body, attacker);
        }
    }
}

/// `getSoundForCurrentActivity(body)`.
pub fn get_sound_for_current_activity(level: &ServerLevel, body: &Piglin) -> Option<SoundEvent> {
    body.brain()
        .get_active_non_core_activity()
        .map(|activity| get_sound_for_activity(level, body, activity))
}

fn get_sound_for_activity(level: &ServerLevel, body: &Piglin, activity: Activity) -> SoundEvent {
    if activity == Activity::FIGHT {
        SoundEvent::ENTITY_PIGLIN_ANGRY
    } else if body.is_converting(level)
        || activity == Activity::AVOID && is_near_avoid_target(level, body)
    {
        SoundEvent::ENTITY_PIGLIN_RETREAT
    } else if activity == Activity::ADMIRE_ITEM {
        SoundEvent::ENTITY_PIGLIN_ADMIRING_ITEM
    } else if activity == Activity::CELEBRATE {
        SoundEvent::ENTITY_PIGLIN_CELEBRATE
    } else if sees_player_holding_loved_item(body) {
        SoundEvent::ENTITY_PIGLIN_JEALOUS
    } else if is_near_repellent(body) {
        SoundEvent::ENTITY_PIGLIN_RETREAT
    } else {
        SoundEvent::ENTITY_PIGLIN_AMBIENT
    }
}

fn is_near_avoid_target(level: &ServerLevel, body: &Piglin) -> bool {
    body.brain()
        .get_memory(&AVOID_TARGET)
        .and_then(|r| level.get_entity(r.0))
        .is_some_and(|avoid| closer_than(avoid, body, 12.0))
}

/// `isWearingSafeArmor(entity)`.
pub use mc_game_core::world::entity::ai::sensing::repellent_sensors::is_wearing_safe_armor;

// ---- anger propagation -------------------------------------------------------------------------------------------

fn get_adult_piglins(body: &dyn Entity) -> Vec<EntityRef> {
    body.brain()
        .get_memory(&NEARBY_ADULT_PIGLINS)
        .cloned()
        .unwrap_or_default()
}

/// `getVisibleAdultPiglins(body)`.
fn get_visible_adult_piglins(body: &dyn Entity) -> Vec<EntityRef> {
    body.brain()
        .get_memory(&NEAREST_VISIBLE_ADULT_PIGLINS)
        .cloned()
        .unwrap_or_default()
}

/// `broadcastAngerTarget(level, body, target)`.
pub fn broadcast_anger_target(level: &mut ServerLevel, body: &dyn Entity, target: &dyn Entity) {
    for EntityRef(id) in get_adult_piglins(body) {
        level.with_entity(id, |level, piglin| {
            if !(target.get_type() == EntityType::HOGLIN
                && (!can_hunt_entity(piglin) || !target.can_be_hunted()))
            {
                set_anger_target_if_closer_than_current(level, piglin, target);
            }
        });
    }
}

/// `broadcastUniversalAnger(level, body)`.
pub fn broadcast_universal_anger(level: &mut ServerLevel, body: &dyn Entity) {
    for EntityRef(id) in get_adult_piglins(body) {
        level.with_entity(id, |level, piglin| {
            if let Some(EntityRef(player)) = get_nearest_visible_targetable_player(piglin) {
                level.with_entity(player, |level, player| {
                    set_anger_target(level, piglin, player)
                });
            }
        });
    }
}

/// `setAngerTarget(level, body, target)`.
pub fn set_anger_target(level: &mut ServerLevel, body: &mut dyn Entity, target: &dyn Entity) {
    if is_entity_attackable_ignoring_line_of_sight(level, body, target) {
        let brain = body.brain_mut();
        brain.erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        brain.set_memory_with_expiry(&ANGRY_AT, target.uuid(), ANGER_DURATION);
        if target.get_type() == EntityType::HOGLIN && can_hunt_entity(body) {
            dont_kill_any_more_hoglins_for_a_while(level, body);
        }
        if target.get_type() == EntityType::PLAYER && level.universal_anger() {
            body.brain_mut()
                .set_memory_with_expiry(&UNIVERSAL_ANGER, true, ANGER_DURATION);
        }
    }
}

/// `setAngerTargetToNearestTargetablePlayerIfFound(level, body, targetIfNoPlayerFound)`.
fn set_anger_target_to_nearest_targetable_player_if_found(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    target_if_no_player_found: &dyn Entity,
) {
    match get_nearest_visible_targetable_player(body) {
        Some(EntityRef(player)) if player == target_if_no_player_found.id() => {
            set_anger_target(level, body, target_if_no_player_found)
        }
        Some(EntityRef(player)) => {
            level.with_entity(player, |level, player| {
                set_anger_target(level, body, player)
            });
        }
        None => set_anger_target(level, body, target_if_no_player_found),
    }
}

/// `setAngerTargetIfCloserThanCurrent(level, body, newTarget)`.
fn set_anger_target_if_closer_than_current(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    new_target: &dyn Entity,
) {
    let current = get_anger_target(level, body, new_target);
    let nearest = get_nearest_target(body, current, Located::of(new_target));
    if current.is_none_or(|c| c.id != nearest.id) {
        // `nearest` is the new target whenever it differs from the current one
        set_anger_target(level, body, new_target);
    }
}

/// `getAngerTarget(body)`: `BehaviorUtils.getLivingEntityFromUUIDMemory(body, ANGRY_AT)`.
fn get_anger_target(level: &ServerLevel, body: &dyn Entity, hint: &dyn Entity) -> Option<Located> {
    let uuid: Uuid = *body.brain().get_memory(&ANGRY_AT)?;
    if uuid == hint.uuid() {
        return Some(Located::of(hint));
    }
    level
        .get_entity_by_uuid(uuid)
        .filter(|e| e.living().is_some())
        .map(Located::of)
}

/// `getAvoidTarget(body)`.
pub fn get_avoid_target<'a>(level: &'a ServerLevel, body: &dyn Entity) -> Option<&'a dyn Entity> {
    body.brain()
        .get_memory(&AVOID_TARGET)
        .and_then(|r| level.get_entity(r.0))
}

/// `getNearestVisibleTargetablePlayer(body)`.
pub fn get_nearest_visible_targetable_player(body: &dyn Entity) -> Option<EntityRef> {
    body.brain()
        .get_memory(&NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .copied()
}

/// `broadcastRetreat(body, target)`.
fn broadcast_retreat(level: &mut ServerLevel, body: &dyn Entity, target: &dyn Entity) {
    for EntityRef(id) in get_visible_adult_piglins(body) {
        level.with_entity(id, |level, abstract_piglin| {
            if abstract_piglin.get_type() == EntityType::PIGLIN {
                retreat_from_nearest_target(level, abstract_piglin, target);
            }
        });
    }
}

/// `retreatFromNearestTarget(body, newAvoidTarget)`.
fn retreat_from_nearest_target(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    new_avoid_target: &dyn Entity,
) {
    let avoid = resolve_ref(
        level,
        body.brain().get_memory(&AVOID_TARGET).copied(),
        new_avoid_target,
    );
    let mut nearest = get_nearest_target(body, avoid, Located::of(new_avoid_target));
    let attack = resolve_ref(
        level,
        body.brain().get_memory(&ATTACK_TARGET).copied(),
        new_avoid_target,
    );
    nearest = get_nearest_target(body, attack, nearest);
    set_avoid_target_and_dont_hunt_for_a_while(level, body, nearest);
}

/// `wantsToStopFleeing(body)`.
fn wants_to_stop_fleeing(level: &ServerLevel, body: &dyn Entity) -> bool {
    let Some(&EntityRef(id)) = body.brain().get_memory(&AVOID_TARGET) else {
        return true;
    };
    let Some(avoided) = level.get_entity(id) else {
        return false;
    };
    if avoided.get_type() == EntityType::HOGLIN {
        !hoglins_outnumber_piglins(body)
    } else if is_zombified(avoided) {
        !body
            .brain()
            .is_memory_value(&NEAREST_VISIBLE_ZOMBIFIED, &EntityRef(id))
    } else {
        false
    }
}

/// `hoglinsOutnumberPiglins(body)`.
fn hoglins_outnumber_piglins(body: &dyn Entity) -> bool {
    let piglin_count = body
        .brain()
        .get_memory(&VISIBLE_ADULT_PIGLIN_COUNT)
        .copied()
        .unwrap_or(0)
        + 1;
    let hoglin_count = body
        .brain()
        .get_memory(&VISIBLE_ADULT_HOGLIN_COUNT)
        .copied()
        .unwrap_or(0);
    hoglin_count > piglin_count
}

/// `setAvoidTargetAndDontHuntForAWhile(body, target)`.
fn set_avoid_target_and_dont_hunt_for_a_while(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    target: Located,
) {
    let duration = sample(level, RETREAT_DURATION);
    let brain = body.brain_mut();
    brain.erase_memory(&ANGRY_AT);
    brain.erase_memory(&ATTACK_TARGET);
    brain.erase_memory(&WALK_TARGET);
    brain.set_memory_with_expiry(&AVOID_TARGET, EntityRef(target.id), i64::from(duration));
    dont_kill_any_more_hoglins_for_a_while(level, body);
}

/// `dontKillAnyMoreHoglinsForAWhile(body)`.
pub fn dont_kill_any_more_hoglins_for_a_while(level: &mut ServerLevel, body: &mut dyn Entity) {
    let duration = sample(level, TIME_BETWEEN_HUNTS);
    body.brain_mut()
        .set_memory_with_expiry(&HUNTED_RECENTLY, true, i64::from(duration));
}

// ---- small predicates --------------------------------------------------------------------------------------------

fn eat(body: &mut Piglin) {
    body.brain_mut()
        .set_memory_with_expiry(&ATE_RECENTLY, true, EAT_COOLDOWN);
}

fn has_eaten_recently(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(ATE_RECENTLY)
}

/// `isIdle(body)`.
pub fn is_idle(body: &dyn Entity) -> bool {
    body.brain().is_active(Activity::IDLE)
}

fn has_crossbow(body: &dyn Entity) -> bool {
    body.get_item_by_slot(EquipmentSlot::MainHand).item() == Item::CROSSBOW
        || body.get_item_by_slot(EquipmentSlot::OffHand).item() == Item::CROSSBOW
}

fn admire_gold_item(body: &mut dyn Entity) {
    body.brain_mut()
        .set_memory_with_expiry(&ADMIRING_ITEM, true, i64::from(ADMIRE_DURATION));
}

fn is_admiring_item(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(ADMIRING_ITEM)
}

fn is_barter_currency(item_stack: &ItemStack) -> bool {
    item_stack.item() == BARTERING_ITEM
}

fn is_food(item_stack: &ItemStack) -> bool {
    item_tag("piglin_food", item_stack)
}

fn is_near_repellent(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(NEAREST_REPELLENT)
}

fn sees_player_holding_loved_item(body: &dyn Entity) -> bool {
    body.brain()
        .has_memory_value(NEAREST_PLAYER_HOLDING_WANTED_ITEM)
}

fn does_not_see_any_player_holding_loved_item(body: &dyn Entity) -> bool {
    !sees_player_holding_loved_item(body)
}

fn is_admiring_disabled(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(ADMIRING_DISABLED)
}

fn was_hurt_recently(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(HURT_BY)
}

fn is_holding_item_in_off_hand(body: &dyn Entity) -> bool {
    !body.get_item_by_slot(EquipmentSlot::OffHand).is_empty()
}

fn is_not_holding_loved_item_in_off_hand(body: &dyn Entity) -> bool {
    let off_hand = body.get_item_by_slot(EquipmentSlot::OffHand);
    off_hand.is_empty() || !is_loved_item(&off_hand)
}

fn is_dancing(e: &dyn Entity) -> bool {
    e.as_any()
        .downcast_ref::<Piglin>()
        .is_some_and(Piglin::is_dancing)
}

fn can_hunt(e: &dyn Entity) -> bool {
    e.as_any()
        .downcast_ref::<Piglin>()
        .is_some_and(Piglin::can_hunt_piglin)
}

/// `findNearbyAdultPiglins(brain)`.
pub use mc_game_core::world::entity::ai::sensing::repellent_sensors::find_nearby_adult_piglins;
