//! `GoToTargetLocation`.

use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, LOOK_TARGET, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::MemoryModuleType;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

fn get_random_offset(random: &mut dyn RandomSource) -> i32 {
    random.next_int_bounded(3) - 1
}

fn get_nearby_pos(level: &mut ServerLevel, pos: BlockPos) -> BlockPos {
    let random = &mut level.tick_state.random;
    let x = get_random_offset(random);
    let z = get_random_offset(random);
    pos.offset(x, 0, z)
}

/// `create(locationMemory, closeEnoughDist, speedModifier)`.
pub fn create(location_memory: MemoryModuleType<BlockPos>, close_enough_dist: i32, speed_modifier: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let location = i.present(&location_memory);
    let _attack_target = i.absent(&ATTACK_TARGET);
    let _walk_target = i.absent(&WALK_TARGET);
    let _look_target = i.registered(&LOOK_TARGET);
    i.create(move |c| {
        let celebrate_location = *c.get(&location);
        let close_enough_to_target = celebrate_location.closer_than(&c.body.block_position(), f64::from(close_enough_dist));
        if !close_enough_to_target {
            let nearby = get_nearby_pos(c.level, celebrate_location);
            behavior_utils::set_walk_and_look_target_memories_pos(c.body, nearby, speed_modifier, close_enough_dist);
        }
        true
    })
}
