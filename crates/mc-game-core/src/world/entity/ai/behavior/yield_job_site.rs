//! `YieldJobSite`: an unemployed villager hands a potential job site to a nearby villager that wants it.

use super::behavior_utils::set_walk_and_look_target_memories_pos;
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{
    JOB_SITE, LOOK_TARGET, NEAREST_LIVING_ENTITIES, POTENTIAL_JOB_SITE, WALK_TARGET,
};
use crate::world::entity::ai::village::poi::PoiTypeHolder;
use crate::world::entity::mob::navigation as nav;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::entity::npc::villager::{Villager, villager_mut, villager_ref};
use crate::world::entity::{Entity, EntityId};
use mc_core::pos::BlockPos;
use mc_data::VillagerProfession;
use mc_protocol::codec::GlobalPos;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let potential_job = i.present(&POTENTIAL_JOB_SITE);
    let _job_site = i.absent(&JOB_SITE);
    let nearest_entities = i.present(&NEAREST_LIVING_ENTITIES);
    let walk_target = i.registered(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    i.create(move |c| {
        if c.body().is_baby() {
            return false;
        }
        if villager_mut(c.body).get_villager_data().profession != VillagerProfession::NONE {
            return false;
        }
        let poi_pos = c.get(&potential_job).pos;
        let Some(poi_type) = c.level.poi_manager().get_type(poi_pos) else {
            return true;
        };
        let me = c.body().id();
        let nearby: Vec<EntityId> = c.get(&nearest_entities).iter().map(|r| r.0).collect();
        // `.filter(v instanceof Villager && v != body).filter(isAlive).filter(nearbyWantsJobsite).findFirst()`
        let mut found = None;
        for id in nearby {
            if id == me {
                continue;
            }
            let Some(v) = c
                .level
                .get_entity(id)
                .filter(|e| e.is_alive())
                .and_then(villager_ref)
            else {
                continue;
            };
            let wants = match nearby_wants_jobsite(poi_type, v, poi_pos) {
                Wants::No => false,
                Wants::Yes => true,
                Wants::IfReachable => {
                    let range = poi_type.value().valid_range;
                    c.level
                        .with_entity(id, |level, v| {
                            nav::create_path_pos(v, level, poi_pos, range)
                        })
                        .flatten()
                        .is_some_and(|p| p.can_reach())
                }
            };
            if wants {
                found = Some(id);
                break;
            }
        }
        if let Some(nearby_villager) = found {
            c.erase(&walk_target);
            c.erase(&look_target);
            c.erase(&potential_job);
            let dimension = c.level.dimension.clone();
            c.level.with_entity(nearby_villager, |_, v| {
                if v.brain().get_memory(&JOB_SITE).is_none() {
                    set_walk_and_look_target_memories_pos(v, poi_pos, speed_modifier, 1);
                    v.brain_mut().set_memory(
                        &POTENTIAL_JOB_SITE,
                        GlobalPos {
                            dimension,
                            pos: poi_pos,
                        },
                    );
                }
            });
        }
        true
    })
}

enum Wants {
    No,
    Yes,
    IfReachable,
}

/// `nearbyWantsJobsite(type, nearbyVillager, poiPos)` (the `canReachPos` pathfinding is left to the caller).
fn nearby_wants_jobsite(
    poi_type: PoiTypeHolder,
    nearby_villager: &Villager,
    poi_pos: BlockPos,
) -> Wants {
    if nearby_villager
        .brain()
        .get_memory(&POTENTIAL_JOB_SITE)
        .is_some()
    {
        return Wants::No;
    }
    let nearby_job_site = nearby_villager.brain().get_memory(&JOB_SITE);
    let profession = nearby_villager.get_villager_data().profession;
    if profession_data::data(profession)
        .held_job_site
        .test(poi_type)
    {
        match nearby_job_site {
            None => Wants::IfReachable,
            Some(site) if site.pos == poi_pos => Wants::Yes,
            Some(_) => Wants::No,
        }
    } else {
        Wants::No
    }
}
