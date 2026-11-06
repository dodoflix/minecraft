//! `net.minecraft.world.level.blockscan` (`BoxBlockMatcher` / `BlockScanUtils.findBlocks`): the visiting order of
//! `LevelReader.findBlocksIn(from, to)` is section-major (section x, section z, section y; inside a section y, z, x), then
//! the positions above/below the world (observable when the consumer draws random numbers).

use super::LevelReader;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};

/// `BlockScanUtils.findBlocks`: calls `consumer(pos, state)` for every block of the box accepted by `predicate`; a `true`
/// result aborts the scan (and is returned).
pub fn find_blocks(
    level: &dyn LevelReader,
    from: BlockPos,
    to: BlockPos,
    predicate: &dyn Fn(BlockState) -> bool,
    consumer: &mut dyn FnMut(BlockPos, BlockState) -> bool,
) -> bool {
    let (min_x, min_y, min_z) = (
        from.x().min(to.x()),
        from.y().min(to.y()),
        from.z().min(to.z()),
    );
    let (max_x, max_y, max_z) = (
        from.x().max(to.x()),
        from.y().max(to.y()),
        from.z().max(to.z()),
    );
    let (world_min_y, world_max_y) = (level.get_min_y(), level.get_max_y());
    let min_section_y = (world_min_y >> 4).max(min_y >> 4);
    let max_section_y = (world_max_y >> 4).min(max_y >> 4);
    for x in (min_x >> 4)..=(max_x >> 4) {
        for z in (min_z >> 4)..=(max_z >> 4) {
            for y in min_section_y..=max_section_y {
                let from_x = (x << 4).max(min_x);
                let from_y = (y << 4).max(min_y);
                let from_z = (z << 4).max(min_z);
                let to_x = ((x << 4) + 15).min(max_x);
                let to_y = ((y << 4) + 15).min(max_y);
                let to_z = ((z << 4) + 15).min(max_z);
                for by in from_y..=to_y {
                    for bz in from_z..=to_z {
                        for bx in from_x..=to_x {
                            let pos = BlockPos::new(bx, by, bz);
                            let state = level.get_block_state(pos);
                            if predicate(state) && consumer(pos, state) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    let air = Block::AIR.default_state();
    if (min_y < world_min_y || max_y > world_max_y) && predicate(air) {
        let outside = [
            (min_y < world_min_y).then_some((min_y, world_min_y - 1)),
            (max_y > world_max_y).then_some((world_max_y + 1, max_y)),
        ];
        for (low, high) in outside.into_iter().flatten() {
            for z in min_z..=max_z {
                for y in low..=high {
                    for x in min_x..=max_x {
                        if consumer(BlockPos::new(x, y, z), air) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// `findBlocksIn(from, to).filterState(predicate).atLeastMatched(n)`.
pub fn at_least_matched(
    level: &dyn LevelReader,
    from: BlockPos,
    to: BlockPos,
    predicate: &dyn Fn(BlockState) -> bool,
    n: i32,
) -> bool {
    let mut count = 0;
    find_blocks(level, from, to, predicate, &mut |_, _| {
        count += 1;
        count >= n
    })
}

/// `findBlocksIn(from, to).filterState(predicate).atMostMatched(n)`.
pub fn at_most_matched(
    level: &dyn LevelReader,
    from: BlockPos,
    to: BlockPos,
    predicate: &dyn Fn(BlockState) -> bool,
    n: i32,
) -> bool {
    !at_least_matched(level, from, to, predicate, n + 1)
}

/// `findBlocksIn(from, to).filterState(predicate).anyMatched()`.
pub fn any_matched(
    level: &dyn LevelReader,
    from: BlockPos,
    to: BlockPos,
    predicate: &dyn Fn(BlockState) -> bool,
) -> bool {
    find_blocks(level, from, to, predicate, &mut |_, _| true)
}
