//! `TryFindLand`.

use super::declarative::BehaviorBuilder;
use super::position_tracker::{BlockPosTracker, PositionTracker};
use super::OneShot;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, LOOK_TARGET, WALK_TARGET};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::movement::collision_context;
use crate::world::level::BlockGetter;
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::collision::CollisionContext;
use crate::world::level::material::FluidStateExt;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::Fluid;

/// `COOLDOWN_TICKS`.
const COOLDOWN_TICKS: i64 = 60;

/// `create(range, speedModifier)`.
pub fn create(range: i32, speed_modifier: f32) -> OneShot {
    let mut next_ok_start_time: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let _attack_target = i.absent(&ATTACK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    i.create(move |c| {
        let in_water = mc_data::tags::get::<Fluid>("water").is_some_and(|t| t.contains(c.level.get_fluid_state(c.body.block_position()).fluid()));
        if !in_water {
            false
        } else if c.timestamp < next_ok_start_time {
            next_ok_start_time = c.timestamp + COOLDOWN_TICKS;
            true
        } else {
            let body_block_pos = c.body.block_position();
            let context = collision_context(&*c.body);
            let level: &ServerLevel = &*c.level;
            let found = BlockPos::within_box_by_manhattan_distance(&body_block_pos, range, range, range).find(|&pos| {
                let state = level.get_block_state(pos);
                pos.differs_horizontally(&body_block_pos) && state.get_fluid_state().is_empty() && can_stand_on(level, pos, state, &context)
            });
            if let Some(target_pos) = found {
                c.set(&look_target, PositionTracker::Block(BlockPosTracker::new(target_pos)));
                c.set(&walk_target, WalkTarget::new(PositionTracker::Block(BlockPosTracker::new(target_pos)), speed_modifier, 1));
            }
            next_ok_start_time = c.timestamp + COOLDOWN_TICKS;
            true
        }
    })
}

fn can_stand_on(level: &ServerLevel, pos: BlockPos, state: BlockState, context: &CollisionContext) -> bool {
    let below = pos.relative(Direction::Down);
    state.get_collision_shape_ctx(level, pos, context).is_empty() && level.get_block_state(below).is_face_sturdy_at(level, below, Direction::Up)
}
