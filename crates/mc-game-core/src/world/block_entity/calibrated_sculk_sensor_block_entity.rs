//! `CalibratedSculkSensorBlockEntity`: a sensor entity of its own type (listener radius 16 and the back-signal filter
//! are part of its vibration user, see `vibration::BlockEntityVibration`), saved exactly like [`SculkSensorBlockEntity`].

use super::sculk_sensor_block_entity::SculkSensorBlockEntity;
use super::{BLOCK_ENTITY_TYPES, BlockEntityRegistration};
use linkme::distributed_slice;
use mc_data::BlockEntityType;

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CALIBRATED_SCULK_SENSOR: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CALIBRATED_SCULK_SENSOR,
    create: |pos, state| Box::new(SculkSensorBlockEntity::with_type(BlockEntityType::CALIBRATED_SCULK_SENSOR, pos, state)),
};
