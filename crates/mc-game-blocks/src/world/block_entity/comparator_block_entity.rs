//! `ComparatorBlockEntity`: the remembered output signal (`OutputSignal`).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration,
};
use mc_nbt::CompoundTag;

pub struct ComparatorBlockEntity {
    base: BlockEntityData,
    output: i32,
}

impl ComparatorBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::COMPARATOR, pos, state),
            output: 0,
        }
    }

    /// `getOutputSignal`.
    pub fn get_output_signal(&self) -> i32 {
        self.output
    }

    /// `setOutputSignal` (vanilla does not `setChanged`).
    pub fn set_output_signal(&mut self, value: i32) {
        self.output = value;
    }
}

impl BlockEntity for ComparatorBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.output = tag.get_int_or("OutputSignal", 0);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("OutputSignal", self.output);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static COMPARATOR: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::COMPARATOR,
    create: |pos, state| Box::new(ComparatorBlockEntity::new(pos, state)),
};
