//! `loot.functions`: `LootItemFunction` (the `item_modifier` registry and every inline loot function).

mod basic;
mod containers;
mod enchant;
pub mod items;
pub mod list_operation;

use std::any::Any;
use std::sync::{Arc, LazyLock};

use mc_dfu::{Codec, Either, MapCodec};
use mc_protocol::components::ItemStack;

use super::context::{ContextKey, LootContext};
use super::holder::{Holder, HolderSet, TypeTable, holder_codec, holder_set_codec};
use super::predicates::{ConditionHolder, condition_holder_codec};
use super::validation::{ValidatableData, ValidationContext, validate_holder_opt};
use crate::registry::keys;

pub use basic::SequenceFunction;
pub use containers::ContainerComponentManipulator;

/// `LootItemFunction`.
pub trait LootItemFunction: Send + Sync {
    /// The registered type name (`LootItemFunctions.bootstrap`).
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `apply(stack, context)`.
    fn apply(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

/// A function that extends `LootItemConditionalFunction`.
pub trait ConditionalFunction: Send + Sync + 'static {
    fn type_name(&self) -> &'static str;
    fn condition(&self) -> &Option<ConditionHolder>;
    /// `run(stack, context)`.
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    /// The subclass part of `validate` (after the condition).
    fn validate_fields(&self, _context: &ValidationContext) {}
}

impl<T: ConditionalFunction> LootItemFunction for T {
    fn type_name(&self) -> &'static str {
        ConditionalFunction::type_name(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn apply(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(condition) = self.condition() {
            if !condition.value(context.get_resolver()).test(context) {
                return stack;
            }
        }
        self.run(stack, context)
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        ConditionalFunction::referenced_params(self)
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&ConditionalFunction::referenced_params(self));
        validate_holder_opt(context, "condition", self.condition());
        self.validate_fields(context);
    }
}

impl ValidatableData for dyn LootItemFunction {
    const REGISTRY: &'static str = keys::ITEM_MODIFIER;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

pub type LootItemFunctionRef = Arc<dyn LootItemFunction>;
pub type FunctionHolder = Holder<dyn LootItemFunction>;
pub type FunctionHolderSet = HolderSet<dyn LootItemFunction>;

/// `commonFields`: the optional `condition`.
pub(crate) fn condition_field() -> MapCodec<Option<ConditionHolder>> {
    condition_holder_codec().optional_field_of("condition")
}

/// `LootItemFunction.decorate`: the stack through `function` (if any) before `output`.
pub fn decorate<'s>(
    function: &'s Option<FunctionHolder>,
    output: &'s mut dyn FnMut(ItemStack, &mut LootContext),
) -> impl FnMut(ItemStack, &mut LootContext) + 's {
    move |stack, context| {
        let stack = match function {
            Some(f) => f.value(context.get_resolver()).apply(stack, context),
            None => stack,
        };
        output(stack, context)
    }
}

static TYPES: LazyLock<Arc<TypeTable<dyn LootItemFunction>>> = LazyLock::new(|| {
    let mut entries = Vec::new();
    basic::register(&mut entries);
    enchant::register(&mut entries);
    items::register(&mut entries);
    containers::register(&mut entries);
    TypeTable::new("minecraft:loot_function_type", entries)
});

/// `LootItemFunctions.TYPED_CODEC`.
pub fn typed_function_codec() -> Codec<LootItemFunctionRef> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<LootItemFunctionRef>> = LazyLock::new(|| TYPES.codec(|f| f.type_name()));
        C.clone()
    })
}

/// `LootItemFunctions.DIRECT_CODEC`: a typed function, or a bare list of functions (`SequenceFunction.INLINE_CODEC`).
pub fn function_codec() -> Codec<LootItemFunctionRef> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<LootItemFunctionRef>> = LazyLock::new(|| {
            let typed = typed_function_codec();
            let inline = function_list_codec_typed().xmap(|functions| SequenceFunction::new(None, functions), |s: &SequenceFunction| s.functions().clone());
            typed.either(&inline).xmap(
                |e| match e {
                    Either::Left(f) => f,
                    Either::Right(s) => Arc::new(s) as LootItemFunctionRef,
                },
                |f: &LootItemFunctionRef| match f.as_any().downcast_ref::<SequenceFunction>() {
                    Some(s) if s.can_use_inline_codec() => Either::Right(s.clone()),
                    _ => Either::Left(f.clone()),
                },
            )
        });
        C.clone()
    })
}

/// `LootItemFunctions.CODEC` (`RegistryCodecs.holder(ITEM_MODIFIER, DIRECT_CODEC)`).
pub fn function_holder_codec() -> Codec<FunctionHolder> {
    holder_codec(keys::ITEM_MODIFIER, function_codec())
}

/// `LootItemFunctions.LIST_CODEC` (`holderSet(ITEM_MODIFIER, TYPED_CODEC)`).
pub fn function_list_codec_typed() -> Codec<FunctionHolderSet> {
    holder_set_codec(keys::ITEM_MODIFIER, typed_function_codec())
}
