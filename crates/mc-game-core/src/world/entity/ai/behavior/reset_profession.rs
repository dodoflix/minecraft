//! `ResetProfession`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::JOB_SITE;
use crate::world::entity::npc::villager::villager_data::VillagerDataExt;
use crate::world::entity::npc::villager::villager_mut;
use mc_data::VillagerProfession;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let _job_site = i.absent(&JOB_SITE);
    i.create(move |c| {
        let villager = villager_mut(c.body);
        let body_data = villager.get_villager_data();
        let can_be_fired = body_data.profession != VillagerProfession::NONE
            && body_data.profession != VillagerProfession::NITWIT;
        if can_be_fired && villager.get_villager_xp() == 0 && body_data.level <= 1 {
            let data = villager
                .get_villager_data()
                .with_profession(VillagerProfession::NONE);
            villager.set_villager_data(data);
            villager.refresh_brain(c.level);
            true
        } else {
            false
        }
    })
}
