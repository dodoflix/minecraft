//! `TryFindLiquid`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{BlockPosTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, LOOK_TARGET, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::fluid::Fluid;

/// `create(range, speedModifier, fluidTag)`.
pub fn create(range: i32, speed_modifier: f32, fluid_tag: &'static str) -> OneShot {
    let mut next_ok_start_time: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let _attack_target = i.absent(&ATTACK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    i.create(move |c| {
        let fluid = mc_data::tags::get::<Fluid>(fluid_tag);
        let is_fluid = |state: mc_data::block::BlockState| fluid.is_some_and(|t| t.contains(state.get_fluid_state().fluid()));
        if is_fluid(c.level.get_block_state(c.body.block_position())) {
            false
        } else if c.timestamp < next_ok_start_time {
            next_ok_start_time = c.timestamp + 20 + 2;
            true
        } else {
            let mut found_pos: Option<BlockPos> = None;
            let body_block_pos = c.body.block_position();
            let body_position = c.body.position();
            let level = &*c.level;
            for pos in BlockPos::within_box_by_manhattan_distance(&body_block_pos, range, range, range) {
                if !pos.differs_horizontally(&body_block_pos) {
                    continue;
                }
                let state = level.get_block_state(pos);
                // `state.getBlock() instanceof LiquidBlock && state.getFluidState().is(fluidTag)`
                if !((state.block() == Block::WATER || state.block() == Block::LAVA) && is_fluid(state)) {
                    continue;
                }
                if level.get_block_state(pos.above()).is_air() {
                    found_pos = Some(pos);
                    break;
                }
                if found_pos.is_none() && !pos.closer_to_center_than(&body_position, 1.5) {
                    found_pos = Some(pos);
                }
            }
            if let Some(block_pos) = found_pos {
                c.set(&look_target, PositionTracker::Block(BlockPosTracker::new(block_pos)));
                c.set(&walk_target, WalkTarget::new(PositionTracker::Block(BlockPosTracker::new(block_pos)), speed_modifier, 0));
            }
            next_ok_start_time = c.timestamp + 40;
            true
        }
    })
}
