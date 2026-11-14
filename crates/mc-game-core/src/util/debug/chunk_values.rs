//! `LevelChunk.registerDebugValues`: the structure starts and the raid centers of a chunk.

use super::{ChunkDebugGetter, DebugRegistration, DebugValue, subscriptions};
use crate::server::level::ServerLevel;
use crate::world::level::chunk::LevelChunk;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{
    DebugStructureInfo, DebugStructureInfoPiece, DebugSubscriptionEvent,
};

/// `registerDebugValues(level, registration)`.
pub fn register_chunk_debug_values(
    chunk: &LevelChunk,
    registration: &mut DebugRegistration<ChunkDebugGetter>,
) {
    if !chunk.data.access.structures.starts.is_empty() {
        registration.register(subscriptions::structures(), structures);
    }
    registration.register(subscriptions::raids(), raids);
}

fn raids(level: &ServerLevel, pos: ChunkPos) -> Option<DebugValue> {
    let centers = level.get_raids()?.get_raid_centers_in_chunk(pos.x, pos.z);
    Some(DebugValue::new(DebugSubscriptionEvent::Raids(centers)))
}

/// The `BB` int array of a piece or start (`StructurePiece.addAdditionalSaveData` writes `BoundingBox.createTag`).
fn piece_box(tag: &CompoundTag) -> Option<[i32; 6]> {
    tag.get_int_array("BB")?.try_into().ok()
}

fn structures(level: &ServerLevel, pos: ChunkPos) -> Option<DebugValue> {
    let chunk = level.chunk_source.get_chunk_now(pos.x, pos.z)?;
    let mut infos = Vec::new();
    for start in chunk.data.access.structures.starts.values() {
        if start.get_string("id").is_some_and(|id| id == "INVALID") {
            continue;
        }
        let children = start.get_list("Children")?;
        let pieces: Vec<[i32; 6]> = (0..children.len())
            .filter_map(|i| children.get_compound(i).and_then(piece_box))
            .collect();
        // `PiecesContainer.calculateBoundingBox`: the box around every piece
        let b = pieces.iter().copied().reduce(|a, p| {
            [
                a[0].min(p[0]),
                a[1].min(p[1]),
                a[2].min(p[2]),
                a[3].max(p[3]),
                a[4].max(p[4]),
                a[5].max(p[5]),
            ]
        })?;
        infos.push(DebugStructureInfo {
            bounding_box: BoundingBox::new(b[0], b[1], b[2], b[3], b[4], b[5]),
            pieces: pieces
                .iter()
                .enumerate()
                .map(|(i, p)| DebugStructureInfoPiece {
                    bounding_box: BoundingBox::new(p[0], p[1], p[2], p[3], p[4], p[5]),
                    is_start: i == 0,
                })
                .collect(),
        });
    }
    Some(DebugValue::new(DebugSubscriptionEvent::Structures(infos)))
}
