//! `VillagerMakeLove`: two willing villagers with a free bed make a baby.

use super::behavior_utils::{lock_gaze_and_walk_to_each_other, target_is_valid};
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, HOME, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use crate::world::entity::ai::village::poi::{PoiTypeHolder, PoiTypes};
use crate::world::entity::mob::ageable;
use crate::world::entity::mob::navigation as nav;
use crate::world::entity::npc::villager::{Villager, villager_mut};
use crate::world::level::LevelAccessor as _;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_protocol::codec::GlobalPos;

pub struct VillagerMakeLove {
    birth_timestamp: i64,
}

impl VillagerMakeLove {
    pub fn new() -> BehaviorBase<VillagerMakeLove> {
        BehaviorBase::with_duration(
            entry_condition![BREED_TARGET => ValuePresent, NEAREST_VISIBLE_LIVING_ENTITIES => ValuePresent],
            350,
            350,
            VillagerMakeLove { birth_timestamp: 0 },
        )
    }

    /// `isBreedingPossible(myBody)`.
    fn is_breeding_possible(&self, level: &ServerLevel, my_body: &dyn Entity) -> bool {
        let Some(&EntityRef(target)) = my_body.brain().get_memory(&BREED_TARGET) else {
            return false;
        };
        let Some(breed_target) = level
            .get_entity(target)
            .filter(|e| e.get_type() == EntityType::VILLAGER)
        else {
            return false;
        };
        target_is_valid(level, my_body, &BREED_TARGET, |e| {
            e.get_type() == EntityType::VILLAGER
        }) && my_body.can_breed()
            && breed_target.can_breed()
    }

    /// `takeVacantBed(level, body)`.
    fn take_vacant_bed(&self, level: &mut ServerLevel, body: &mut dyn Entity) -> Option<BlockPos> {
        let body_pos = body.block_position();
        // the filter runs while the POI manager is borrowed, the pathfinding needs the level: query first, then take
        let candidates = level.poi_manager().find_all_with_type(
            &|p| p.is(PoiTypes::HOME),
            &|_| true,
            body_pos,
            48,
            crate::world::entity::ai::village::poi::Occupancy::HasSpace,
        );
        let mut chosen = None;
        for (ty, pos) in candidates {
            if can_reach(level, body, pos, ty) {
                chosen = Some(pos);
                break;
            }
        }
        let pos = chosen?;
        level.poi_manager().take(
            &|p| p.is(PoiTypes::HOME),
            &|_, poi_pos| poi_pos == pos,
            pos,
            1,
        )
    }

    /// `breed(level, source, target)`.
    fn breed(
        &self,
        level: &mut ServerLevel,
        source: &mut dyn Entity,
        target: &mut dyn Entity,
    ) -> Option<crate::world::entity::EntityId> {
        let child = source.get_breed_offspring(level, target.id());
        let mut child = child?;
        ageable::set_age_in_level(source, level, 6000);
        ageable::set_age_in_level(target, level, 6000);
        ageable::set_age(child.as_mut(), -24000);
        let (x, y, z) = (source.base().x(), source.base().y(), source.base().z());
        child.snap_to(x, y, z, 0.0, 0.0);
        let id = child.id();
        level.add_fresh_entity_with_passengers(vec![child]);
        level.broadcast_entity_event(id, 12);
        Some(id)
    }

    /// `tryToGiveBirth(level, body, target)`.
    fn try_to_give_birth(
        &self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        target: &mut dyn Entity,
    ) {
        match self.take_vacant_bed(level, body) {
            None => {
                level.broadcast_entity_event(target.id(), 13);
                level.broadcast_entity_event(body.id(), 13);
            }
            Some(childs_bed) => match self.breed(level, body, target) {
                Some(child) => {
                    let global_bed_pos = GlobalPos {
                        dimension: level.dimension.clone(),
                        pos: childs_bed,
                    };
                    level.with_entity(child, |_, c| {
                        c.brain_mut().set_memory(&HOME, global_bed_pos)
                    });
                }
                None => {
                    let _ = level.poi_manager().release(childs_bed);
                }
            },
        }
    }
}

/// `canReach(body, poiPos, poiType)`.
fn can_reach(
    level: &mut ServerLevel,
    body: &mut dyn Entity,
    poi_pos: BlockPos,
    poi_type: PoiTypeHolder,
) -> bool {
    let path = nav::create_path_pos(body, level, poi_pos, poi_type.value().valid_range);
    path.is_some_and(|p| p.can_reach())
}

impl Behavior for VillagerMakeLove {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        self.is_breeding_possible(level, body)
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        timestamp: i64,
    ) -> bool {
        timestamp <= self.birth_timestamp && self.is_breeding_possible(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(&EntityRef(target_id)) = body.brain().get_memory(&BREED_TARGET) else {
            return;
        };
        level.with_entity(target_id, |_, target| {
            lock_gaze_and_walk_to_each_other(body, target, 0.5, 2)
        });
        level.broadcast_entity_event(target_id, 18);
        level.broadcast_entity_event(body.id(), 18);
        let duration = 275 + body.base_mut().random.next_int_bounded(50);
        self.birth_timestamp = timestamp + i64::from(duration);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(&EntityRef(target_id)) = body.brain().get_memory(&BREED_TARGET) else {
            return;
        };
        let body_pos = body.position();
        let Some(dist) = level
            .get_entity(target_id)
            .map(|t| t.base().distance_to_sqr_vec(body_pos))
        else {
            return;
        };
        if dist > 5.0 {
            return;
        }
        level.with_entity(target_id, |level, target| {
            lock_gaze_and_walk_to_each_other(body, target, 0.5, 2);
            if timestamp >= self.birth_timestamp {
                villager_mut(body).eat_and_digest_food();
                target
                    .as_any_mut()
                    .downcast_mut::<Villager>()
                    .expect("villager")
                    .eat_and_digest_food();
                self.try_to_give_birth(level, body, target);
            } else if body.base_mut().random.next_int_bounded(35) == 0 {
                level.broadcast_entity_event(target.id(), 12);
                level.broadcast_entity_event(body.id(), 12);
            }
        });
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&BREED_TARGET);
    }
}
