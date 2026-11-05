//! The level-mutating `onMobRemoved` / `onMobHurt` reactions of `WeavingMobEffect`, `InfestedMobEffect` and
//! `WindChargedMobEffect`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::block::BlockStateExt;
use crate::world::level::level_access::LevelAccessor;
use crate::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

/// `Mth.randomBetweenInclusive(random, min, max)`.
fn random_between_inclusive(random: &mut dyn RandomSource, min: i32, max: i32) -> i32 {
    random.next_int_bounded(max - min + 1) + min
}

/// `WeavingMobEffect.onMobRemoved` (KILLED): `mob instanceof Player || mobGriefing`.
pub fn weaving(level: &mut ServerLevel, mob: &mut dyn Entity) {
    if !(mob.is_player() || level.mob_griefing()) {
        return;
    }
    let pos = mob.block_position();
    let random = &mut *mob.base_mut().random;
    // `maxCobwebs = random -> Mth.randomBetweenInclusive(random, 2, 3)`
    let cobweb_count = random_between_inclusive(random, 2, 3) as usize;
    let mut positions = BlockPosHashSet::new();
    for block_pos in BlockPos::random_in_cube(random, 15, &pos, 1) {
        let below = block_pos.below();
        if !positions.contains(&block_pos) && level.get_block_state(block_pos).can_be_replaced() && level.get_block_state(below).is_face_sturdy_at(&*level, below, Direction::Up) {
            positions.insert(block_pos);
            if positions.len() >= cobweb_count {
                break;
            }
        }
    }
    for block_pos in positions.iter() {
        level.set_block_and_update(block_pos, mc_data::Block::COBWEB.default_state());
        level.level_event(None, 3018, block_pos, 0);
    }
}

/// `InfestedMobEffect.onMobHurt`: `chanceToSpawn = 0.1F`, `spawnedCount = random -> randomBetweenInclusive(1, 2)`.
/// The silverfish themselves are spawned by the mob classes (no `Silverfish` entity exists yet), only the draws from
/// the mob's random are reproduced.
pub fn infested(_level: &mut ServerLevel, mob: &mut dyn Entity) {
    let random = &mut *mob.base_mut().random;
    if random.next_float() <= 0.1 {
        let _count = random_between_inclusive(random, 1, 2);
    }
}
