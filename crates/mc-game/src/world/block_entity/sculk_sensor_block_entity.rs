//! `SculkSensorBlockEntity` (also the base of the calibrated sensor): `last_vibration_frequency` and the vibration
//! system's `listener` data. The vibration listener/user logic is not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use crate::world::level::gameevent::vibrations::VibrationData;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

pub struct SculkSensorBlockEntity {
    base: BlockEntityData,
    pub vibration_data: VibrationData,
    pub last_vibration_frequency: i32,
}

impl SculkSensorBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self::with_type(BlockEntityType::SCULK_SENSOR, pos, state)
    }

    /// The protected constructor taking the type (`CalibratedSculkSensorBlockEntity`).
    pub(super) fn with_type(ty: BlockEntityType, pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(ty, pos, state), vibration_data: VibrationData::default(), last_vibration_frequency: 0 }
    }
}

impl BlockEntity for SculkSensorBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.last_vibration_frequency = tag.get_int_or("last_vibration_frequency", 0);
        self.vibration_data = read_codec(tag, "listener", VibrationData::codec()).unwrap_or_default();
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("last_vibration_frequency", self.last_vibration_frequency);
        if let Ok(listener) = mc_dfu::encode_nbt(VibrationData::codec(), &self.vibration_data).into_result() {
            tag.put("listener", listener);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SCULK_SENSOR: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::SCULK_SENSOR, create: |pos, state| Box::new(SculkSensorBlockEntity::new(pos, state)) };
