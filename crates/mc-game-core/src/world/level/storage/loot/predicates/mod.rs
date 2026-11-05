//! `loot.predicates`: `LootItemCondition` (the `predicate` registry and every inline loot condition).

mod conditions;

use std::any::Any;
use std::sync::{Arc, LazyLock};

use mc_dfu::Codec;

pub use conditions::*;

use super::context::{ContextKey, ContextKeySet, LootContext};
use super::holder::{Holder, HolderSet, TypeTable, holder_codec, holder_set_codec};
use super::validation::{ProblemCollector, ValidatableData, ValidationContext};
use crate::registry::keys;

/// `LootItemCondition`.
pub trait LootItemCondition: Send + Sync {
    /// The registered type name (`LootItemConditionTypes`).
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `test(context)`.
    fn test(&self, context: &mut LootContext) -> bool;
    /// `getReferencedContextParams()`.
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    /// `validate(context)` (`LootContextUser`: the referenced parameters must be provided).
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

impl dyn LootItemCondition {
    /// `Validatable.validatorForContext(params)`: the first problems of validating against `params` as an error.
    pub fn validate_in(&self, params: &ContextKeySet) -> Result<(), String> {
        let problems = ProblemCollector::new();
        let context = ValidationContext::without_resolver(&problems, *params);
        self.validate(&context);
        if problems.is_empty() { Ok(()) } else { Err(format!("Validation error: {}", problems.get_report())) }
    }
}

impl ValidatableData for dyn LootItemCondition {
    const REGISTRY: &'static str = keys::PREDICATE;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

pub type ConditionRef = Arc<dyn LootItemCondition>;
pub type ConditionHolder = Holder<dyn LootItemCondition>;
pub type ConditionHolderSet = HolderSet<dyn LootItemCondition>;

static TYPES: LazyLock<Arc<TypeTable<dyn LootItemCondition>>> = LazyLock::new(conditions::type_table);

/// `LootItemCondition.DIRECT_CODEC`.
pub fn condition_codec() -> Codec<ConditionRef> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<ConditionRef>> = LazyLock::new(|| TYPES.codec(|c| c.type_name()));
        C.clone()
    })
}

/// `LootItemCondition.CODEC` (`RegistryCodecs.holder(PREDICATE, DIRECT_CODEC)`).
pub fn condition_holder_codec() -> Codec<ConditionHolder> {
    holder_codec(keys::PREDICATE, condition_codec())
}

/// `LootItemCondition.LIST_CODEC`.
pub fn condition_list_codec() -> Codec<ConditionHolderSet> {
    holder_set_codec(keys::PREDICATE, condition_codec())
}
