//! `SculkShriekerBlockEntity`: `warning_level` and the vibration system's `listener` data. Shrieking/warden warnings and
//! the vibration listener are not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use crate::world::level::gameevent::vibrations::VibrationData;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

pub struct SculkShriekerBlockEntity {
    base: BlockEntityData,
    pub warning_level: i32,
    pub vibration_data: VibrationData,
}

impl SculkShriekerBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::SCULK_SHRIEKER, pos, state), warning_level: 0, vibration_data: VibrationData::default() }
    }
}

impl BlockEntity for SculkShriekerBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.warning_level = tag.get_int_or("warning_level", 0);
        self.vibration_data = read_codec(tag, "listener", VibrationData::codec()).unwrap_or_default();
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("warning_level", self.warning_level);
        if let Ok(listener) = mc_dfu::encode_nbt(VibrationData::codec(), &self.vibration_data).into_result() {
            tag.put("listener", listener);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SCULK_SHRIEKER: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::SCULK_SHRIEKER, create: |pos, state| Box::new(SculkShriekerBlockEntity::new(pos, state)) };
