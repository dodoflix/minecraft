//! `AssignProfessionFromJobSite`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{JOB_SITE, POTENTIAL_JOB_SITE};
use crate::world::entity::npc::villager::villager_data::VillagerDataExt;
use crate::world::entity::npc::villager::villager_mut;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use mc_data::VillagerProfession;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let potential_job_site = i.present(&POTENTIAL_JOB_SITE);
    let job_site = i.registered(&JOB_SITE);
    i.create(move |c| {
        let pos = c.get(&potential_job_site).clone();
        if !pos.pos.closer_to_center_than(&c.body().position(), 2.0) {
            return false;
        }
        c.erase(&potential_job_site);
        c.set(&job_site, pos.clone());
        let id = c.body().id();
        c.level.broadcast_entity_event(id, 14);
        let villager = villager_mut(c.body);
        if villager.get_villager_data().profession != VillagerProfession::NONE {
            return true;
        }
        // `server.getLevel(pos.dimension())`: only the level the villager is in is reachable from here
        if pos.dimension == c.level.dimension {
            if let Some(poi_type) = c.level.poi_manager().get_type(pos.pos) {
                if let Some(profession) = VillagerProfession::iter()
                    .find(|&p| profession_data::data(p).held_job_site.test(poi_type))
                {
                    let villager = villager_mut(c.body);
                    let data = villager.get_villager_data().with_profession(profession);
                    villager.set_villager_data(data);
                    villager.refresh_brain(c.level);
                }
            }
        }
        true
    })
}
