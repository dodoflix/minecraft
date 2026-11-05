//! `DaylightDetectorBlockEntity`: no saved data; its ticker (`DaylightDetectorBlock.tickEntity`) refreshes the
//! signal every 20 ticks in dimensions with sky light.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::level::block::impls::daylight_detector_block::update_signal_strength;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;

pub struct DaylightDetectorBlockEntity {
    base: BlockEntityData,
}

impl DaylightDetectorBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::DAYLIGHT_DETECTOR, pos, state) }
    }
}

impl BlockEntity for DaylightDetectorBlockEntity {
    block_entity_base!();

    /// `getTicker`: only server levels whose dimension has sky light tick (decided per call, see `server_tick`).
    fn is_ticking(&self) -> bool {
        true
    }

    /// `DaylightDetectorBlock.tickEntity`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        if level.chunk_source.chunk_map.has_sky_light && crate::world::level::LevelAccessor::get_game_time(level) % 20 == 0 {
            let state = crate::world::level::BlockGetter::get_block_state(level, self.base.pos);
            update_signal_strength(state, level, self.base.pos);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static DAYLIGHT_DETECTOR: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::DAYLIGHT_DETECTOR, create: |pos, state| Box::new(DaylightDetectorBlockEntity::new(pos, state)) };
