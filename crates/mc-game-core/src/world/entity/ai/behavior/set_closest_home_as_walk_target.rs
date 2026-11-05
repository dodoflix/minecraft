//! `SetClosestHomeAsWalkTarget`: walk to the closest bed POI of a homeless villager.

use mc_core::random::RandomSource;
use super::acquire_poi::find_path_to_pois;
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{HOME, WALK_TARGET};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder, PoiTypes};
use crate::world::level::LevelAccessor as _;
use mc_core::pos::BlockPos;
use std::collections::HashMap;

const CACHE_TIMEOUT: i64 = 40;
const BATCH_SIZE: i32 = 5;
const RATE: i64 = 20;
const OK_DISTANCE_SQR: f64 = 4.0;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    let mut batch_cache: HashMap<i64, i64> = HashMap::new();
    let mut last_update: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    let _home = i.absent(&HOME);
    i.create(move |c| {
        if c.level.game_time() - last_update < RATE {
            return false;
        }
        let body_pos = c.body().block_position();
        let closest = c.level.poi_manager().find_closest(&|p| p.is(PoiTypes::HOME), body_pos, 48, Occupancy::Any);
        let Some(_closest) = closest.filter(|p| p.dist_sqr(&body_pos) > OK_DISTANCE_SQR) else { return false };
        let mut tried_count = 0;
        last_update = c.level.game_time() + i64::from(c.level.random().next_int_bounded(20));
        let candidates = c.level.poi_manager().find_all_with_type(&|p| p.is(PoiTypes::HOME), &|_| true, body_pos, 48, Occupancy::Any);
        let mut pois: Vec<(PoiTypeHolder, BlockPos)> = Vec::new();
        for (ty, pos) in candidates {
            let key = pos.as_long();
            if batch_cache.contains_key(&key) {
                continue;
            }
            tried_count += 1;
            if tried_count >= BATCH_SIZE {
                continue;
            }
            batch_cache.insert(key, last_update + CACHE_TIMEOUT);
            pois.push((ty, pos));
        }
        let path = find_path_to_pois(c.body, c.level, &pois);
        match path.filter(|p| p.can_reach()) {
            Some(path) => {
                let target_pos = path.get_target();
                if c.level.poi_manager().get_type(target_pos).is_some() {
                    c.set(&walk_target, WalkTarget::from_block_pos(target_pos, speed_modifier, 1));
                }
            }
            None if tried_count < BATCH_SIZE => batch_cache.retain(|_, v| *v >= last_update),
            None => {}
        }
        true
    })
}
