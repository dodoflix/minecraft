//! `NearestBedSensor`: babies remember the closest reachable bed.

use super::sensor::Sensor;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::behavior::acquire_poi::find_path_to_pois;
use crate::world::entity::ai::behavior::village_util::java_poi_set_order;
use crate::world::entity::ai::memory::MemoryTypeId;
use crate::world::entity::ai::memory::memory_module_type::NEAREST_BED;
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypes};
use crate::world::entity::entity::Entity;
use std::collections::HashMap;

const CACHE_TIMEOUT: i64 = 40;
const BATCH_SIZE: i32 = 5;
const RATE: i32 = 20;

pub struct NearestBedSensor {
    batch_cache: HashMap<i64, i64>,
    tried_count: i32,
    last_update: i64,
}

impl NearestBedSensor {
    pub fn new() -> Self {
        Self {
            batch_cache: HashMap::new(),
            tried_count: 0,
            last_update: 0,
        }
    }
}

impl Default for NearestBedSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for NearestBedSensor {
    fn scan_rate(&self) -> i32 {
        RATE
    }

    fn requires(&self) -> Vec<MemoryTypeId> {
        vec![NEAREST_BED.id()]
    }

    fn do_tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        use crate::world::level::LevelAccessor as _;
        if !body.is_baby() {
            return;
        }
        self.tried_count = 0;
        self.last_update = level.game_time() + i64::from(level.random().next_int_bounded(20));
        let body_pos = body.block_position();
        let candidates = level.poi_manager().find_all_with_type(
            &|p| p.is(PoiTypes::HOME),
            &|_| true,
            body_pos,
            48,
            Occupancy::Any,
        );
        let mut pois = Vec::new();
        for (ty, pos) in candidates {
            let key = pos.as_long();
            if self.batch_cache.contains_key(&key) {
                continue;
            }
            self.tried_count += 1;
            if self.tried_count >= BATCH_SIZE {
                continue;
            }
            self.batch_cache
                .insert(key, self.last_update + CACHE_TIMEOUT);
            pois.push((ty, pos));
        }
        let pois = java_poi_set_order(pois);
        let path = find_path_to_pois(body, level, &pois);
        match path.filter(|p| p.can_reach()) {
            Some(path) => {
                let target_pos = path.get_target();
                if level.poi_manager().get_type(target_pos).is_some() {
                    body.brain_mut().set_memory(&NEAREST_BED, target_pos);
                }
            }
            None if self.tried_count < BATCH_SIZE => {
                let last = self.last_update;
                self.batch_cache.retain(|_, v| *v >= last);
            }
            None => {}
        }
    }
}
