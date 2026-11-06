//! `TryFindLandNearLiquid`.

use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::fluid::Fluid;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{
    BlockPosTracker, PositionTracker,
};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, LOOK_TARGET, WALK_TARGET,
};
use mc_game_core::world::entity::movement::collision_context;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::material::FluidStateExt;

/// `create(range, speedModifier, fluidTag)`.
pub fn create(range: i32, speed_modifier: f32, fluid_tag: &'static str) -> OneShot {
    let mut next_ok_start_time: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let _attack_target = i.absent(&ATTACK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    i.create(move |c| {
        let fluid = mc_data::tags::get::<Fluid>(fluid_tag);
        if fluid
            .is_some_and(|t| t.contains(c.level.get_fluid_state(c.body.block_position()).fluid()))
        {
            false
        } else if c.timestamp < next_ok_start_time {
            next_ok_start_time = c.timestamp + 40;
            true
        } else {
            let context = collision_context(&*c.body);
            let body_block_pos = c.body.block_position();
            let level = &*c.level;
            let found =
                BlockPos::within_box_by_manhattan_distance(&body_block_pos, range, range, range)
                    .find(|&pos| {
                        if !pos.differs_horizontally(&body_block_pos) {
                            return false;
                        }
                        let state = level.get_block_state(pos);
                        if !state
                            .get_collision_shape_ctx(level, pos, &context)
                            .is_empty()
                        {
                            false
                        } else if level
                            .get_block_state(pos.relative(Direction::Down))
                            .get_collision_shape_ctx(level, pos, &context)
                            .is_empty()
                        {
                            false
                        } else {
                            Direction::HORIZONTAL.iter().any(|&direction| {
                                let test_pos = pos.relative(direction);
                                level.get_block_state(test_pos).is_air()
                                    && fluid.is_some_and(|t| {
                                        t.contains(
                                            level
                                                .get_block_state(test_pos.relative(Direction::Down))
                                                .get_fluid_state()
                                                .fluid(),
                                        )
                                    })
                            })
                        }
                    });
            if let Some(target_pos) = found {
                c.set(
                    &look_target,
                    PositionTracker::Block(BlockPosTracker::new(target_pos)),
                );
                c.set(
                    &walk_target,
                    WalkTarget::new(
                        PositionTracker::Block(BlockPosTracker::new(target_pos)),
                        speed_modifier,
                        0,
                    ),
                );
            }
            next_ok_start_time = c.timestamp + 40;
            true
        }
    })
}
