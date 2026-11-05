//! `PoiCompetitorScan`: of the villagers sharing a job site, the one with the most xp keeps it.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{JOB_SITE, NEAREST_LIVING_ENTITIES};
use crate::world::entity::ai::village::poi::PoiTypeHolder;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::entity::npc::villager::{villager_mut, villager_ref, Villager};
use crate::world::entity::EntityId;
use mc_data::EntityType;
use mc_protocol::codec::GlobalPos;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let job_site = i.present(&JOB_SITE);
    let nearest_entities = i.present(&NEAREST_LIVING_ENTITIES);
    i.create(move |c| {
        let pos = c.get(&job_site).clone();
        let nearest = c.get(&nearest_entities).clone();
        let me = c.body().id();
        if let Some(poi_type) = c.level.poi_manager().get_type(pos.pos) {
            // `.reduce(body, selectWinner)`: the running winner meets every competitor in order
            let competitors: Vec<EntityId> = nearest
                .iter()
                .map(|r| r.0)
                .filter(|&id| {
                    id != me
                        && c.level.get_entity(id).is_some_and(|e| {
                            e.get_type() == EntityType::VILLAGER && e.is_alive() && villager_ref(e).is_some_and(|v| competes_for_same_jobsite(&pos, poi_type, v))
                        })
                })
                .collect();
            let mut winner = me;
            let mut winner_xp = villager_mut(c.body).get_villager_xp();
            for other in competitors {
                let other_xp = c.level.get_entity(other).and_then(villager_ref).map_or(0, |v| v.get_villager_xp());
                // `first.getVillagerXp() > second.getVillagerXp()`: the first (the running winner) wins, else the second
                if winner_xp > other_xp {
                    erase_job_site(c, other);
                } else {
                    erase_job_site(c, winner);
                    winner = other;
                    winner_xp = other_xp;
                }
            }
        }
        true
    })
}

fn erase_job_site(c: &mut super::TriggerContext<'_>, id: EntityId) {
    if id == c.body().id() {
        c.body_mut().brain_mut().erase_memory(&JOB_SITE);
    } else {
        c.level.with_entity(id, |_, e| e.brain_mut().erase_memory(&JOB_SITE));
    }
}

fn competes_for_same_jobsite(pos: &GlobalPos, poi_type: PoiTypeHolder, nearby: &Villager) -> bool {
    use crate::world::entity::Entity;
    nearby.brain().get_memory(&JOB_SITE).is_some_and(|site| site == pos && profession_data::data(nearby.get_villager_data().profession).held_job_site.test(poi_type))
}
