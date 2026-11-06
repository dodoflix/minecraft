//! Pure parts of the spawn selection: `MinecraftServer.setInitialSpawn`'s chunk spiral and
//! `PlayerSpawnFinder`'s candidate permutation. The level-bound halves live in `spawn.rs`.

/// `MinecraftServer.SPAWN_POSITION_SEARCH_RADIUS`.
pub const SPAWN_POSITION_SEARCH_RADIUS: i32 = 5;

/// The chunk offsets `setInitialSpawn` visits, in order: a square spiral starting at (0, 0) for `11 * 11` steps,
/// skipping offsets outside `[-5, 5]`.
pub fn spawn_spiral() -> impl Iterator<Item = (i32, i32)> {
    let (mut x, mut z, mut dx, mut dz) = (0i32, 0i32, 0i32, -1i32);
    (0..11 * 11).filter_map(move |_| {
        let current = (x, z);
        if x == z || (x < 0 && x == -z) || (x > 0 && x == 1 - z) {
            let old_dx = dx;
            dx = -dz;
            dz = old_dx;
        }
        x += dx;
        z += dz;
        (-SPAWN_POSITION_SEARCH_RADIUS..=SPAWN_POSITION_SEARCH_RADIUS)
            .contains(&current.0)
            .then_some(current)
            .filter(|c| {
                (-SPAWN_POSITION_SEARCH_RADIUS..=SPAWN_POSITION_SEARCH_RADIUS).contains(&c.1)
            })
    })
}

/// `PlayerSpawnFinder.ABSOLUTE_MAX_ATTEMPTS`.
const ABSOLUTE_MAX_ATTEMPTS: i64 = 1024;

/// `PlayerSpawnFinder`'s candidate order: `(offset + coprime * i) % count` over a `(2r+1)^2` square, at most 1024
/// candidates.
pub struct SpawnCandidates {
    pub radius: i32,
    pub candidate_count: i32,
    coprime: i32,
    offset: i32,
}

impl SpawnCandidates {
    /// `offset` is `RandomSource.createThreadLocalInstance().nextInt(candidateCount)`; see [`Self::candidate_count_for`].
    pub fn new(radius: i32, offset: i32) -> Self {
        let candidate_count = Self::candidate_count_for(radius);
        Self {
            radius,
            candidate_count,
            coprime: get_coprime(candidate_count),
            offset,
        }
    }

    pub fn candidate_count_for(radius: i32) -> i32 {
        let square_side = i64::from(radius) * 2 + 1;
        ABSOLUTE_MAX_ATTEMPTS.min(square_side * square_side) as i32
    }

    /// `(deltaX - radius, deltaZ - radius)` of candidate `index` (`index < candidate_count`).
    pub fn delta(&self, index: i32) -> (i32, i32) {
        let value = (i64::from(self.offset) + i64::from(self.coprime) * i64::from(index))
            % i64::from(self.candidate_count);
        let side = i64::from(self.radius) * 2 + 1;
        (
            (value % side) as i32 - self.radius,
            (value / side) as i32 - self.radius,
        )
    }
}

/// `PlayerSpawnFinder.getCoprime`.
fn get_coprime(possible_origins: i32) -> i32 {
    if possible_origins <= 16 {
        possible_origins - 1
    } else {
        17
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn spiral_covers_the_square_once_in_vanilla_order() {
        let cells: Vec<_> = spawn_spiral().collect();
        assert_eq!(
            &cells[..6],
            &[(0, 0), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)]
        );
        assert_eq!(cells.len(), 121);
        assert_eq!(cells.iter().collect::<HashSet<_>>().len(), 121);
        assert!(cells.iter().all(|&(x, z)| x.abs() <= 5 && z.abs() <= 5));
    }

    #[test]
    fn candidates_are_a_permutation() {
        for (radius, offset) in [(0, 0), (1, 5), (10, 77), (32, 1000)] {
            let c = SpawnCandidates::new(radius, offset);
            let count = c.candidate_count;
            assert_eq!(count as i64, (i64::from(radius) * 2 + 1).pow(2).min(1024));
            let seen: HashSet<_> = (0..count).map(|i| c.delta(i)).collect();
            // coprime with count (or count == 1) => every index maps to a distinct cell
            if radius <= 15 {
                assert_eq!(seen.len(), count as usize, "radius {radius}");
            }
            assert!(
                seen.iter()
                    .all(|&(x, z)| x.abs() <= radius && z.abs() <= radius)
            );
        }
    }
}
