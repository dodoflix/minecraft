//! Map saved data seen from item frames (`MapItem.getSavedData`, `MapItemSavedData.removedFromFrame` /
//! `isTrackedCountOverLimit`).

use crate::server::level::ServerLevel;
use mc_core::pos::BlockPos;
use mc_protocol::components::MapId;

/// `MapItem.getSavedData(mapId, level).removedFromFrame(pos, entityId)`.
pub fn removed_from_frame(
    _level: &mut ServerLevel,
    _map_id: MapId,
    _pos: BlockPos,
    _entity_id: i32,
) {
}

/// `MapItem.getSavedData(stack, level)` then `data.isTrackedCountOverLimit(limit)`; `None` when the stack has no map data.
pub fn tracked_count_over_limit(
    _level: &ServerLevel,
    _stack: &crate::world::item::ItemStack,
    _limit: i32,
) -> Option<bool> {
    None
}
