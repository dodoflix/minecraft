//! `RingBell`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::MEETING_POINT;
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor as _;
use crate::world::level::block::impls::bell_block;
use mc_data::block::Block;

const BELL_RING_CHANCE: f32 = 0.95;
pub const RING_BELL_FROM_DISTANCE: f64 = 3.0;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let meeting_point = i.present(&MEETING_POINT);
    i.create(move |c| {
        if c.level.random().next_float() <= BELL_RING_CHANCE {
            return false;
        }
        let pos = c.get(&meeting_point).pos;
        if pos.closer_than(&c.body().block_position(), RING_BELL_FROM_DISTANCE) {
            let state = c.level.get_block_state(pos);
            if state.block() == Block::BELL {
                let source = c.body().id();
                bell_block::attempt_to_ring(c.level, Some(source), pos, None);
            }
        }
        true
    })
}
