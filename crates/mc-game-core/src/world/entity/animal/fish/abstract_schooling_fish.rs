//! `net.minecraft.world.entity.animal.fish.AbstractSchoolingFish` + `net.minecraft.world.entity.ai.goal.FollowFlockLeaderGoal`.
//!
//! Java keeps `leader` (a fish reference) and `schoolSize`; here a leader is an `EntityId` plus a shared counter
//! (`Arc<AtomicI32>`) so followers can `leader.addFollower()` / `removeFollower()` without mutable access to another
//! entity (the leader may be taken out of the level or not yet added while `finalizeSpawn` runs).

use mc_core::random::RandomSource as _;
use crate::world::level::level_access::{BlockGetter as _, LevelAccessor as _};
use crate::world::level::material::FluidStateExt as _;
use super::abstract_fish::AbstractFish;
use crate::impl_spawn_group_data;
use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{Goal, GoalFlags, reduced_tick_delay};
use crate::world::entity::mob::{self, navigation};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_data::enums::EntitySpawnReason;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

/// The fields of `AbstractSchoolingFish`.
#[derive(Debug)]
pub struct SchoolData {
    leader: Option<(EntityId, Arc<AtomicI32>)>,
    school_size: Arc<AtomicI32>,
}

impl Default for SchoolData {
    fn default() -> Self {
        Self { leader: None, school_size: Arc::new(AtomicI32::new(1)) }
    }
}

/// `AbstractSchoolingFish` members a subclass supplies.
pub trait SchoolingFish: AbstractFish {
    fn school(&self) -> &SchoolData;
    fn school_mut(&mut self) -> &mut SchoolData;
    /// `getMaxSchoolSize()` (default `super.getMaxSpawnClusterSize()` = 8).
    fn get_max_school_size(&self) -> i32 {
        8
    }
}

/// `AbstractSchoolingFish.SchoolSpawnGroupData`.
pub struct SchoolSpawnGroupData {
    pub leader: EntityId,
    leader_size: Arc<AtomicI32>,
}
impl_spawn_group_data!(SchoolSpawnGroupData);

impl SchoolSpawnGroupData {
    pub fn new(leader: &dyn SchoolingFish, leader_id: EntityId) -> Self {
        Self { leader: leader_id, leader_size: leader.school().school_size.clone() }
    }
}

/// The school data of `e` if it is an `AbstractSchoolingFish` (the entity-side `instanceof`).
pub fn as_schooling(e: &dyn Entity) -> Option<&dyn SchoolingFish> {
    super::as_schooling_fish(e)
}

impl SchoolData {
    /// `hasFollowers()`.
    pub fn has_followers(&self) -> bool {
        self.school_size.load(Ordering::Relaxed) > 1
    }

    /// `isFollower()`: a leader that is still alive.
    pub fn is_follower(&self, level: &ServerLevel) -> bool {
        self.leader.as_ref().is_some_and(|(id, _)| level.get_entity(*id).is_some_and(|l| l.is_alive()))
    }

    /// `canBeFollowed()`.
    pub fn can_be_followed(&self, max_school_size: i32) -> bool {
        self.has_followers() && self.school_size.load(Ordering::Relaxed) < max_school_size
    }

    /// `startFollowing(leader)`.
    pub fn start_following(&mut self, leader: EntityId, leader_size: Arc<AtomicI32>) {
        leader_size.fetch_add(1, Ordering::Relaxed);
        self.leader = Some((leader, leader_size));
    }

    /// `stopFollowing()`.
    pub fn stop_following(&mut self) {
        if let Some((_, size)) = self.leader.take() {
            size.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// `schoolSize = 1` reset of `tick()`.
    pub fn reset_school_size(&self) {
        self.school_size.store(1, Ordering::Relaxed);
    }

    pub fn leader(&self) -> Option<EntityId> {
        self.leader.as_ref().map(|(id, _)| *id)
    }

    fn size_handle(&self) -> Arc<AtomicI32> {
        self.school_size.clone()
    }
}

/// `canRandomSwim()`: `!isFollower()`.
pub fn can_random_swim(fish: &dyn SchoolingFish, level: &ServerLevel) -> bool {
    !fish.school().is_follower(level)
}

/// `AbstractSchoolingFish.tick()` (after `super.tick()`).
pub fn tick(fish: &mut dyn Entity, level: &mut ServerLevel) {
    let followers = as_schooling(fish).is_some_and(|f| f.school().has_followers());
    if followers && level.random().next_int_bounded(200) == 1 {
        let bb = fish.base().bounding_box().inflate(8.0, 8.0, 8.0);
        let ty = fish.get_type();
        let neighbors = level.get_entities_of_class_with_stand_in(fish, &bb, |e| e.get_type() == ty, |_| true);
        if neighbors.len() <= 1
            && let Some(f) = super::as_schooling_fish(fish)
        {
            f.school().reset_school_size();
        }
    }
}

/// `AbstractSchoolingFish.finalizeSpawn` (after `super.finalizeSpawn`, which the caller ran).
pub fn finalize_spawn_group(fish: &mut dyn SchoolingFish, fish_id: EntityId, group_data: Option<Box<dyn SpawnGroupData>>) -> Option<Box<dyn SpawnGroupData>> {
    match group_data {
        None => Some(Box::new(SchoolSpawnGroupData::new(fish, fish_id))),
        Some(data) => {
            let leader = data
                .as_any()
                .downcast_ref::<SchoolSpawnGroupData>()
                .map(|d| (d.leader, d.leader_size.clone()))
                .or_else(|| data.as_any().downcast_ref::<super::tropical_fish::TropicalFishGroupData>().map(|d| (d.school.leader, d.school.leader_size.clone())));
            if let Some((leader, size)) = leader {
                fish.school_mut().start_following(leader, size);
            }
            Some(data)
        }
    }
}

/// `Mob.finalizeSpawn` super chain for a schooling fish followed by [`finalize_spawn_group`].
pub fn finalize_spawn(
    fish: &mut (impl SchoolingFish + Entity),
    level: &mut dyn ServerLevelAccessor,
    difficulty: DifficultyInstance,
    reason: EntitySpawnReason,
    group_data: Option<Box<dyn SpawnGroupData>>,
) -> Option<Box<dyn SpawnGroupData>> {
    let group_data = mob::spawn::finalize_spawn(fish, level, difficulty, reason, group_data);
    let id = fish.id();
    finalize_spawn_group(fish, id, group_data)
}

/// `FollowFlockLeaderGoal`.
pub struct FollowFlockLeaderGoal {
    time_to_recalc_path: i32,
    next_start_tick: i32,
}

impl FollowFlockLeaderGoal {
    /// `FollowFlockLeaderGoal(mob)`: `nextStartTick = nextStartTick(mob)`.
    pub fn new(mob: &mut dyn Entity) -> Self {
        Self { time_to_recalc_path: 0, next_start_tick: Self::next_start_tick(mob) }
    }

    /// `nextStartTick(mob)`: `reducedTickDelay(200 + mob.getRandom().nextInt(200) % 20)`.
    fn next_start_tick(mob: &mut dyn Entity) -> i32 {
        reduced_tick_delay(200 + mob.base_mut().random.next_int_bounded(200) % 20)
    }
}

impl Goal for FollowFlockLeaderGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(me) = as_schooling(mob) else { return false };
        if me.school().has_followers() {
            return false;
        }
        if me.school().is_follower(level) {
            return true;
        }
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        self.next_start_tick = Self::next_start_tick(mob);
        let bb = mob.base().bounding_box().inflate(8.0, 8.0, 8.0);
        let ty = mob.get_type();
        // `fish.canBeFollowed() || !fish.isFollower()`; the list includes the mob itself (see
        // `get_entities_of_class_with_stand_in`)
        let candidates = level.get_entities_of_class_with_stand_in(
            mob,
            &bb,
            |e| e.get_type() == ty,
            |e| as_schooling(e).is_some_and(|f| f.school().can_be_followed(f.get_max_school_size()) || !f.school().is_follower(level)),
        );
        let leader_id = candidates
            .iter()
            .copied()
            .find(|&id| {
                if id == mob.id() { as_schooling(mob) } else { level.get_entity(id).and_then(as_schooling) }
                    .is_some_and(|f| f.school().can_be_followed(f.get_max_school_size()))
            })
            .unwrap_or(mob.id());
        let own_id = mob.id();
        // `leader.addFollowers(candidates.filter(!isFollower))`: `limit(max - schoolSize).filter(f != leader)`
        let (leader_size, leader_max, leader_handle) = {
            let leader = if leader_id == own_id { as_schooling(mob) } else { level.get_entity(leader_id).and_then(as_schooling) };
            let Some(leader) = leader else { return false };
            (leader.school().school_size.load(Ordering::Relaxed), leader.get_max_school_size(), leader.school().size_handle())
        };
        let non_followers: Vec<EntityId> = candidates
            .iter()
            .copied()
            .filter(|&id| {
                let f = if id == own_id { as_schooling(mob) } else { level.get_entity(id).and_then(as_schooling) };
                f.is_some_and(|f| !f.school().is_follower(level))
            })
            .collect();
        let limit = (leader_max - leader_size).max(0) as usize;
        for id in non_followers.into_iter().take(limit).filter(|&id| id != leader_id) {
            if id == own_id {
                if let Some(f) = super::as_schooling_fish_mut(mob) {
                    f.school_mut().start_following(leader_id, leader_handle.clone());
                }
            } else {
                level.with_entity(id, |_, e| {
                    if let Some(f) = super::as_schooling_fish_mut(e) {
                        f.school_mut().start_following(leader_id, leader_handle.clone());
                    }
                });
            }
        }
        as_schooling(mob).is_some_and(|f| f.school().is_follower(level))
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(me) = as_schooling(mob) else { return false };
        me.school().is_follower(level)
            && me.school().leader().and_then(|id| level.get_entity(id)).is_some_and(|leader| mob.base().distance_to_sqr_vec(leader.position()) <= 121.0)
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.time_to_recalc_path = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(f) = super::as_schooling_fish_mut(mob) {
            f.school_mut().stop_following();
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = self.adjusted_tick_delay(10);
            // `pathToLeader()`
            let Some(me) = as_schooling(mob) else { return };
            if me.school().is_follower(level)
                && let Some(leader) = me.school().leader()
            {
                if let Some(target_pos) = level.get_entity(leader).map(|l| l.block_position()) {
                    navigation::move_to_entity(mob, level, target_pos, 1.0);
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
