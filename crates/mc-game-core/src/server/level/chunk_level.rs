//! `ChunkLevel`: the ticket-level <-> chunk status mapping (lower level = more loaded).
use super::FullChunkStatus;
use crate::world::level::chunk::status::{ChunkStep, GENERATION_PYRAMID};
use mc_world::status::ChunkStatus;
use std::sync::LazyLock;

pub const FULL_CHUNK_LEVEL: i32 = 33;
pub const BLOCK_TICKING_LEVEL: i32 = 32;
pub const ENTITY_TICKING_LEVEL: i32 = 31;

fn full_chunk_step() -> &'static ChunkStep {
    GENERATION_PYRAMID.get_step_to(ChunkStatus::Full)
}

/// `ChunkLevel.RADIUS_AROUND_FULL_CHUNK`.
pub static RADIUS_AROUND_FULL_CHUNK: LazyLock<i32> =
    LazyLock::new(|| full_chunk_step().accumulated_dependencies.get_radius());
/// `ChunkLevel.MAX_LEVEL`.
pub static MAX_LEVEL: LazyLock<i32> =
    LazyLock::new(|| FULL_CHUNK_LEVEL + *RADIUS_AROUND_FULL_CHUNK);

pub fn max_level() -> i32 {
    *MAX_LEVEL
}

/// `generationStatus(level)`.
pub fn generation_status(level: i32) -> Option<ChunkStatus> {
    get_status_around_full_chunk_opt(level - FULL_CHUNK_LEVEL)
}

fn get_status_around_full_chunk_opt(distance_to_full_chunk: i32) -> Option<ChunkStatus> {
    if distance_to_full_chunk > *RADIUS_AROUND_FULL_CHUNK {
        None
    } else if distance_to_full_chunk <= 0 {
        Some(ChunkStatus::Full)
    } else {
        Some(
            full_chunk_step()
                .accumulated_dependencies
                .get(distance_to_full_chunk),
        )
    }
}

/// `getStatusAroundFullChunk(distance)` (default `EMPTY`).
pub fn get_status_around_full_chunk(distance_to_full_chunk: i32) -> ChunkStatus {
    get_status_around_full_chunk_opt(distance_to_full_chunk).unwrap_or(ChunkStatus::Empty)
}

/// `byStatus(ChunkStatus)`.
pub fn by_status(status: ChunkStatus) -> i32 {
    FULL_CHUNK_LEVEL + full_chunk_step().get_accumulated_radius_of(status)
}

pub fn full_status(level: i32) -> FullChunkStatus {
    if level <= ENTITY_TICKING_LEVEL {
        FullChunkStatus::EntityTicking
    } else if level <= BLOCK_TICKING_LEVEL {
        FullChunkStatus::BlockTicking
    } else if level <= FULL_CHUNK_LEVEL {
        FullChunkStatus::Full
    } else {
        FullChunkStatus::Inaccessible
    }
}

/// `byStatus(FullChunkStatus)`.
pub fn by_full_status(status: FullChunkStatus) -> i32 {
    match status {
        FullChunkStatus::Inaccessible => max_level(),
        FullChunkStatus::Full => FULL_CHUNK_LEVEL,
        FullChunkStatus::BlockTicking => BLOCK_TICKING_LEVEL,
        FullChunkStatus::EntityTicking => ENTITY_TICKING_LEVEL,
    }
}

pub fn is_entity_ticking(level: i32) -> bool {
    level <= ENTITY_TICKING_LEVEL
}

pub fn is_block_ticking(level: i32) -> bool {
    level <= BLOCK_TICKING_LEVEL
}

pub fn is_loaded(level: i32) -> bool {
    level <= max_level()
}
