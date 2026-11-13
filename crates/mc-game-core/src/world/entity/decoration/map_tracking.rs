//! Map saved data seen from item frames (`MapItem.getSavedData`, `MapItemSavedData.removedFromFrame` /
//! `isTrackedCountOverLimit`).

use crate::server::level::ServerLevel;
use mc_core::pos::BlockPos;
use mc_protocol::components::{MAP_ID, MapId};

/// `MapItem.getSavedData(mapId, level).removedFromFrame(pos, entityId)`.
pub fn removed_from_frame(level: &ServerLevel, map_id: MapId, pos: BlockPos, entity_id: i32) {
    level.modify_map_data(map_id, |data| data.removed_from_frame(pos, entity_id));
}

/// `MapItem.getSavedData(stack, level)` then `data.isTrackedCountOverLimit(limit)`; `None` when the stack has no map data.
pub fn tracked_count_over_limit(
    level: &ServerLevel,
    stack: &crate::world::item::ItemStack,
    limit: i32,
) -> Option<bool> {
    level.read_map_data(*stack.get(&MAP_ID)?, |data| {
        data.is_tracked_count_over_limit(limit)
    })
}
