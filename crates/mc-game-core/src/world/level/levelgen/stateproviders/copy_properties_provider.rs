//! `CopyPropertiesProvider` (`copy_properties`).

use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct CopyPropertiesProvider {
    pub source: Holder<dyn BlockStateProvider>,
}

impl BlockStateProvider for CopyPropertiesProvider {
    fn get_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        let state = self.source.value().get_state(level, random, pos);
        state.with_properties_of(level.get_block_state(pos))
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static COPY_PROPERTIES: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "copy_properties",
    decode: |fields, ctx| Ok(Arc::new(CopyPropertiesProvider { source: fields.required("source", |v| ctx.block_state_provider(v))? })),
};
