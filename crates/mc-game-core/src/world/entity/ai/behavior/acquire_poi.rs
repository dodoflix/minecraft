//! `AcquirePoi`: find a free point of interest of a type, path to it, take a ticket and remember it.

use super::village_util::{PoiPredicate, ValidPoi, java_block_pos_set_order, java_poi_set_order};
use super::{BehaviorBuilder, OneShot};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::MemoryModuleType;
use crate::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder};
use crate::world::entity::mob::navigation as nav;
use crate::world::level::LevelAccessor as _;
use crate::world::level::pathfinder::Path;
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;
use std::collections::HashMap;
use std::sync::Arc;

pub const SCAN_RANGE: i32 = 48;

/// `create(poiType, memoryToAcquire, onlyIfAdult, onPoiAcquisitionEvent, validPoi)`.
pub fn create(
    poi_type: PoiPredicate,
    memory_to_acquire: MemoryModuleType<GlobalPos>,
    only_if_adult: bool,
    on_poi_acquisition_event: Option<i8>,
    valid_poi: ValidPoi,
) -> OneShot {
    create_validated(
        poi_type,
        memory_to_acquire,
        memory_to_acquire,
        only_if_adult,
        on_poi_acquisition_event,
        valid_poi,
    )
}

/// `create(poiType, memoryToAcquire, onlyIfAdult, onPoiAcquisitionEvent)`.
pub fn create_simple(
    poi_type: PoiPredicate,
    memory_to_acquire: MemoryModuleType<GlobalPos>,
    only_if_adult: bool,
    on_poi_acquisition_event: Option<i8>,
) -> OneShot {
    create(
        poi_type,
        memory_to_acquire,
        only_if_adult,
        on_poi_acquisition_event,
        Arc::new(|_, _| true),
    )
}

/// `create(poiType, memoryToValidate, memoryToAcquire, onlyIfAdult, onPoiAcquisitionEvent, validPoi)`.
pub fn create_validated(
    poi_type: PoiPredicate,
    memory_to_validate: MemoryModuleType<GlobalPos>,
    memory_to_acquire: MemoryModuleType<GlobalPos>,
    only_if_adult: bool,
    on_poi_acquisition_event: Option<i8>,
    valid_poi: ValidPoi,
) -> OneShot {
    let mut next_scheduled_start: i64 = 0;
    let mut batch_cache: HashMap<i64, JitteredLinearRetry> = HashMap::new();
    let mut i = BehaviorBuilder::instance();
    let _to_acquire = i.absent(&memory_to_acquire);
    let mut acquire_poi = i.create(move |c| {
        if only_if_adult && c.body().is_baby() {
            return false;
        }
        let timestamp = c.timestamp;
        let level: &mut ServerLevel = c.level;
        if next_scheduled_start == 0 {
            next_scheduled_start =
                level.game_time() + i64::from(level.random().next_int_bounded(20));
            return false;
        } else if level.game_time() < next_scheduled_start {
            return false;
        }
        next_scheduled_start = timestamp + 20 + i64::from(level.random().next_int_bounded(20));
        batch_cache.retain(|_, retry| retry.is_still_valid(timestamp));
        let body_pos = c.body.block_position();
        // `findAllClosestFirstWithType(poiType, cacheTest, ...)`: the cache test runs over the whole range in walk order (it may
        // roll the retry delay), then the matches are sorted by distance (stable), limited to 5 and validated
        let candidates = level.poi_manager().find_all_with_type(
            &*poi_type,
            &|_| true,
            body_pos,
            SCAN_RANGE,
            Occupancy::HasSpace,
        );
        let mut matches: Vec<(PoiTypeHolder, BlockPos)> = Vec::new();
        for (ty, pos) in candidates {
            let pass = match batch_cache.get_mut(&pos.as_long()) {
                None => true,
                Some(marker) if !marker.should_retry(timestamp) => false,
                Some(marker) => {
                    marker.mark_attempt(timestamp, level.random());
                    true
                }
            };
            if pass {
                matches.push((ty, pos));
            }
        }
        matches.sort_by(|a, b| a.1.dist_sqr(&body_pos).total_cmp(&b.1.dist_sqr(&body_pos)));
        // `.collect(Collectors.toSet())`: a `HashSet` of pairs, iterated by hash below
        let poi_positions = java_poi_set_order(
            matches
                .into_iter()
                .take(5)
                .filter(|(_, pos)| valid_poi(level, *pos))
                .collect(),
        );
        let path = find_path_to_pois(c.body, level, &poi_positions);
        match path.filter(|p| p.can_reach()) {
            Some(path) => {
                let target_pos = path.get_target();
                if level.poi_manager().get_type(target_pos).is_some() {
                    level.poi_manager().take(
                        &*poi_type,
                        &|_, poi_pos| poi_pos == target_pos,
                        target_pos,
                        1,
                    );
                    let global = GlobalPos {
                        dimension: level.dimension.clone(),
                        pos: target_pos,
                    };
                    c.body.brain_mut().set_memory(&memory_to_acquire, global);
                    if let Some(event) = on_poi_acquisition_event {
                        level.broadcast_entity_event(c.body.id(), event);
                    }
                    batch_cache.clear();
                }
            }
            None => {
                for (_, pos) in &poi_positions {
                    let marker = JitteredLinearRetry::new(level.random(), timestamp);
                    batch_cache.entry(pos.as_long()).or_insert(marker);
                }
            }
        }
        true
    });
    if memory_to_acquire == memory_to_validate {
        acquire_poi
    } else {
        let mut i = BehaviorBuilder::instance();
        let _to_validate = i.absent(&memory_to_validate);
        i.create(move |c| {
            use super::Trigger;
            acquire_poi.trigger(c.level, c.body, c.timestamp)
        })
    }
}

/// `findPathToPois(body, pois)`.
pub fn find_path_to_pois(
    body: &mut dyn Entity,
    level: &mut ServerLevel,
    pois: &[(PoiTypeHolder, BlockPos)],
) -> Option<Path> {
    if pois.is_empty() {
        return None;
    }
    let mut targets = Vec::with_capacity(pois.len());
    let mut max_range = 1;
    for (poi_type, pos) in pois {
        max_range = max_range.max(poi_type.value().valid_range);
        targets.push(*pos);
    }
    let targets = java_block_pos_set_order(&targets);
    nav::with_navigation(body, level, |navigation, body, level| {
        navigation.create_path_set(body, level, &targets, max_range)
    })
    .flatten()
}

/// `AcquirePoi.JitteredLinearRetry`.
struct JitteredLinearRetry {
    previous_attempt_timestamp: i64,
    next_scheduled_attempt_timestamp: i64,
    current_delay: i32,
}

impl JitteredLinearRetry {
    fn new(random: &mut dyn mc_core::random::RandomSource, first_attempt_timestamp: i64) -> Self {
        let mut marker = Self {
            previous_attempt_timestamp: 0,
            next_scheduled_attempt_timestamp: 0,
            current_delay: 0,
        };
        marker.mark_attempt(first_attempt_timestamp, random);
        marker
    }

    fn mark_attempt(&mut self, timestamp: i64, random: &mut dyn mc_core::random::RandomSource) {
        self.previous_attempt_timestamp = timestamp;
        let suggested_delay = self.current_delay + random.next_int_bounded(40) + 40;
        self.current_delay = suggested_delay.min(400);
        self.next_scheduled_attempt_timestamp = timestamp + i64::from(self.current_delay);
    }

    fn is_still_valid(&self, timestamp: i64) -> bool {
        timestamp - self.previous_attempt_timestamp < 400
    }

    fn should_retry(&self, timestamp: i64) -> bool {
        timestamp >= self.next_scheduled_attempt_timestamp
    }
}
