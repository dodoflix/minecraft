//! `PlayTagWithOtherKids`.

use super::position_tracker::{EntityTracker, PositionTracker};
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::EntityId;
use crate::world::entity::ai::memory::memory_module_type::{
    INTERACTION_TARGET, LOOK_TARGET, VISIBLE_VILLAGER_BABIES, WALK_TARGET,
};
use crate::world::entity::ai::memory::{EntityRef, WalkTarget};
use crate::world::entity::ai::util::land_random_pos;
use crate::world::level::LevelAccessor as _;
use mc_core::pos::BlockPos;

const MAX_FLEE_XZ_DIST: i32 = 20;
const MAX_FLEE_Y_DIST: i32 = 8;
const FLEE_SPEED_MODIFIER: f32 = 0.6;
const CHASE_SPEED_MODIFIER: f32 = 0.6;
const MAX_CHASERS_PER_TARGET: i32 = 5;
const AVERAGE_WAIT_TIME_BETWEEN_RUNS: i32 = 10;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let babies = i.present(&VISIBLE_VILLAGER_BABIES);
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    let interaction_target = i.registered(&INTERACTION_TARGET);
    i.create(move |c| {
        if c.level
            .random()
            .next_int_bounded(AVERAGE_WAIT_TIME_BETWEEN_RUNS)
            != 0
        {
            return false;
        }
        let friends_nearby: Vec<EntityRef> = c.get(&babies).clone();
        let me = c.body().id();
        let other_kid_chasing_me = friends_nearby
            .iter()
            .any(|f| is_friend_chasing_me(c.level, me, f.0));
        if !other_kid_chasing_me {
            let chase = find_someone_being_chased(c.level, &friends_nearby)
                .or_else(|| friends_nearby.first().map(|f| f.0));
            if let Some(kid) = chase
                && let Some(entity) = c.level.get_entity(kid)
            {
                let look = PositionTracker::Entity(EntityTracker::new(entity, true));
                let walk = WalkTarget::new(
                    PositionTracker::Entity(EntityTracker::new(entity, false)),
                    CHASE_SPEED_MODIFIER,
                    1,
                );
                c.set(&interaction_target, EntityRef(kid));
                c.set(&look_target, look);
                c.set(&walk_target, walk);
            }
            true
        } else {
            for _ in 0..10 {
                let pos =
                    land_random_pos::get_pos(c.body, c.level, MAX_FLEE_XZ_DIST, MAX_FLEE_Y_DIST);
                if let Some(pos) = pos
                    && c.level
                        .is_village(BlockPos::containing(pos.x, pos.y, pos.z))
                {
                    c.set(
                        &walk_target,
                        WalkTarget::from_vec3(pos, FLEE_SPEED_MODIFIER, 0),
                    );
                    break;
                }
            }
            true
        }
    })
}

/// `findSomeoneBeingChased(friendsNearby)`: `checkHowManyChasersEachFriendHas` is a `HashMap<LivingEntity, Integer>` (identity
/// hashes: its iteration order is not reproducible); the stable sort by count keeps the first inserted among equals.
fn find_someone_being_chased(
    level: &crate::server::level::ServerLevel,
    friends_nearby: &[EntityRef],
) -> Option<EntityId> {
    let mut chased: Vec<(EntityId, i32)> = Vec::new();
    for friend in friends_nearby {
        if let Some(target) = whom_are_you_chasing(level, friend.0) {
            match chased.iter_mut().find(|(k, _)| *k == target) {
                Some(entry) => entry.1 += 1,
                None => chased.push((target, 1)),
            }
        }
    }
    chased.sort_by_key(|&(_, count)| count);
    chased
        .into_iter()
        .find(|&(_, count)| count > 0 && count <= MAX_CHASERS_PER_TARGET)
        .map(|(k, _)| k)
}

fn whom_are_you_chasing(
    level: &crate::server::level::ServerLevel,
    friend: EntityId,
) -> Option<EntityId> {
    level
        .get_entity(friend)?
        .brain()
        .get_memory(&INTERACTION_TARGET)
        .map(|r| r.0)
}

fn is_friend_chasing_me(
    level: &crate::server::level::ServerLevel,
    me: EntityId,
    friend: EntityId,
) -> bool {
    whom_are_you_chasing(level, friend) == Some(me)
}
