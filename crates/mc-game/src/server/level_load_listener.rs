//! `server.level.progress`: `LevelLoadListener`, `LevelLoadProgressTracker`, `LoggingLevelLoadListener`.

use super::shared::get_nanos;
use crate::world::level::DimensionKey;
use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use mc_text::Component;

/// `LevelLoadListener.Stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    StartServer,
    PrepareGlobalSpawn,
    LoadInitialChunks,
    LoadPlayerChunks,
}

pub trait LevelLoadListener: Send {
    fn start(&mut self, stage: Stage, total_chunks: i32);
    fn update(&mut self, stage: Stage, current_chunks: i32, total_chunks: i32);
    fn finish(&mut self, stage: Stage);
    fn update_focus(&mut self, dimension: &DimensionKey, chunk_pos: ChunkPos);
}

/// `LevelLoadListener.compose`.
pub struct ComposedLevelLoadListener<A, B>(pub A, pub B);

impl<A: LevelLoadListener, B: LevelLoadListener> LevelLoadListener for ComposedLevelLoadListener<A, B> {
    fn start(&mut self, stage: Stage, total_chunks: i32) {
        self.0.start(stage, total_chunks);
        self.1.start(stage, total_chunks);
    }
    fn update(&mut self, stage: Stage, current_chunks: i32, total_chunks: i32) {
        self.0.update(stage, current_chunks, total_chunks);
        self.1.update(stage, current_chunks, total_chunks);
    }
    fn finish(&mut self, stage: Stage) {
        self.0.finish(stage);
        self.1.finish(stage);
    }
    fn update_focus(&mut self, dimension: &DimensionKey, chunk_pos: ChunkPos) {
        self.0.update_focus(dimension, chunk_pos);
        self.1.update_focus(dimension, chunk_pos);
    }
}

/// `LevelLoadProgressTracker`.
pub struct LevelLoadProgressTracker {
    include_player_chunks: bool,
    total_weight: i32,
    finalized_weight: i32,
    segment_weight: i32,
    segment_fraction: f32,
    progress: f32,
}

/// `Mth.square(7)`.
const EXPECTED_PLAYER_CHUNKS: i32 = 49;

impl LevelLoadProgressTracker {
    pub fn new(include_player_chunks: bool) -> Self {
        Self { include_player_chunks, total_weight: 0, finalized_weight: 0, segment_weight: 0, segment_fraction: 0.0, progress: 0.0 }
    }

    pub fn get(&self) -> f32 {
        self.progress
    }

    fn begin_segment(&mut self, weight: i32) {
        self.segment_weight = weight;
        self.segment_fraction = 0.0;
        self.update_progress();
    }

    fn finish_segment(&mut self) {
        self.finalized_weight = self.finalized_weight.wrapping_add(self.segment_weight);
        self.segment_weight = 0;
        self.update_progress();
    }

    fn tracks_stage(&self, stage: Stage) -> bool {
        match stage {
            Stage::LoadInitialChunks => true,
            Stage::LoadPlayerChunks => self.include_player_chunks,
            _ => false,
        }
    }

    fn update_progress(&mut self) {
        self.progress = if self.total_weight == 0 {
            0.0
        } else {
            let current_weight = self.finalized_weight as f32 + self.segment_fraction * self.segment_weight as f32;
            current_weight / self.total_weight as f32
        };
    }
}

impl LevelLoadListener for LevelLoadProgressTracker {
    fn start(&mut self, stage: Stage, total_chunks: i32) {
        if !self.tracks_stage(stage) {
            return;
        }
        match stage {
            Stage::LoadInitialChunks => {
                let player_chunks_weight = if self.include_player_chunks { EXPECTED_PLAYER_CHUNKS } else { 0 };
                self.total_weight = 10i32.wrapping_add(total_chunks).wrapping_add(player_chunks_weight);
                self.begin_segment(10);
                self.finish_segment();
                self.begin_segment(total_chunks);
            }
            Stage::LoadPlayerChunks => self.begin_segment(EXPECTED_PLAYER_CHUNKS),
            _ => {}
        }
    }

    fn update(&mut self, stage: Stage, current_chunks: i32, total_chunks: i32) {
        if self.tracks_stage(stage) {
            self.segment_fraction = if total_chunks == 0 { 0.0 } else { current_chunks as f32 / total_chunks as f32 };
            self.update_progress();
        }
    }

    fn finish(&mut self, stage: Stage) {
        if self.tracks_stage(stage) {
            self.finish_segment();
        }
    }

    fn update_focus(&mut self, _dimension: &DimensionKey, _chunk_pos: ChunkPos) {}
}

fn get_millis() -> i64 {
    get_nanos() / 1_000_000
}

/// `LoggingLevelLoadListener`.
pub struct LoggingLevelLoadListener {
    include_player_chunks: bool,
    progress_tracker: LevelLoadProgressTracker,
    closed: bool,
    start_time: i64,
    next_log_time: i64,
}

impl LoggingLevelLoadListener {
    pub fn new(include_player_chunks: bool) -> Self {
        Self {
            include_player_chunks,
            progress_tracker: LevelLoadProgressTracker::new(include_player_chunks),
            closed: false,
            start_time: i64::MAX,
            next_log_time: i64::MAX,
        }
    }

    pub fn for_dedicated_server() -> Self {
        Self::new(false)
    }

    pub fn for_singleplayer() -> Self {
        Self::new(true)
    }
}

impl LevelLoadListener for LoggingLevelLoadListener {
    fn start(&mut self, stage: Stage, total_chunks: i32) {
        if self.closed {
            return;
        }
        if self.start_time == i64::MAX {
            let now = get_millis();
            self.start_time = now;
            self.next_log_time = now;
        }
        self.progress_tracker.start(stage, total_chunks);
        match stage {
            Stage::PrepareGlobalSpawn => log::info!("Selecting global world spawn..."),
            Stage::LoadInitialChunks => log::info!("Loading {total_chunks} persistent chunks..."),
            Stage::LoadPlayerChunks => log::info!("Loading {total_chunks} chunks for player spawn..."),
            Stage::StartServer => {}
        }
    }

    fn update(&mut self, stage: Stage, current_chunks: i32, total_chunks: i32) {
        if self.closed {
            return;
        }
        self.progress_tracker.update(stage, current_chunks, total_chunks);
        if get_millis() > self.next_log_time {
            self.next_log_time += 500;
            let percent = mth::floor_f32(self.progress_tracker.get() * 100.0);
            log::info!("{}", Component::translatable("menu.preparingSpawn", vec![percent.into()]).get_string());
        }
    }

    fn finish(&mut self, stage: Stage) {
        if self.closed {
            return;
        }
        self.progress_tracker.finish(stage);
        let final_stage = if self.include_player_chunks { Stage::LoadPlayerChunks } else { Stage::LoadInitialChunks };
        if stage == final_stage {
            log::info!("Time elapsed: {} ms", get_millis() - self.start_time);
            self.next_log_time = i64::MAX;
            self.closed = true;
        }
    }

    fn update_focus(&mut self, _dimension: &DimensionKey, _chunk_pos: ChunkPos) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_weights() {
        let mut t = LevelLoadProgressTracker::new(false);
        t.start(Stage::PrepareGlobalSpawn, 0);
        assert_eq!(t.get(), 0.0);
        t.start(Stage::LoadInitialChunks, 90);
        assert_eq!(t.get(), 0.1); // the 10-weight "prepare server" segment is finished
        t.update(Stage::LoadInitialChunks, 45, 90);
        assert_eq!(t.get(), 0.55);
        t.finish(Stage::LoadInitialChunks);
        assert_eq!(t.get(), 1.0);

        let mut with_players = LevelLoadProgressTracker::new(true);
        with_players.start(Stage::LoadInitialChunks, 41);
        with_players.finish(Stage::LoadInitialChunks);
        assert_eq!(with_players.get(), 0.51);
        with_players.start(Stage::LoadPlayerChunks, 49);
        with_players.update(Stage::LoadPlayerChunks, 49, 49);
        assert_eq!(with_players.get(), 1.0);
    }

    #[test]
    fn progress_message_text() {
        assert_eq!(Component::translatable("menu.preparingSpawn", vec![100.into()]).get_string(), "Preparing spawn area: 100%");
    }
}
