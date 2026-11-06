//! `net.minecraft.server.level.PlayerSpawnFinder`: finds a safe spawn around a suggestion. Vanilla drives a chain of
//! chunk-load futures; here the finder is a poll-driven state machine and the caller (the configuration-phase
//! `PrepareSpawnTask`) loads the chunk of each candidate (`SPAWN_SEARCH` ticket, radius 0), then asks for its check:
//!
//! ```text
//! match PlayerSpawnFinder::find_spawn(..) { Immediate(v) => v, Search(mut f) => loop {
//!     match f.next_candidate() { Candidate(c) => { load_chunk(c.chunk_pos()); if let Some(v) = f.check(level, &c) { break v } }
//!                                Fallback(c) => { load_chunk(..); break fixup_spawn_height(level, c) } } } }
//! ```

use crate::server::spawn_search::SpawnCandidates;
use crate::world::level::LevelReader;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::CollisionGetter;
use crate::world::level::shapes::is_face_full;
use mc_core::direction::Direction;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::EntityType;
use mc_data::block::BlockState;
use mc_world::heightmap::HeightmapType;

/// What the spawn search needs from a `ServerLevel` beyond block access.
pub trait SpawnSearchLevel: LevelReader + CollisionGetter {
    /// `dimensionType().hasCeiling()`.
    fn has_ceiling(&self) -> bool;
    /// `getChunkSource().getGenerator().getSpawnHeight(level)`.
    fn generator_spawn_height(&self) -> i32;
    /// `LevelChunk.getHeight(type, x & 15, z & 15)` of the (loaded) chunk containing `x`/`z`: the raw heightmap value.
    fn chunk_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32;
}

/// `EntityTypes.PLAYER.getDimensions().makeBoundingBox(Vec3.atBottomCenterOf(pos))`.
fn player_box_at(pos: BlockPos) -> AABB {
    let data = EntityType::PLAYER.data();
    let w = f64::from(data.width / 2.0);
    let h = f64::from(data.height);
    let (x, y, z) = (
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    );
    AABB::new(x - w, y, z - w, x + w, y + h, z + w)
}

fn bottom_center_of(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    )
}

/// `noCollisionNoLiquid`.
fn no_collision_no_liquid<L: CollisionGetter>(level: &L, pos: BlockPos) -> bool {
    let context = crate::world::level::collision::CollisionContext::empty_with_fluid_collisions();
    level.no_collision(None, Some(&context), &player_box_at(pos))
}

/// `fixupSpawnHeight`: move up out of the ground, then down onto it.
pub fn fixup_spawn_height<L: CollisionGetter>(level: &L, spawn_pos: BlockPos) -> Vec3 {
    let mut y = spawn_pos.y();
    let at = |y: i32| BlockPos::new(spawn_pos.x(), y, spawn_pos.z());
    while !no_collision_no_liquid(level, at(y)) && y < level.get_max_y() {
        y += 1;
    }
    y -= 1;
    while no_collision_no_liquid(level, at(y)) && y > level.get_min_y() {
        y -= 1;
    }
    y += 1;
    bottom_center_of(at(y))
}

/// `getLevelRespawnPos(level, x, z)`: the standable position above the surface column, if any.
pub fn get_level_respawn_pos<L: SpawnSearchLevel>(level: &L, x: i32, z: i32) -> Option<BlockPos> {
    let top_y = if level.has_ceiling() {
        level.generator_spawn_height()
    } else {
        level.chunk_height(HeightmapType::MotionBlocking, x, z)
    };
    if top_y < level.get_min_y() {
        return None;
    }
    let surface = level.chunk_height(HeightmapType::WorldSurface, x, z);
    if surface <= top_y && surface > level.chunk_height(HeightmapType::OceanFloor, x, z) {
        return None;
    }
    let mut y = top_y + 1;
    while y >= level.get_min_y() {
        let pos = BlockPos::new(x, y, z);
        let state: BlockState = level.get_block_state(pos);
        if !state.get_fluid_state().is_empty() {
            break;
        }
        if is_face_full(&state.get_collision_shape(level, pos), Direction::Up) {
            return Some(pos.above());
        }
        y -= 1;
    }
    None
}

/// `getSpawnPosInChunk`.
pub fn get_spawn_pos_in_chunk<L: SpawnSearchLevel>(
    level: &L,
    chunk_pos: ChunkPos,
) -> Option<BlockPos> {
    for x in chunk_pos.get_min_block_x()..=chunk_pos.get_max_block_x() {
        for z in chunk_pos.get_min_block_z()..=chunk_pos.get_max_block_z() {
            if let Some(pos) = get_level_respawn_pos(level, x, z) {
                return Some(pos);
            }
        }
    }
    None
}

/// One candidate column of the search; the caller must have its chunk loaded before calling [`PlayerSpawnFinder::check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub x: i32,
    pub z: i32,
    pub index: i32,
}

impl Candidate {
    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos::new(self.x >> 4, self.z >> 4)
    }
}

pub enum Step {
    /// Load `candidate`'s chunk, then [`PlayerSpawnFinder::check`] it.
    Candidate(Candidate),
    /// All candidates failed: load the chunk of the suggestion and use [`fixup_spawn_height`] there.
    Fallback(BlockPos),
}

pub enum FindSpawn {
    /// `GameType.ADVENTURE`: no search, the suggestion with a fixed-up height (needs its chunk loaded by the caller).
    Adventure(BlockPos),
    Search(PlayerSpawnFinder),
}

pub struct PlayerSpawnFinder {
    spawn_suggestion: BlockPos,
    candidates: SpawnCandidates,
    next_candidate_index: i32,
}

impl PlayerSpawnFinder {
    /// `findSpawn`: `respawn_radius` is `GameRules.RESPAWN_RADIUS`, `distance_to_border` is
    /// `Mth.floor(worldBorder.getDistanceToBorder(x, z))` at the suggestion.
    pub fn find_spawn(
        adventure: bool,
        respawn_radius: i32,
        distance_to_border: i32,
        spawn_suggestion: BlockPos,
    ) -> FindSpawn {
        if adventure {
            return FindSpawn::Adventure(spawn_suggestion);
        }
        let mut radius = respawn_radius.max(0);
        if distance_to_border < radius {
            radius = distance_to_border;
        }
        if distance_to_border <= 1 {
            radius = 1;
        }
        // `RandomSource.createThreadLocalInstance().nextInt(candidateCount)`
        let offset = rand::random_range(0..SpawnCandidates::candidate_count_for(radius));
        FindSpawn::Search(Self::with_offset(spawn_suggestion, radius, offset))
    }

    pub fn with_offset(spawn_suggestion: BlockPos, radius: i32, offset: i32) -> Self {
        Self {
            spawn_suggestion,
            candidates: SpawnCandidates::new(radius, offset),
            next_candidate_index: 0,
        }
    }

    /// `scheduleNext`.
    pub fn next_candidate(&mut self) -> Step {
        let index = self.next_candidate_index;
        self.next_candidate_index += 1;
        if index < self.candidates.candidate_count {
            let (dx, dz) = self.candidates.delta(index);
            Step::Candidate(Candidate {
                x: self.spawn_suggestion.x() + dx,
                z: self.spawn_suggestion.z() + dz,
                index,
            })
        } else {
            Step::Fallback(self.spawn_suggestion)
        }
    }

    /// The candidate checker of `scheduleCandidate`: the spawn if the column is usable.
    pub fn check<L: SpawnSearchLevel>(&self, level: &L, candidate: &Candidate) -> Option<Vec3> {
        let spawn_pos = get_level_respawn_pos(level, candidate.x, candidate.z)?;
        no_collision_no_liquid(level, spawn_pos).then(|| bottom_center_of(spawn_pos))
    }

    pub fn radius(&self) -> i32 {
        self.candidates.radius
    }

    pub fn candidate_count(&self) -> i32 {
        self.candidates.candidate_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_cover_the_square_exactly_once_without_repeats() {
        for radius in [0, 1, 2, 5, 10, 16, 20] {
            let mut finder = PlayerSpawnFinder::with_offset(
                BlockPos::new(100, 64, -50),
                radius,
                3 % (((2 * radius + 1) * (2 * radius + 1)).min(1024)),
            );
            let mut seen = std::collections::HashSet::new();
            let n = finder.candidate_count();
            for i in 0..n {
                let Step::Candidate(c) = finder.next_candidate() else {
                    panic!("fallback too early")
                };
                assert_eq!(c.index, i);
                assert!((c.x - 100).abs() <= radius && (c.z + 50).abs() <= radius);
                assert!(seen.insert((c.x, c.z)), "repeat at radius {radius}");
            }
            assert!(
                matches!(finder.next_candidate(), Step::Fallback(p) if p == BlockPos::new(100, 64, -50))
            );
            if n as i64 == (2 * radius as i64 + 1).pow(2) {
                assert_eq!(seen.len() as i32, n);
            }
        }
    }

    #[test]
    fn radius_clamps_like_vanilla() {
        let mk = |adv, rule, dist| match PlayerSpawnFinder::find_spawn(
            adv,
            rule,
            dist,
            BlockPos::ZERO,
        ) {
            FindSpawn::Search(f) => f.radius(),
            FindSpawn::Adventure(_) => -1,
        };
        assert_eq!(mk(false, 10, 1000), 10);
        assert_eq!(mk(false, -5, 1000), 0);
        assert_eq!(mk(false, 10, 4), 4);
        assert_eq!(mk(false, 10, 1), 1);
        assert_eq!(mk(false, 0, 0), 1);
        assert_eq!(mk(true, 10, 1000), -1);
    }
}
