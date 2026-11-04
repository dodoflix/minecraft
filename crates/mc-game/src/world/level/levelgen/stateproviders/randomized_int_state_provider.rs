//! `RandomizedIntStateProvider` (`randomized_int`).

use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::Holder;
use crate::world::level::levelgen::codec::{TypeEntry, parse_str};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::props::PropertyKind;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

/// Vanilla caches the found `IntegerProperty` in a mutable field, but re-resolves it whenever the state lacks it, so
/// the result equals looking the property up by name on every call.
pub struct RandomizedIntStateProvider {
    pub source: Holder<dyn BlockStateProvider>,
    pub property_name: String,
    pub values: IntProvider,
}

impl BlockStateProvider for RandomizedIntStateProvider {
    fn get_state(&self, level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        let unmodified_state = self.source.value().get_state(level, random, pos);
        // `findProperty`: the `IntegerProperty` of the state named `propertyName`.
        if !unmodified_state.block().properties().any(|p| p.name == self.property_name && p.kind == PropertyKind::Integer) {
            return unmodified_state;
        }
        let value = self.values.sample(random);
        // `setValue` throws for a value outside the property's range.
        unmodified_state
            .with_property_str(&self.property_name, &value.to_string())
            .unwrap_or_else(|| panic!("Cannot set property {} to {value} on {}", self.property_name, unmodified_state.block()))
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static RANDOMIZED_INT: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "randomized_int",
    decode: |fields, ctx| {
        let source = fields.required("source", |v| ctx.block_state_provider(v))?;
        let property_name = fields.required("property", |v| parse_str(v).map(str::to_owned))?;
        let values = fields.required("values", IntProvider::from_json)?;
        Ok(Arc::new(RandomizedIntStateProvider { source, property_name, values }))
    },
};
