//! `InsideBrownianWalk`.

use super::OneShot;
use super::declarative::BehaviorBuilder;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::navigation::entity_can_stand_on;
use crate::world::entity::movement::collision_context;
use crate::world::level::BlockGetter;
use crate::world::level::collision::CollisionGetter;
use crate::world::level::level_access::LevelReader;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        let body_pos = c.body.block_position();
        if c.level.can_see_sky(body_pos) {
            return false;
        }
        let mut poses: Vec<BlockPos> = BlockPos::between_closed_xyz(
            body_pos.x() - 1,
            body_pos.y() - 1,
            body_pos.z() - 1,
            body_pos.x() + 1,
            body_pos.y() + 1,
            body_pos.z() + 1,
        )
        .collect();
        // `Collections.shuffle(poses)` (unseeded `java.util.Random`): same algorithm, level random
        for n in (2..=poses.len()).rev() {
            let j = c.level.tick_state.random.next_int_bounded(n as i32) as usize;
            poses.swap(n - 1, j);
        }
        let level = &*c.level;
        let body = &*c.body;
        let target = poses.into_iter().find(|&pos| {
            !level.can_see_sky(pos)
                && level.is_loaded(pos)
                && entity_can_stand_on(level.get_block_state(pos), level, pos, body)
                && level.no_collision(
                    Some(body.id()),
                    Some(&collision_context(body)),
                    &body.get_bounding_box(),
                )
        });
        if let Some(target) = target {
            c.set(
                &walk_target,
                WalkTarget::from_block_pos(target, speed_modifier, 0),
            );
        }
        true
    })
}
