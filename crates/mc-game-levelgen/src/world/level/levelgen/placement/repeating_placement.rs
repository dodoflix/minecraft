//! `RepeatingPlacement`: emits the origin `count(random, origin)` times.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::placement::PlacementContext;

pub trait RepeatingPlacement {
    fn count(&self, random: &mut dyn RandomSource, origin: BlockPos) -> i32;
}

/// `RepeatingPlacement.modify`.
pub fn repeating_modify(
    placement: &impl RepeatingPlacement,
    _context: &PlacementContext<'_>,
    random: &mut dyn RandomSource,
    origin: BlockPos,
    output: &mut Vec<BlockPos>,
) {
    let count = placement.count(random, origin);
    for _ in 0..count {
        output.push(origin);
    }
}
