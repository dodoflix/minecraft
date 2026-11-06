//! `net.minecraft.world.level.levelgen.feature.stateproviders`: `BlockStateProvider` and the type table. Each provider
//! type lives in its own file and registers itself in [`BLOCK_STATE_PROVIDER_TYPES`].

use super::codec::{Fields, LoadError, LoadResult, TypeEntry, decode_dispatch, parse_block_state};
use super::registries::DecodeCtx;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub mod simple_state_provider;

pub use simple_state_provider::SimpleStateProvider;

use super::holder::Holder;

pub trait BlockStateProvider: Send + Sync {
    /// `getState(level, random, pos)`.
    fn get_state(
        &self,
        level: &dyn LevelAccessor,
        random: &mut dyn RandomSource,
        pos: BlockPos,
    ) -> BlockState;

    /// `getOptionalState(level, random, pos)`.
    fn get_optional_state(
        &self,
        level: &dyn LevelAccessor,
        random: &mut dyn RandomSource,
        pos: BlockPos,
    ) -> Option<BlockState> {
        Some(self.get_state(level, random, pos))
    }
}

#[linkme::distributed_slice]
pub static BLOCK_STATE_PROVIDER_TYPES: [TypeEntry<dyn BlockStateProvider>];

/// `BlockStateProvider.DIRECT_CODEC`: `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)` mapped to a provider. `FULL_CODEC`
/// reads a block state from an object with `id`, `TYPED_CODEC` a provider from an object with `type`; exactly one of
/// them must succeed (a `SimpleStateProvider` results from the former).
pub fn decode_direct(
    value: &Value,
    ctx: &DecodeCtx<'_>,
) -> LoadResult<Arc<dyn BlockStateProvider>> {
    let fields = Fields::new(value)?;
    let full = match fields.get("id") {
        Some(_) => parse_block_state(value),
        None => Err(LoadError::new("No key id in MapLike")),
    };
    let typed = match fields.get("type") {
        Some(_) => decode_dispatch(value, ctx, &BLOCK_STATE_PROVIDER_TYPES),
        None => Err(LoadError::new("No key type in MapLike")),
    };
    match (full, typed) {
        (Ok(_), Ok(_)) => Err(LoadError::new(
            "Both alternatives read successfully, can not pick the correct one",
        )),
        (Ok(state), Err(_)) => Ok(Arc::new(SimpleStateProvider { state })),
        (Err(_), Ok(provider)) => Ok(provider),
        (Err(first), Err(second)) => Err(LoadError::new(format!("{second}; {first}"))),
    }
}

/// `BlockStateProvider.of(state)`.
pub fn of(state: BlockState) -> Arc<SimpleStateProvider> {
    Arc::new(SimpleStateProvider { state })
}

/// `BlockStateProvider.holderOf(state)`.
pub fn holder_of(state: BlockState) -> Holder<dyn BlockStateProvider> {
    Holder::direct(of(state))
}
