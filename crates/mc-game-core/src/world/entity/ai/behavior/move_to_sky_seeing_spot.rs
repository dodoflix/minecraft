//! `MoveToSkySeeingSpot`.

use super::{BehaviorBuilder, OneShot};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::level::LevelReader;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::heightmap::HeightmapType;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        if c.level.can_see_sky(c.body().block_position()) {
            false
        } else {
            if let Some(pos) = get_outdoor_position(c.level, c.body) {
                c.set(&walk_target, WalkTarget::from_vec3(pos, speed_modifier, 0));
            }
            true
        }
    })
}

/// `getOutdoorPosition(level, body)`.
fn get_outdoor_position(level: &ServerLevel, body: &mut dyn Entity) -> Option<Vec3> {
    let pos = body.block_position();
    for _ in 0..10 {
        let random = &mut body.base_mut().random;
        let (dx, dy, dz) = (
            random.next_int_bounded(20) - 10,
            random.next_int_bounded(6) - 3,
            random.next_int_bounded(20) - 10,
        );
        let random_pos = pos.offset(dx, dy, dz);
        if has_no_blocks_above(level, body, random_pos) {
            return Some(Vec3::at_bottom_center_of(&random_pos));
        }
    }
    None
}

/// `hasNoBlocksAbove(level, body, target)`.
pub fn has_no_blocks_above(level: &ServerLevel, body: &dyn Entity, target: BlockPos) -> bool {
    level.can_see_sky(target)
        && f64::from(
            level
                .get_heightmap_pos(HeightmapType::MotionBlocking, target)
                .y(),
        ) <= body.base().y()
}
