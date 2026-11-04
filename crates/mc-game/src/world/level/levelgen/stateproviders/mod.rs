//! `net.minecraft.world.level.levelgen.feature.stateproviders`: `BlockStateProvider`, `SimpleStateProvider` and the type
//! table. Each provider type lives in its own file and registers itself in [`BLOCK_STATE_PROVIDER_TYPES`].

use super::codec::{Fields, LoadError, LoadResult, TypeEntry, decode_dispatch, parse_block_state};
use super::registries::DecodeCtx;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub trait BlockStateProvider: Send + Sync {
    /// `getState(level, random, pos)`.
    fn get_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState;

    /// `getOptionalState(level, random, pos)`.
    fn get_optional_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> Option<BlockState> {
        Some(self.get_state(level, random, pos))
    }
}

/// `SimpleStateProvider`.
pub struct SimpleStateProvider {
    pub state: BlockState,
}

impl BlockStateProvider for SimpleStateProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, _random: &mut dyn RandomSource, _pos: BlockPos) -> BlockState {
        self.state
    }
}

#[linkme::distributed_slice]
pub static BLOCK_STATE_PROVIDER_TYPES: [TypeEntry<dyn BlockStateProvider>];

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static SIMPLE_STATE_PROVIDER: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "simple_state_provider",
    decode: |fields, _| Ok(Arc::new(SimpleStateProvider { state: fields.required("state", parse_block_state)? })),
};

/// `BlockStateProvider.DIRECT_CODEC`: `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)` (an object with `type` is a typed
/// provider, an object with `id` a block state, i.e. a `SimpleStateProvider`).
pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn BlockStateProvider>> {
    let fields = Fields::new(value)?;
    let typed = fields.get("type").is_some().then(|| decode_dispatch(value, ctx, &BLOCK_STATE_PROVIDER_TYPES));
    let full = fields.get("id").is_some().then(|| parse_block_state(value));
    match (typed, full) {
        (Some(Ok(_)), Some(Ok(_))) => Err(LoadError::new("Both alternatives read successfully, can not pick the correct one")),
        (Some(Ok(provider)), _) => Ok(provider),
        (_, Some(Ok(state))) => Ok(Arc::new(SimpleStateProvider { state })),
        (Some(Err(e)), _) => Err(e),
        (None, Some(Err(e))) => Err(e),
        (None, None) => Err(LoadError::new("No key id in MapLike")),
    }
}
