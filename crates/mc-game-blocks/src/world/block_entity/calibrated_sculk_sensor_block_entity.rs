//! `CalibratedSculkSensorBlockEntity`: a sensor entity of its own type (listener radius 16 and the back-signal filter
//! are part of its vibration user, see `vibration::BlockEntityVibration`), saved exactly like [`SculkSensorBlockEntity`].

use linkme::distributed_slice;
use mc_data::BlockEntityType;
use mc_game_core::world::block_entity::sculk_sensor_block_entity::SculkSensorBlockEntity;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntityRegistration};

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CALIBRATED_SCULK_SENSOR: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CALIBRATED_SCULK_SENSOR,
    create: |pos, state| {
        Box::new(SculkSensorBlockEntity::with_type(
            BlockEntityType::CALIBRATED_SCULK_SENSOR,
            pos,
            state,
        ))
    },
};
