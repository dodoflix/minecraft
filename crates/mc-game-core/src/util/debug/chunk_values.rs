//! `LevelChunk.registerDebugValues`: the structure starts and the raid centers of a chunk.

use super::{ChunkDebugGetter, DebugRegistration, DebugValue, subscriptions};
use crate::server::level::ServerLevel;
use crate::world::level::chunk::LevelChunk;
use mc_core::pos::ChunkPos;
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

fn structures(level: &ServerLevel, pos: ChunkPos) -> Option<DebugValue> {
    let chunk = level.chunk_source.get_chunk_now(pos.x, pos.z)?;
    let env = level.gen_env.as_ref()?;
    let mut infos = Vec::new();
    for (id, raw) in &chunk.data.access.structures.starts {
        let Some(handle) = env.start_cache.get_or_parse(env, pos, id, raw) else {
            continue;
        };
        let mut start = handle.lock();
        if !start.is_valid() {
            continue;
        }
        infos.push(DebugStructureInfo {
            bounding_box: start.get_bounding_box(),
            pieces: start
                .piece_container
                .pieces
                .iter()
                .enumerate()
                .map(|(i, piece)| DebugStructureInfoPiece {
                    bounding_box: piece.base().bounding_box,
                    is_start: i == 0,
                })
                .collect(),
        });
    }
    Some(DebugValue::new(DebugSubscriptionEvent::Structures(infos)))
}
