//! `CopperGolemAi`: the brain of the copper golem (flee when hurt, carry items from copper chests to chests, look at players,
//! stroll while the transport rests).

use super::copper_golem::CopperGolem;
use mc_data::block::BlockState;
use mc_data::{EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::behavior::animal_panic::AnimalPanic;
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::transport_items_between_containers::{
    ContainerInteractionState, OnTargetReachedInteraction, TARGET_INTERACTION_TIME,
    TransportItemTarget, TransportItemsBetweenContainers,
};
use mc_game_ai::world::entity::ai::behavior::{random_stroll, set_entity_look_target_sometimes};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::ContainerUser;
use mc_game_core::world::block_entity::chest_block_entity::ChestBlockEntity;
use mc_game_core::world::entity::ai::behavior::interact_with_door;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::{DoNothing, RunOne};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::MemoryStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    GAZE_COOLDOWN_TICKS, TRANSPORT_ITEMS_COOLDOWN_TICKS, WALK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::level::BlockGetter as _;
use mc_game_core::world::level::block::state::BlockStateExt as _;
use mc_protocol::packets::play::CopperGolemState;
use std::sync::Arc;

/// `SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 1.5;
/// `SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
/// `TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS`.
const TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS: i32 = 32;
/// `TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS`.
const TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS: i32 = 8;
/// `TICK_TO_START_ON_REACHED_INTERACTION`.
const TICK_TO_START_ON_REACHED_INTERACTION: i32 = 1;
/// `TICK_TO_PLAY_ON_REACHED_SOUND`.
const TICK_TO_PLAY_ON_REACHED_SOUND: i32 = 9;
/// `ContainerUser.getContainerInteractionRange()` of the golem.
const CONTAINER_INTERACTION_RANGE: f64 = 3.0;

/// `TRANSPORT_ITEM_SOURCE_BLOCK`: `block.is(BlockTags.COPPER_CHESTS)`.
fn is_source_block(state: BlockState) -> bool {
    state.is_tag("copper_chests")
}

/// `TRANSPORT_ITEM_DESTINATION_BLOCK`: a chest or a trapped chest.
fn is_destination_block(state: BlockState) -> bool {
    state.is_block(mc_data::block::Block::CHEST)
        || state.is_block(mc_data::block::Block::TRAPPED_CHEST)
}

/// `CopperGolemAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity()]
}

/// `CopperGolemAi.updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::IDLE]);
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(interact_with_door::create()),
            boxed(CountDownCooldownTicks::new(GAZE_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(TRANSPORT_ITEMS_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    ActivityData::create_pairs(
        Activity::IDLE,
        vec![
            (
                0,
                boxed(TransportItemsBetweenContainers::new(
                    1.0,
                    is_source_block,
                    is_destination_block,
                    TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS,
                    TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS,
                    get_target_reached_interactions(),
                    on_travelling,
                    should_queue_for_target,
                )),
            ),
            (
                1,
                boxed(set_entity_look_target_sometimes::create_type(
                    EntityType::PLAYER,
                    6.0,
                    40,
                    80,
                )),
            ),
            (
                2,
                boxed(RunOne::with_entry_condition(
                    vec![
                        (WALK_TARGET.id(), MemoryStatus::ValueAbsent),
                        (
                            TRANSPORT_ITEMS_COOLDOWN_TICKS.id(),
                            MemoryStatus::ValuePresent,
                        ),
                    ],
                    vec![
                        (
                            boxed(random_stroll::stroll_within(
                                SPEED_MULTIPLIER_WHEN_IDLING,
                                2,
                                2,
                            )),
                            1,
                        ),
                        (boxed(DoNothing::new(30, 60)), 1),
                    ],
                )),
            ),
        ],
    )
}

/// `getTargetReachedInteractions()`.
fn get_target_reached_interactions() -> Vec<(ContainerInteractionState, OnTargetReachedInteraction)>
{
    vec![
        (
            ContainerInteractionState::PickupItem,
            on_reached_target_interaction(
                CopperGolemState::GettingItem,
                Some(SoundEvent::ENTITY_COPPER_GOLEM_NO_ITEM_GET),
            ),
        ),
        (
            ContainerInteractionState::PickupNoItem,
            on_reached_target_interaction(
                CopperGolemState::GettingNoItem,
                Some(SoundEvent::ENTITY_COPPER_GOLEM_NO_ITEM_NO_GET),
            ),
        ),
        (
            ContainerInteractionState::PlaceItem,
            on_reached_target_interaction(
                CopperGolemState::DroppingItem,
                Some(SoundEvent::ENTITY_COPPER_GOLEM_ITEM_DROP),
            ),
        ),
        (
            ContainerInteractionState::PlaceNoItem,
            on_reached_target_interaction(
                CopperGolemState::DroppingNoItem,
                Some(SoundEvent::ENTITY_COPPER_GOLEM_ITEM_NO_DROP),
            ),
        ),
    ]
}

/// `container.startOpen(copperGolem)` / `container.stopOpen(copperGolem)` on every chest of the target (a compound
/// container forwards to `container1`, then `container2`). The golem is out of the level while its brain runs, so it is
/// exposed for the game events and the opener count rechecks that look it up.
fn open_or_close(
    body: &dyn Entity,
    level: &mut ServerLevel,
    target: &TransportItemTarget,
    open: bool,
) {
    let user = ContainerUser {
        id: body.id(),
        spectator: false,
        interaction_range: CONTAINER_INTERACTION_RANGE,
    };
    level.expose(body, |level| {
        for pos in target.block_entity_positions() {
            level.with_block_entity(pos, |level, be| {
                if open {
                    be.start_open(level, &user);
                } else {
                    be.stop_open(level, &user);
                }
            });
        }
    });
}

/// `container.getEntitiesWithContainerOpen().contains(body)`: a compound container (double chest) does not override the
/// empty default of `Container`, so only a single chest can answer yes.
fn has_container_open(
    body: &dyn Entity,
    level: &mut ServerLevel,
    target: &TransportItemTarget,
) -> bool {
    if target.block_entity_positions().nth(1).is_some() {
        return false;
    }
    level.expose(body, |level| {
        target.block_entity_positions().any(|pos| {
            level
                .get_block_entity(pos)
                .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
                .is_some_and(|chest| {
                    chest
                        .entities_with_container_open(level)
                        .contains(&body.id())
                })
        })
    })
}

/// `onReachedTargetInteraction(state, sound)`.
fn on_reached_target_interaction(
    state: CopperGolemState,
    sound: Option<SoundEvent>,
) -> OnTargetReachedInteraction {
    Arc::new(move |body, level, target, ticks_since_reaching_target| {
        if !body.as_any().is::<CopperGolem>() {
            return;
        }
        if ticks_since_reaching_target == TICK_TO_START_ON_REACHED_INTERACTION {
            open_or_close(&*body, level, target, true);
            if let Some(golem) = body.as_any_mut().downcast_mut::<CopperGolem>() {
                golem.set_opened_chest_pos(target.pos);
                golem.set_state(state);
            }
        }
        if ticks_since_reaching_target == TICK_TO_PLAY_ON_REACHED_SOUND
            && let Some(sound) = sound
        {
            body.play_sound_default(level, sound);
        }
        if ticks_since_reaching_target == TARGET_INTERACTION_TIME {
            if has_container_open(&*body, level, target) {
                open_or_close(&*body, level, target, false);
            }
            if let Some(golem) = body.as_any_mut().downcast_mut::<CopperGolem>() {
                golem.clear_opened_chest_pos();
            }
        }
    })
}

/// `onTravelling()`.
fn on_travelling(body: &mut dyn Entity) {
    if let Some(golem) = body.as_any_mut().downcast_mut::<CopperGolem>() {
        golem.clear_opened_chest_pos();
        golem.set_state(CopperGolemState::Idle);
    }
}

/// `shouldQueueForTarget()`: another user has the chest open.
fn should_queue_for_target(level: &ServerLevel, target: &TransportItemTarget) -> bool {
    level
        .get_block_entity(target.pos)
        .and_then(|be| be.as_any().downcast_ref::<ChestBlockEntity>())
        .is_some_and(|chest| !chest.entities_with_container_open(level).is_empty())
}
