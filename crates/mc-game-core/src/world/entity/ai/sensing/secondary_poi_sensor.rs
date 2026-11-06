//! `SecondaryPoiSensor`: the blocks around a villager that its profession treats as secondary job sites.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::ai::memory::memory_module_type::SECONDARY_JOB_SITE;
use crate::world::entity::entity::Entity;
use crate::world::entity::npc::villager::villager_mut;
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::level::BlockGetter;
use mc_protocol::codec::GlobalPos;

const SCAN_RATE: i32 = 40;

pub struct SecondaryPoiSensor;

impl Sensor for SecondaryPoiSensor {
    fn scan_rate(&self) -> i32 {
        SCAN_RATE
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![SECONDARY_JOB_SITE.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        let dimension = level.dimension.clone();
        let center = body.block_position();
        let secondary =
            profession_data::data(villager_mut(body).get_villager_data().profession).secondary_poi;
        let mut job_sites: Vec<GlobalPos> = Vec::new();
        for x in -4..=4 {
            for y in -2..=2 {
                for z in -4..=4 {
                    let test_pos = center.offset(x, y, z);
                    if secondary.contains(&level.get_block_state(test_pos).block()) {
                        job_sites.push(GlobalPos {
                            dimension: dimension.clone(),
                            pos: test_pos,
                        });
                    }
                }
            }
        }
        let brain = body.brain_mut();
        if !job_sites.is_empty() {
            brain.set_memory(&SECONDARY_JOB_SITE, job_sites);
        } else {
            brain.erase_memory(&SECONDARY_JOB_SITE);
        }
    }
}
