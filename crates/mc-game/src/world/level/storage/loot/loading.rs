//! Loading of the reloadable loot registries (`ReloadableServerRegistries`): the typed decoders of `loot_table`,
//! `predicate`, `item_modifier`, `context_int_provider`, `context_float_provider` and `slot_source`, and the validation
//! pass (`LootDataType.runValidation`) that runs after a (re)load.

use std::sync::Arc;

use mc_dfu::{Codec, DataResult, Dyn, JSON};
use serde_json::Value;

use super::functions::{LootItemFunctionRef, function_codec};
use super::holder::with_decode_context;
use super::parameters::LootContextParamSets;
use super::predicates::{ConditionRef, condition_codec};
use super::providers::number::{FloatProviderRef, IntProviderRef, float_provider_codec, int_provider_codec};
use super::table::{LootTable, loot_table_direct_codec};
use super::validation::{ProblemCollector, ValidationContextSource};
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::world::item::slot::{SlotSourceRef, slot_source_codec};

fn parse<A: Send + Sync + 'static>(codec: Codec<A>, json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String>
where
    A: Clone,
{
    let dyn_value = Dyn::from_json_str(&json.to_string()).map_err(|e| format!("Failed to parse JSON: {e}"))?;
    match with_decode_context(ctx, || codec.parse(&JSON, &dyn_value)) {
        DataResult::Success(v) => Ok(DecodedEntry::new(v)),
        DataResult::Error { message, .. } => Err(message.get()),
    }
}

fn decode_table(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<Arc<LootTable>>(loot_table_direct_codec(), json, ctx)
}
fn decode_predicate(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<ConditionRef>(condition_codec(), json, ctx)
}
fn decode_modifier(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<LootItemFunctionRef>(function_codec(), json, ctx)
}
fn decode_int_provider(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<IntProviderRef>(int_provider_codec(), json, ctx)
}
fn decode_float_provider(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<FloatProviderRef>(float_provider_codec(), json, ctx)
}
fn decode_slot_source(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    parse::<SlotSourceRef>(slot_source_codec(), json, ctx)
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static LOOT_TABLE_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::LOOT_TABLE, decode: decode_table };
#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static PREDICATE_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::PREDICATE, decode: decode_predicate };
#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static ITEM_MODIFIER_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::ITEM_MODIFIER, decode: decode_modifier };
#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static INT_PROVIDER_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CONTEXT_INT_PROVIDER, decode: decode_int_provider };
#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static FLOAT_PROVIDER_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::CONTEXT_FLOAT_PROVIDER, decode: decode_float_provider };
#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static SLOT_SOURCE_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::SLOT_SOURCE, decode: decode_slot_source };

/// `LootTables`: the table registered as `id` (`ReloadableServerRegistries.Holder.getLootTable`: `LootTable.EMPTY` when
/// absent).
pub fn get_loot_table(registries: &RegistryAccess, id: &mc_core::ident::Identifier) -> Arc<LootTable> {
    registries.get::<Arc<LootTable>>(keys::LOOT_TABLE, id).cloned().unwrap_or_else(LootTable::empty)
}

/// The loot table of `id`, if one is registered.
pub fn find_loot_table(registries: &RegistryAccess, id: &mc_core::ident::Identifier) -> Option<Arc<LootTable>> {
    registries.get::<Arc<LootTable>>(keys::LOOT_TABLE, id).cloned()
}

/// `ReloadableServerRegistries.validateLootRegistries(fullContextWithNewTags)`: logs every problem and fails on fatal ones.
pub fn validate_loot_registries(registries: &RegistryAccess) -> Result<(), String> {
    let problems = ProblemCollector::new();
    let source = ValidationContextSource::new(&problems, registries);
    let all_params = LootContextParamSets::ALL_PARAMS;
    let mut run = |registry: &'static str, validate: &mut dyn FnMut(&super::validation::ValidationContext, &RegistryAccess, &mc_core::ident::Identifier)| {
        let Some(reg) = registries.registry(registry) else { return };
        for entry in reg.entries() {
            let root = source.context(all_params);
            // `LootDataType.TABLE` takes the table's own parameter set; the others `ALL_PARAMS`
            validate(&root, registries, &entry.id);
        }
    };
    use super::validation::ValidatableData;
    macro_rules! each {
        ($reg:expr, $t:ty, $params:expr) => {
            run($reg, &mut |_root, access, id| {
                if let Some(v) = access.get::<$t>($reg, id) {
                    let params: super::context::ContextKeySet = $params(v);
                    let ctx = source.context(params).enter_element(&format!("{{{id}@{}}}", $reg), $reg, id);
                    v.validate_value(&ctx);
                }
            })
        };
    }
    each!(keys::PREDICATE, ConditionRef, |_| all_params);
    each!(keys::ITEM_MODIFIER, LootItemFunctionRef, |_| all_params);
    each!(keys::SLOT_SOURCE, SlotSourceRef, |_| all_params);
    each!(keys::LOOT_TABLE, Arc<LootTable>, |t: &Arc<LootTable>| t.get_param_set());
    each!(keys::CONTEXT_FLOAT_PROVIDER, FloatProviderRef, |_| all_params);
    each!(keys::CONTEXT_INT_PROVIDER, IntProviderRef, |_| all_params);
    problems.for_each(|id, problem| log::warn!("Found loot table element validation problem in {id}: {}", problem.description));
    if problems.has_fatal_problems() { Err("Found loot table validation problems".to_owned()) } else { Ok(()) }
}
