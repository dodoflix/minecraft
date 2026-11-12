//! `DaylightDetectorBlockEntity`: no saved data; its ticker (`DaylightDetectorBlock.tickEntity`) refreshes the
//! signal every 20 ticks in dimensions with sky light.

use crate::world::level::block::impls::daylight_detector_block::update_signal_strength;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration,
};

pub struct DaylightDetectorBlockEntity {
    base: BlockEntityData,
}

impl DaylightDetectorBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::DAYLIGHT_DETECTOR, pos, state),
        }
    }
}

impl BlockEntity for DaylightDetectorBlockEntity {
    block_entity_base!();

    /// `getTicker`: only levels whose dimension has sky light get a ticker.
    fn is_ticking_in(&self, level: &ServerLevel) -> bool {
        level.chunk_source.chunk_map.has_sky_light
    }

    /// `DaylightDetectorBlock.tickEntity`.
    fn server_tick(&mut self, level: &mut ServerLevel) {
        if mc_game_core::world::level::LevelAccessor::get_game_time(level) % 20 == 0 {
            let state =
                mc_game_core::world::level::BlockGetter::get_block_state(level, self.base.pos);
            update_signal_strength(state, level, self.base.pos);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static DAYLIGHT_DETECTOR: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::DAYLIGHT_DETECTOR,
    create: |pos, state| Box::new(DaylightDetectorBlockEntity::new(pos, state)),
};
