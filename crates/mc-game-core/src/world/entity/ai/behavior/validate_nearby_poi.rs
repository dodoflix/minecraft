//! `ValidateNearbyPoi`: forget a remembered POI that vanished or whose bed got occupied.

use super::village_util::PoiPredicate;
use super::{BehaviorBuilder, OneShot};
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::MemoryModuleType;
use crate::world::entity::npc::villager::villager_ref;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::props;
use mc_protocol::codec::GlobalPos;

const MAX_DISTANCE: f64 = 16.0;

/// `create(poiType, memoryType)`.
pub fn create(poi_type: PoiPredicate, memory_type: MemoryModuleType<GlobalPos>) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let memory = i.present(&memory_type);
    i.create(move |c| {
        let global = c.get(&memory).clone();
        let pos = global.pos;
        if c.level.dimension == global.dimension
            && pos.closer_to_center_than(&c.body().position(), MAX_DISTANCE)
        {
            let exists = c.level.poi_manager().exists(pos, &*poi_type);
            if !exists {
                c.erase(&memory);
            } else if bed_is_occupied(c.level, pos, c.body()) {
                c.erase(&memory);
                if !bed_is_occupied_by_villager(c.level, pos) {
                    let _ = c.level.poi_manager().release(pos);
                }
            }
            true
        } else {
            false
        }
    })
}

/// `bedIsOccupied(poiLevel, poiPos, body)`.
fn bed_is_occupied(
    level: &crate::server::level::ServerLevel,
    poi_pos: BlockPos,
    body: &dyn Entity,
) -> bool {
    let state = level.get_block_state(poi_pos);
    state.is_tag("villagers_can_sleep_on_bed") && state.get(props::OCCUPIED) && !body.is_sleeping()
}

/// `bedIsOccupiedByVillager(poiLevel, poiPos)`.
fn bed_is_occupied_by_villager(
    level: &crate::server::level::ServerLevel,
    poi_pos: BlockPos,
) -> bool {
    let bb = AABB::new(
        f64::from(poi_pos.x()),
        f64::from(poi_pos.y()),
        f64::from(poi_pos.z()),
        f64::from(poi_pos.x() + 1),
        f64::from(poi_pos.y() + 1),
        f64::from(poi_pos.z() + 1),
    );
    !level
        .get_entities_of_class(&bb, |e| villager_ref(e).is_some(), |e| e.is_sleeping())
        .is_empty()
}
