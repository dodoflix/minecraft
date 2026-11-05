//! `net.minecraft.world.entity.monster.PatrollingMonster`: patrol state, the long distance patrol goal and the spawn rules.
//! The only subclasses are raiders, so the data lives in [`RaiderData::patrol`](crate::world::entity::raid::raider::RaiderData)
//! and every function takes the entity (reading `Entity::raider()`).

use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::mob::navigation;
use crate::world::entity::raid::raid::Raid;
use crate::world::entity::raid::raider::{self, RaiderData};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::{Entity, equipment::EquipmentSlot};
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;
use mc_nbt::{CompoundTag, Tag};
use mc_world::heightmap::HeightmapType;

/// The `PatrollingMonster` fields.
#[derive(Clone, Debug, Default)]
pub struct PatrolData {
    pub patrol_target: Option<BlockPos>,
    pub patrol_leader: bool,
    pub patrolling: bool,
}

fn data(e: &dyn Entity) -> &RaiderData {
    e.raider().expect("patrolling monster")
}

fn data_mut(e: &mut dyn Entity) -> &mut RaiderData {
    e.raider_mut().expect("patrolling monster")
}

/// `addAdditionalSaveData` of `PatrollingMonster` (after `Monster`'s).
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    let p = &data(e).patrol;
    if let Some(t) = p.patrol_target {
        out.put_int_array("patrol_target", vec![t.x(), t.y(), t.z()]);
    }
    out.put_boolean("PatrolLeader", p.patrol_leader);
    out.put_boolean("Patrolling", p.patrolling);
}

/// `readAdditionalSaveData` of `PatrollingMonster`.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    let p = &mut data_mut(e).patrol;
    p.patrol_target = match input.get("patrol_target") {
        Some(Tag::IntArray(v)) if v.len() == 3 => Some(BlockPos::new(v[0], v[1], v[2])),
        _ => None,
    };
    p.patrol_leader = input.get_boolean_or("PatrolLeader", false);
    p.patrolling = input.get_boolean_or("Patrolling", false);
}

/// `PatrollingMonster.finalizeSpawn` up to the `super` call (`Mob.finalizeSpawn`, which the caller runs).
pub fn finalize_spawn_before_super(e: &mut dyn Entity, level: &mut dyn ServerLevelAccessor, reason: EntitySpawnReason) {
    if reason != EntitySpawnReason::Patrol
        && reason != EntitySpawnReason::Event
        && reason != EntitySpawnReason::Structure
        && level.random().next_float() < 0.06
        && e.can_be_leader()
    {
        data_mut(e).patrol.patrol_leader = true;
    }
    if is_patrol_leader(e) {
        let banner = Raid::get_ominous_banner_instance(level.registry_access());
        crate::world::entity::mob::equipment::set_item_slot_spawn(e, EquipmentSlot::Head, banner);
        e.set_drop_chance(EquipmentSlot::Head, 2.0);
    }
    if reason == EntitySpawnReason::Patrol {
        data_mut(e).patrol.patrolling = true;
    }
}

/// `PatrollingMonster.finalizeSpawn(level, difficulty, reason, groupData)`.
pub fn finalize_spawn(e: &mut dyn Entity, level: &mut dyn ServerLevelAccessor, difficulty: DifficultyInstance, reason: EntitySpawnReason, group_data: Option<Box<dyn SpawnGroupData>>) -> Option<Box<dyn SpawnGroupData>> {
    finalize_spawn_before_super(e, level, reason);
    crate::world::entity::mob::spawn::finalize_spawn(e, level, difficulty, reason, group_data)
}

/// `checkPatrollingMonsterSpawnRules(type, level, reason, pos, random)`.
pub fn check_patrolling_monster_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    if level.get_brightness(crate::world::level::LightLayer::Block, pos) > 8 {
        false
    } else {
        crate::world::entity::spawn_predicates::check_any_light_monster_spawn_rules(ty, level, reason, pos, random)
    }
}

/// `removeWhenFarAway(distSqr)`.
pub fn remove_when_far_away(e: &dyn Entity, dist_sqr: f64) -> bool {
    !data(e).patrol.patrolling || dist_sqr > 16384.0
}

/// `setPatrolTarget(target)`.
pub fn set_patrol_target(e: &mut dyn Entity, target: BlockPos) {
    let p = &mut data_mut(e).patrol;
    p.patrol_target = Some(target);
    p.patrolling = true;
}

pub fn get_patrol_target(e: &dyn Entity) -> Option<BlockPos> {
    data(e).patrol.patrol_target
}

pub fn has_patrol_target(e: &dyn Entity) -> bool {
    data(e).patrol.patrol_target.is_some()
}

/// `setPatrolLeader(isLeader)`.
pub fn set_patrol_leader(e: &mut dyn Entity, is_leader: bool) {
    let p = &mut data_mut(e).patrol;
    p.patrol_leader = is_leader;
    p.patrolling = true;
}

pub fn is_patrol_leader(e: &dyn Entity) -> bool {
    data(e).patrol.patrol_leader
}

/// `findPatrolTarget()`.
pub fn find_patrol_target(e: &mut dyn Entity) {
    let pos = e.block_position();
    let random = &mut e.base_mut().random;
    let dx = -500 + random.next_int_bounded(1000);
    let dz = -500 + random.next_int_bounded(1000);
    let p = &mut data_mut(e).patrol;
    p.patrol_target = Some(BlockPos::new(pos.x().wrapping_add(dx), pos.y(), pos.z().wrapping_add(dz)));
    p.patrolling = true;
}

pub fn is_patrolling(e: &dyn Entity) -> bool {
    data(e).patrol.patrolling
}

pub fn set_patrolling(e: &mut dyn Entity, value: bool) {
    data_mut(e).patrol.patrolling = value;
}

/// `PatrollingMonster.LongDistancePatrolGoal`.
pub struct LongDistancePatrolGoal {
    speed_modifier: f64,
    leader_speed_modifier: f64,
    cooldown_until: i64,
}

impl LongDistancePatrolGoal {
    pub fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        LongDistancePatrolGoal { speed_modifier, leader_speed_modifier, cooldown_until: -1 }
    }

    fn find_patrol_companions(mob: &dyn Entity, level: &ServerLevel) -> Vec<i32> {
        let bb = mob.get_bounding_box().inflate_scalar(16.0);
        level.get_entities_of_class(&bb, |e| e.raider().is_some(), |e| e.can_join_patrol(level) && e.id() != mob.id())
    }

    fn move_randomly(&self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let pos = mob.block_position();
        let random = &mut mob.base_mut().random;
        let dx = -8 + random.next_int_bounded(16);
        let dz = -8 + random.next_int_bounded(16);
        let path_target = level.get_heightmap_pos(HeightmapType::MotionBlockingNoLeaves, BlockPos::new(pos.x().wrapping_add(dx), pos.y(), pos.z().wrapping_add(dz)));
        navigation::move_to(mob, level, f64::from(path_target.x()), f64::from(path_target.y()), f64::from(path_target.z()), self.speed_modifier)
    }
}

impl Goal for LongDistancePatrolGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let is_on_cooldown = level.game_time() < self.cooldown_until;
        is_patrolling(mob) && mob.get_target().is_none() && !mob.has_controlling_passenger() && has_patrol_target(mob) && !is_on_cooldown
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let patrol_leader = is_patrol_leader(mob);
        if navigation::is_done(mob, level) {
            let companions = Self::find_patrol_companions(mob, level);
            let patrol_target = get_patrol_target(mob).expect("canUse checked the patrol target");
            if is_patrolling(mob) && companions.is_empty() {
                set_patrolling(mob, false);
            } else if patrol_leader && patrol_target.closer_to_center_than(&mob.position(), 10.0) {
                find_patrol_target(mob);
            } else {
                let mut long_distance_target = Vec3::at_bottom_center_of(&patrol_target);
                let self_vector = mob.position();
                let distance = self_vector.subtract(long_distance_target);
                // `Vec3.yRot(90.0F)`: the angle is radians, 90 of them
                long_distance_target = distance.y_rot(90.0).scale(0.4).add(long_distance_target);
                let move_target = long_distance_target.subtract(self_vector).normalize().scale(10.0).add(self_vector);
                let mut path_target = BlockPos::containing_position(&move_target);
                path_target = level.get_heightmap_pos(HeightmapType::MotionBlockingNoLeaves, path_target);
                let speed = if patrol_leader { self.leader_speed_modifier } else { self.speed_modifier };
                if !navigation::move_to(mob, level, f64::from(path_target.x()), f64::from(path_target.y()), f64::from(path_target.z()), speed) {
                    self.move_randomly(mob, level);
                    self.cooldown_until = level.game_time() + 200;
                } else if patrol_leader {
                    for companion in companions {
                        level.with_entity(companion, |_, c| set_patrol_target(c, path_target));
                    }
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

#[allow(dead_code)]
fn _raider_marker() {
    let _ = raider::is_celebrating;
}
