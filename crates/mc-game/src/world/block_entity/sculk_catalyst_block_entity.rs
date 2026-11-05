//! `SculkCatalystBlockEntity`: saves the cursors of its level spreader (`cursors`). The game event listener and the
//! spreading tick are not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::world::level::block::sculk_spreader::SculkSpreader;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

pub struct SculkCatalystBlockEntity {
    base: BlockEntityData,
    pub sculk_spreader: SculkSpreader,
}

impl SculkCatalystBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::SCULK_CATALYST, pos, state), sculk_spreader: SculkSpreader::create_level_spreader() }
    }
}

impl BlockEntity for SculkCatalystBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.sculk_spreader.load(tag);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.sculk_spreader.save(tag);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static SCULK_CATALYST: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::SCULK_CATALYST, create: |pos, state| Box::new(SculkCatalystBlockEntity::new(pos, state)) };
