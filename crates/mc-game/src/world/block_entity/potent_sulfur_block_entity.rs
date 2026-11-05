//! `PotentSulfurBlockEntity`: saves `countdown` (`waitingCountdown`, `-1` = not started). The geyser/nausea tickers are
//! not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;

pub struct PotentSulfurBlockEntity {
    base: BlockEntityData,
    pub waiting_countdown: i32,
    pub eruption_tick: i64,
}

impl PotentSulfurBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::POTENT_SULFUR, pos, state), waiting_countdown: -1, eruption_tick: -1 }
    }
}

impl BlockEntity for PotentSulfurBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        if let Some(countdown) = tag.get("countdown").and_then(|t| t.as_number()) {
            self.waiting_countdown = countdown.int_value();
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_int("countdown", self.waiting_countdown);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static POTENT_SULFUR: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::POTENT_SULFUR, create: |pos, state| Box::new(PotentSulfurBlockEntity::new(pos, state)) };
