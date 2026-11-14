//! `net.minecraft.world.entity.animal.allay.AllayAi`: the brain of the allay (fetch the wanted item, bring the collected items
//! to the liked player or note block, stay close to them).

use super::allay::Allay;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::SoundEvent;
use mc_data::enums::GameType;
use mc_game_ai::world::entity::ai::behavior::animal_panic::AnimalPanic;
use mc_game_ai::world::entity::ai::behavior::behaviors::{CountDownCooldownTicks, boxed};
use mc_game_ai::world::entity::ai::behavior::go_and_give_items_to_target::GoAndGiveItemsToTarget;
use mc_game_ai::world::entity::ai::behavior::{
    random_stroll, set_entity_look_target_sometimes, stay_close_to_target,
};
use mc_game_core::advancements::triggers::CriteriaTriggers;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::container::Container;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::look_at_target_sink::LookAtTargetSink;
use mc_game_core::world::entity::ai::behavior::move_to_target_sink::MoveToTargetSink;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, EntityTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::behavior::swim::Swim;
use mc_game_core::world::entity::ai::behavior::{
    DoNothing, RunOne, go_to_wanted_item, set_walk_target_from_look_target,
};
use mc_game_core::world::entity::ai::brain::ActivityData;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ITEM_PICKUP_COOLDOWN_TICKS, LIKED_NOTEBLOCK_COOLDOWN_TICKS, LIKED_NOTEBLOCK_POSITION,
    LIKED_PLAYER, NEAREST_VISIBLE_WANTED_ITEM,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::npc::inventory_carrier::InventoryCarrier;
use mc_game_core::world::entity::schedule::Activity;
use mc_game_core::world::entity::{EntityId, ServerPlayer};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_protocol::codec::GlobalPos;
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;

/// `THROW_VELOCITY`.
fn throw_velocity() -> Vec3 {
    Vec3::new(f64::from(0.2_f32), f64::from(0.3_f32), f64::from(0.2_f32))
}
/// `ITEM_PICKUP_COOLDOWN_DURATION`.
const ITEM_PICKUP_COOLDOWN_DURATION: i32 = 60;
/// `GIVE_ITEM_TIMEOUT_DURATION`.
const GIVE_ITEM_TIMEOUT_DURATION: i32 = 20;
/// `TIME_TO_FORGET_NOTEBLOCK`.
const TIME_TO_FORGET_NOTEBLOCK: i32 = 600;
/// `DISTANCE_TO_WANTED_ITEM`.
const DISTANCE_TO_WANTED_ITEM: i32 = 32;
/// `MAX_NOTEBLOCK_DISTANCE`.
const MAX_NOTEBLOCK_DISTANCE: i32 = 1024;

/// `Allay.THROW_SOUND_PITCHES`.
const THROW_SOUND_PITCHES: [f32; 16] = [
    0.5625, 0.625, 0.75, 0.9375, 1.0, 1.0, 1.125, 1.25, 1.5, 1.875, 2.0, 2.25, 2.5, 3.0, 3.75, 4.0,
];

/// `AllayAi.getActivities()`.
pub fn get_activities(_body: &dyn Entity) -> Vec<ActivityData> {
    vec![init_core_activity(), init_idle_activity()]
}

/// `initCoreActivity()`.
fn init_core_activity() -> ActivityData {
    ActivityData::create(
        Activity::CORE,
        0,
        vec![
            boxed(Swim::new(0.8)),
            boxed(AnimalPanic::new(2.5)),
            boxed(LookAtTargetSink::new(45, 90)),
            boxed(MoveToTargetSink::new()),
            boxed(CountDownCooldownTicks::new(LIKED_NOTEBLOCK_COOLDOWN_TICKS)),
            boxed(CountDownCooldownTicks::new(ITEM_PICKUP_COOLDOWN_TICKS)),
        ],
    )
}

/// `initIdleActivity()`.
fn init_idle_activity() -> ActivityData {
    ActivityData::create(
        Activity::IDLE,
        0,
        vec![
            boxed(go_to_wanted_item::create_with(
                |_| true,
                1.75,
                true,
                DISTANCE_TO_WANTED_ITEM,
            )),
            boxed(GoAndGiveItemsToTarget::new(
                Arc::new(get_item_deposit_position),
                2.25,
                GIVE_ITEM_TIMEOUT_DURATION,
                Arc::new(throw_item),
                ITEM_PICKUP_COOLDOWN_TICKS,
                ITEM_PICKUP_COOLDOWN_DURATION,
                Arc::new(has_item_to_throw),
            )),
            boxed(stay_close_to_target::create(
                get_item_deposit_position,
                |allay| !has_wanted_item(allay),
                4,
                16,
                2.25,
            )),
            boxed(set_entity_look_target_sometimes::create(6.0, 30, 60)),
            boxed(RunOne::new(vec![
                (boxed(random_stroll::fly(1.0)), 2),
                (boxed(set_walk_target_from_look_target::create(1.0, 3)), 2),
                (boxed(DoNothing::new(30, 60)), 1),
            ])),
        ],
    )
}

/// `allay` as the `Allay` the brain belongs to.
fn allay(body: &mut dyn Entity) -> &mut Allay {
    body.as_any_mut()
        .downcast_mut::<Allay>()
        .expect("allay brain on an allay")
}

/// `hasItemToThrow(allay)`.
fn has_item_to_throw(body: &dyn Entity) -> bool {
    !body
        .as_any()
        .downcast_ref::<Allay>()
        .expect("allay brain on an allay")
        .get_inventory()
        .is_empty()
}

/// `updateActivity(body)`.
pub fn update_activity(body: &mut dyn Entity) {
    body.brain_mut()
        .set_active_activity_to_first_valid(&[Activity::IDLE]);
}

/// `hearNoteblock(allay, pos)`.
pub fn hear_noteblock(level: &ServerLevel, body: &mut dyn Entity, pos: BlockPos) {
    let global_pos = GlobalPos {
        dimension: level.dimension.clone(),
        pos,
    };
    let liked = body.brain().get_memory(&LIKED_NOTEBLOCK_POSITION).cloned();
    match liked {
        None => {
            let brain = body.brain_mut();
            brain.set_memory(&LIKED_NOTEBLOCK_POSITION, global_pos);
            brain.set_memory(&LIKED_NOTEBLOCK_COOLDOWN_TICKS, TIME_TO_FORGET_NOTEBLOCK);
        }
        Some(liked) if liked == global_pos => {
            body.brain_mut()
                .set_memory(&LIKED_NOTEBLOCK_COOLDOWN_TICKS, TIME_TO_FORGET_NOTEBLOCK);
        }
        Some(_) => {}
    }
}

/// `getItemDepositPosition(allay)`: the liked note block above, else the liked player.
fn get_item_deposit_position(
    level: &ServerLevel,
    body: &mut dyn Entity,
) -> Option<PositionTracker> {
    if let Some(position) = body.brain().get_memory(&LIKED_NOTEBLOCK_POSITION).cloned() {
        if should_deposit_items_at_liked_noteblock(level, body, &position) {
            return Some(PositionTracker::Block(BlockPosTracker::new(
                position.pos.above(),
            )));
        }
        body.brain_mut().erase_memory(&LIKED_NOTEBLOCK_POSITION);
    }
    get_liked_player_position_tracker(level, body)
}

/// `hasWantedItem(allay)`.
fn has_wanted_item(body: &dyn Entity) -> bool {
    body.brain().has_memory_value(NEAREST_VISIBLE_WANTED_ITEM)
}

/// `shouldDepositItemsAtLikedNoteblock(allay, brain, likedNoteblockPos)`.
fn should_deposit_items_at_liked_noteblock(
    level: &ServerLevel,
    body: &dyn Entity,
    liked_noteblock_pos: &GlobalPos,
) -> bool {
    is_close_enough(level, liked_noteblock_pos, body.block_position())
        && level
            .get_block_state(liked_noteblock_pos.pos)
            .is_block(mc_data::Block::NOTE_BLOCK)
        && body
            .brain()
            .has_memory_value(LIKED_NOTEBLOCK_COOLDOWN_TICKS)
}

/// `GlobalPos.isCloseEnough(level.dimension(), pos, 1024)`.
pub fn is_close_enough(level: &ServerLevel, global_pos: &GlobalPos, pos: BlockPos) -> bool {
    global_pos.dimension == level.dimension
        && global_pos.pos.dist_chessboard(&pos) <= MAX_NOTEBLOCK_DISTANCE
}

/// `getLikedPlayerPositionTracker(allay)`.
fn get_liked_player_position_tracker(
    level: &ServerLevel,
    body: &dyn Entity,
) -> Option<PositionTracker> {
    let player = get_liked_player(level, body)?;
    Some(PositionTracker::Entity(EntityTracker::new(
        level.get_entity(player)?,
        true,
    )))
}

/// `getLikedPlayer(allay)`: the liked player while it is a survival or creative player within 64 blocks.
pub fn get_liked_player(level: &ServerLevel, body: &dyn Entity) -> Option<EntityId> {
    let liked = *body.brain().get_memory(&LIKED_PLAYER)?;
    let player = level.get_entity_by_uuid(liked)?;
    let server_player = player.as_any().downcast_ref::<ServerPlayer>()?;
    let mode = server_player.game_mode.game_mode_for_player;
    if mode != GameType::Spectator
        && mc_game_core::world::entity::ai::sensing::closer_than(player, body, 64.0)
    {
        Some(player.id())
    } else {
        None
    }
}

/// `throwItem(level, thrower, targetPos)`.
fn throw_item(level: &mut ServerLevel, thrower: &mut dyn Entity, target_pos: Vec3) {
    let item = allay(thrower).get_inventory_mut().remove_item(0, 1);
    if item.is_empty() {
        return;
    }
    behavior_utils::throw_item_full(
        level,
        thrower,
        item.clone(),
        target_pos.add_xyz(0.0, 1.0, 0.0),
        throw_velocity(),
        0.2,
    );
    if let Some(player) = get_liked_player(level, thrower) {
        let pos = BlockPos::containing_position(&target_pos).below();
        level.with_entity(player, |level, entity| {
            if let Some(player) = entity.as_any_mut().downcast_mut::<ServerPlayer>() {
                CriteriaTriggers::ALLAY_DROP_ITEM_ON_BLOCK.trigger(level, player, pos, &item);
            }
        });
    }
    if level.game_time() % 7 == 0 && level.random().next_double() < 0.9 {
        let pitch = THROW_SOUND_PITCHES[level
            .random()
            .next_int_bounded(THROW_SOUND_PITCHES.len() as i32)
            as usize];
        level.play_sound_from_entity_at(
            None,
            thrower.id(),
            thrower.position(),
            SoundEvent::ENTITY_ALLAY_ITEM_THROWN,
            SoundSource::Neutral,
            1.0,
            pitch,
        );
    }
}
