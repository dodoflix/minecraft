//! `ChunkTrackingView`: the square-ish set of chunks a player is tracking.
use mc_core::pos::ChunkPos;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChunkTrackingView {
    #[default]
    Empty,
    Positioned { center: ChunkPos, view_distance: i32 },
}

impl ChunkTrackingView {
    /// `ChunkTrackingView.of(center, radius)`.
    pub fn of(center: ChunkPos, radius: i32) -> Self {
        Self::Positioned { center, view_distance: radius }
    }

    /// `contains(pos)` (includes neighbours).
    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.contains_xz(pos.x, pos.z)
    }

    pub fn contains_xz(&self, x: i32, z: i32) -> bool {
        self.contains_with(x, z, true)
    }

    /// `contains(chunkX, chunkZ, includeNeighbors)`.
    pub fn contains_with(&self, chunk_x: i32, chunk_z: i32, include_neighbors: bool) -> bool {
        match self {
            Self::Empty => false,
            Self::Positioned { center, view_distance } => is_within_distance(center.x, center.z, *view_distance, chunk_x, chunk_z, include_neighbors),
        }
    }

    /// `isInViewDistance(chunkX, chunkZ)`.
    pub fn is_in_view_distance(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.contains_with(chunk_x, chunk_z, false)
    }

    /// `forEach(consumer)`.
    pub fn for_each(&self, mut consumer: impl FnMut(ChunkPos)) {
        if let Self::Positioned { .. } = self {
            let (min_x, min_z, max_x, max_z) = self.bounds();
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    if self.contains_xz(x, z) {
                        consumer(ChunkPos::new(x, z));
                    }
                }
            }
        }
    }

    fn bounds(&self) -> (i32, i32, i32, i32) {
        match self {
            Self::Empty => (0, 0, -1, -1),
            Self::Positioned { center, view_distance } => {
                (center.x - view_distance - 1, center.z - view_distance - 1, center.x + view_distance + 1, center.z + view_distance + 1)
            }
        }
    }

    fn square_intersects(&self, other: &Self) -> bool {
        let (a_min_x, a_min_z, a_max_x, a_max_z) = self.bounds();
        let (b_min_x, b_min_z, b_max_x, b_max_z) = other.bounds();
        a_min_x <= b_max_x && a_max_x >= b_min_x && a_min_z <= b_max_z && a_max_z >= b_min_z
    }

    /// `ChunkTrackingView.difference(from, to, onEnter, onLeave)`.
    pub fn difference(from: &Self, to: &Self, mut on_enter: impl FnMut(ChunkPos), mut on_leave: impl FnMut(ChunkPos)) {
        if from == to {
            return;
        }
        if matches!(from, Self::Positioned { .. }) && matches!(to, Self::Positioned { .. }) && from.square_intersects(to) {
            let (a_min_x, a_min_z, a_max_x, a_max_z) = from.bounds();
            let (b_min_x, b_min_z, b_max_x, b_max_z) = to.bounds();
            let (min_x, min_z, max_x, max_z) = (a_min_x.min(b_min_x), a_min_z.min(b_min_z), a_max_x.max(b_max_x), a_max_z.max(b_max_z));
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    let saw = from.contains_xz(x, z);
                    let sees = to.contains_xz(x, z);
                    if saw != sees {
                        if sees { on_enter(ChunkPos::new(x, z)) } else { on_leave(ChunkPos::new(x, z)) }
                    }
                }
            }
        } else {
            from.for_each(&mut on_leave);
            to.for_each(&mut on_enter);
        }
    }
}

/// `ChunkTrackingView.isInViewDistance(centerX, centerZ, viewDistance, chunkX, chunkZ)`.
pub fn is_in_view_distance(center_x: i32, center_z: i32, view_distance: i32, chunk_x: i32, chunk_z: i32) -> bool {
    is_within_distance(center_x, center_z, view_distance, chunk_x, chunk_z, false)
}

/// `isWithinDistance(...)`.
pub fn is_within_distance(center_x: i32, center_z: i32, view_distance: i32, chunk_x: i32, chunk_z: i32, include_neighbors: bool) -> bool {
    let buffer_range: i32 = if include_neighbors { 2 } else { 1 };
    let delta_x = i64::from(0.max((chunk_x.wrapping_sub(center_x)).wrapping_abs().wrapping_sub(buffer_range)));
    let delta_z = i64::from(0.max((chunk_z.wrapping_sub(center_z)).wrapping_abs().wrapping_sub(buffer_range)));
    let distance_squared = delta_x * delta_x + delta_z * delta_z;
    let radius_squared = view_distance.wrapping_mul(view_distance);
    distance_squared < i64::from(radius_squared)
}
