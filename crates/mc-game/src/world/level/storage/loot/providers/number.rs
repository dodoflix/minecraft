//! `loot.providers.number.{ints,floats}`: `ContextIntProvider` / `ContextFloatProvider` (the `context_int_provider` /
//! `context_float_provider` registries and every inline number provider of loot tables).

use std::any::Any;
use std::sync::{Arc, LazyLock};

use mc_core::math::java::{floor_mod_i32, round_f32};
use mc_core::random::RandomSource;
use mc_dfu::extra::NON_NEGATIVE_INT;
use mc_dfu::{BOOL, Codec, DataResult, Either, FLOAT, INT, MapCodec, record};
use mc_nbt::{Number, Tag};

use crate::registry::keys;
use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::holder::{Holder, HolderSet, TypeTable, holder_codec, holder_set_codec, lift};
use crate::world::level::storage::loot::predicates::{ConditionHolder, condition_holder_codec};
use crate::world::level::storage::loot::providers::score::{ScoreboardNameProvider, scoreboard_name_provider_codec};
use crate::world::level::storage::loot::validation::{
    Validatable, ValidatableData, ValidationContext, validate_holder, validate_holder_field, validate_holder_list, validate_holder_set_min,
};
use crate::world::level::storage::loot::server_access;

/// `java.lang.ArithmeticException`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArithmeticException;

pub type IntResult = Result<i32, ArithmeticException>;
pub type FloatResult = Result<f32, ArithmeticException>;

// ---- traits -------------------------------------------------------------------------------------------------------------

/// `ContextIntProvider`.
pub trait ContextIntProvider: Send + Sync {
    /// The registered type name (`ContextIntProviderTypes`).
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `getIntUnsafe(context)`.
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult;
    /// `getReferencedContextParams()` (`LootContextUser`).
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(&self, _context: &ValidationContext) {}
}

/// `ContextFloatProvider`.
pub trait ContextFloatProvider: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `getFloatUnsafe(context)`.
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(&self, _context: &ValidationContext) {}
}

impl dyn ContextIntProvider {
    /// `getInt(context)`: `0` on an `ArithmeticException`.
    pub fn get_int(&self, context: &mut LootContext) -> i32 {
        self.get_int_unsafe(context).unwrap_or(0)
    }
}

impl dyn ContextFloatProvider {
    /// `getFloat(context)`: `0` for non-finite results and arithmetic exceptions.
    pub fn get_float(&self, context: &mut LootContext) -> f32 {
        match self.get_float_unsafe(context) {
            Ok(v) if v.is_finite() => v,
            _ => 0.0,
        }
    }

    /// `getFloatOrThrow(context)`.
    pub fn get_float_or_throw(&self, context: &mut LootContext) -> FloatResult {
        let v = self.get_float_unsafe(context)?;
        if v.is_finite() { Ok(v) } else { Err(ArithmeticException) }
    }
}

impl ValidatableData for dyn ContextIntProvider {
    const REGISTRY: &'static str = keys::CONTEXT_INT_PROVIDER;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

impl ValidatableData for dyn ContextFloatProvider {
    const REGISTRY: &'static str = keys::CONTEXT_FLOAT_PROVIDER;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

pub type IntProviderRef = Arc<dyn ContextIntProvider>;
pub type FloatProviderRef = Arc<dyn ContextFloatProvider>;
pub type IntHolder = Holder<dyn ContextIntProvider>;
pub type FloatHolder = Holder<dyn ContextFloatProvider>;
pub type IntHolderSet = HolderSet<dyn ContextIntProvider>;
pub type FloatHolderSet = HolderSet<dyn ContextFloatProvider>;

/// `ContextIntProvider.longToIntSafe`.
pub fn long_to_int_safe(value: i64) -> IntResult {
    let result = value as i32;
    if i64::from(result) != value { Err(ArithmeticException) } else { Ok(result) }
}

/// `ContextIntProvider.floatToIntSafe`.
pub fn float_to_int_safe(value: f32) -> IntResult {
    if !value.is_finite() { Err(ArithmeticException) } else { long_to_int_safe(value as i64) }
}

// ---- codecs (ContextIntProviders / ContextFloatProviders) -----------------------------------------------------------

macro_rules! provider_codecs {
    (
        $trait:ident, $registry:expr, $direct:ident, $holder:ident, $set:ident, $table:ident, $const_ty:ident, $holder_ty:ident,
        $load_registry_name:literal
    ) => {
        static $table: LazyLock<Arc<TypeTable<dyn $trait>>> = LazyLock::new(types_for::<dyn $trait>);

        /// `DIRECT_CODEC`.
        pub fn $direct() -> Codec<Arc<dyn $trait>> {
            Codec::lazy(|| {
                static C: LazyLock<Codec<Arc<dyn $trait>>> = LazyLock::new(|| {
                    let typed = $table.codec(|e| e.type_name());
                    $const_ty::inline_codec().either(&typed).xmap(
                        |e| match e {
                            Either::Left(c) => Arc::new(c) as Arc<dyn $trait>,
                            Either::Right(p) => p,
                        },
                        |p: &Arc<dyn $trait>| match p.as_any().downcast_ref::<$const_ty>() {
                            Some(c) => Either::Left(c.clone()),
                            None => Either::Right(p.clone()),
                        },
                    )
                });
                C.clone()
            })
        }

        /// `CODEC` (`RegistryCodecs.holder`).
        pub fn $holder() -> Codec<Holder<dyn $trait>> {
            holder_codec($registry, $direct())
        }

        /// `LIST_CODEC`.
        pub fn $set() -> Codec<HolderSet<dyn $trait>> {
            holder_set_codec($registry, $direct())
        }
    };
}

/// The type table of one provider kind (the `bootstrap` of `ContextIntProviderTypes` / `ContextFloatProviderTypes`).
trait ProviderTypes {
    fn table() -> Arc<TypeTable<Self>>;
}

fn types_for<T: ?Sized + ProviderTypes + 'static>() -> Arc<TypeTable<T>> {
    T::table()
}

provider_codecs!(ContextIntProvider, keys::CONTEXT_INT_PROVIDER, int_provider_codec, int_holder_codec, int_list_codec, INT_TABLE, IntConstant, IntHolder, "int");
provider_codecs!(
    ContextFloatProvider,
    keys::CONTEXT_FLOAT_PROVIDER,
    float_provider_codec,
    float_holder_codec,
    float_list_codec,
    FLOAT_TABLE,
    FloatConstant,
    FloatHolder,
    "float"
);

/// `ContextIntProviders.exactly(value)`.
pub fn int_exactly(value: i32) -> IntHolder {
    Holder::Direct(Arc::new(IntConstant { value }))
}

/// `ContextFloatProviders.exactly(value)`.
pub fn float_exactly(value: f32) -> FloatHolder {
    Holder::Direct(Arc::new(FloatConstant { value }))
}

// ---- constants -----------------------------------------------------------------------------------------------------------

/// ints `ConstantValue`.
#[derive(Clone)]
pub struct IntConstant {
    pub value: i32,
}

impl IntConstant {
    fn map_codec() -> MapCodec<IntConstant> {
        record!(IntConstant { value: INT.field_of("value") })
    }
    fn inline_codec() -> Codec<IntConstant> {
        INT.xmap(|value| IntConstant { value }, |c: &IntConstant| c.value)
    }
}

impl ContextIntProvider for IntConstant {
    fn type_name(&self) -> &'static str {
        "constant"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, _context: &mut LootContext) -> IntResult {
        Ok(self.value)
    }
}

/// floats `ConstantValue`.
#[derive(Clone)]
pub struct FloatConstant {
    pub value: f32,
}

impl FloatConstant {
    fn map_codec() -> MapCodec<FloatConstant> {
        record!(FloatConstant { value: FLOAT.field_of("value") })
    }
    fn inline_codec() -> Codec<FloatConstant> {
        FLOAT.xmap(|value| FloatConstant { value }, |c: &FloatConstant| c.value)
    }
}

impl ContextFloatProvider for FloatConstant {
    fn type_name(&self) -> &'static str {
        "constant"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, _context: &mut LootContext) -> FloatResult {
        Ok(self.value)
    }
}

// ---- generic operator providers ---------------------------------------------------------------------------------------

/// `UnaryProvider`: `input`.
#[derive(Clone)]
pub struct Unary<H: Clone + 'static> {
    pub kind: &'static str,
    pub input: H,
}
/// `BinaryProvider`: `left`, `right`.
#[derive(Clone)]
pub struct Binary<H: Clone + 'static> {
    pub kind: &'static str,
    pub left: H,
    pub right: H,
}
/// `AggregateProvider`: `inputs`.
#[derive(Clone)]
pub struct Aggregate<S: Clone + 'static> {
    pub kind: &'static str,
    pub inputs: S,
}

fn unary_map_codec<H: Clone + Send + Sync + 'static>(kind: &'static str, value: Codec<H>) -> MapCodec<Unary<H>> {
    value.field_of("input").xmap(move |input| Unary { kind, input }, |u: &Unary<H>| u.input.clone())
}

fn binary_map_codec<H: Clone + Send + Sync + 'static>(kind: &'static str, value: Codec<H>) -> MapCodec<Binary<H>> {
    record!(Binary<H> { kind: MapCodec::unit(kind), left: value.field_of("left"), right: value.field_of("right") })
}

fn aggregate_map_codec<S: Clone + Send + Sync + 'static>(kind: &'static str, set: Codec<S>) -> MapCodec<Aggregate<S>> {
    set.field_of("inputs").xmap(move |inputs| Aggregate { kind, inputs }, |a: &Aggregate<S>| a.inputs.clone())
}

fn int_unary(kind: &'static str) -> MapCodec<Arc<dyn ContextIntProvider>> {
    lift(unary_map_codec(kind, int_holder_codec()), |u| Arc::new(u) as IntProviderRef, |t| t.as_any().downcast_ref::<Unary<IntHolder>>())
}
fn int_binary(kind: &'static str) -> MapCodec<Arc<dyn ContextIntProvider>> {
    lift(binary_map_codec(kind, int_holder_codec()), |u| Arc::new(u) as IntProviderRef, |t| t.as_any().downcast_ref::<Binary<IntHolder>>())
}
fn int_aggregate(kind: &'static str) -> MapCodec<Arc<dyn ContextIntProvider>> {
    lift(aggregate_map_codec(kind, int_list_codec()), |u| Arc::new(u) as IntProviderRef, |t| t.as_any().downcast_ref::<Aggregate<IntHolderSet>>())
}
fn float_unary(kind: &'static str) -> MapCodec<Arc<dyn ContextFloatProvider>> {
    lift(unary_map_codec(kind, float_holder_codec()), |u| Arc::new(u) as FloatProviderRef, |t| t.as_any().downcast_ref::<Unary<FloatHolder>>())
}
fn float_binary(kind: &'static str) -> MapCodec<Arc<dyn ContextFloatProvider>> {
    lift(binary_map_codec(kind, float_holder_codec()), |u| Arc::new(u) as FloatProviderRef, |t| t.as_any().downcast_ref::<Binary<FloatHolder>>())
}
fn float_aggregate(kind: &'static str) -> MapCodec<Arc<dyn ContextFloatProvider>> {
    lift(aggregate_map_codec(kind, float_list_codec()), |u| Arc::new(u) as FloatProviderRef, |t| t.as_any().downcast_ref::<Aggregate<FloatHolderSet>>())
}

impl Validatable for Unary<IntHolder> {
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "input", &self.input);
    }
}

// Unary / Binary / Aggregate evaluate by `kind`.

fn int_in(h: &IntHolder, context: &mut LootContext) -> IntResult {
    h.value(context.get_resolver()).get_int_unsafe(context)
}

fn float_in(h: &FloatHolder, context: &mut LootContext) -> FloatResult {
    h.value(context.get_resolver()).get_float_unsafe(context)
}

impl ContextIntProvider for Unary<IntHolder> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let v = int_in(&self.input, context)?;
        match self.kind {
            "abs" => v.checked_abs().ok_or(ArithmeticException),
            "negate" => v.checked_neg().ok_or(ArithmeticException),
            _ => unreachable!(),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "input", &self.input);
    }
}

impl ContextIntProvider for Binary<IntHolder> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let l = int_in(&self.left, context)?;
        let r = int_in(&self.right, context)?;
        match self.kind {
            "sub" => l.checked_sub(r).ok_or(ArithmeticException),
            "floor_mod" => {
                if r == 0 {
                    Err(ArithmeticException)
                } else {
                    Ok(floor_mod_i32(l, r))
                }
            }
            "floor_div" => {
                // `Math.floorDivExact`
                if r == 0 || (l == i32::MIN && r == -1) {
                    Err(ArithmeticException)
                } else {
                    Ok(mc_core::math::java::floor_div_i32(l, r))
                }
            }
            "mod" => {
                if r == 0 {
                    Err(ArithmeticException)
                } else {
                    Ok(l.wrapping_rem(r))
                }
            }
            "div" => {
                if r == 0 {
                    Err(ArithmeticException)
                } else {
                    Ok(l.wrapping_div(r))
                }
            }
            "pow" => {
                if l == 0 && r == 0 {
                    return Err(ArithmeticException);
                }
                // `Math.powExact`
                u32::try_from(r).ok().and_then(|n| l.checked_pow(n)).ok_or(ArithmeticException)
            }
            _ => unreachable!(),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        match self.kind {
            "pow" => {
                validate_holder_field(context, "base", &self.left);
                validate_holder_field(context, "exponent", &self.right);
            }
            _ => {
                validate_holder_field(context, "left", &self.left);
                validate_holder_field(context, "right", &self.right);
            }
        }
    }
}

impl ContextIntProvider for Aggregate<IntHolderSet> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let inputs = self.inputs.holders(context.get_resolver());
        match self.kind {
            "avg" => {
                let (mut sum, mut count) = (0i64, 0i64);
                for h in &inputs {
                    sum = sum.wrapping_add(i64::from(int_in(h, context)?));
                    count += 1;
                }
                if count == 0 {
                    return Err(ArithmeticException);
                }
                long_to_int_safe(sum.wrapping_div(count))
            }
            "max" => {
                let mut v = i32::MIN;
                for h in &inputs {
                    v = v.max(int_in(h, context)?);
                }
                Ok(v)
            }
            "min" => {
                let mut v = i32::MAX;
                for h in &inputs {
                    v = v.min(int_in(h, context)?);
                }
                Ok(v)
            }
            "mul" => {
                let mut v = 1i64;
                for h in &inputs {
                    v = v.wrapping_mul(i64::from(int_in(h, context)?));
                }
                long_to_int_safe(v)
            }
            "add" => {
                let mut v = 0i64;
                for h in &inputs {
                    v = v.wrapping_add(i64::from(int_in(h, context)?));
                }
                long_to_int_safe(v)
            }
            _ => unreachable!(),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_set_min(context, "inputs", &self.inputs, 1);
    }
}

impl ContextFloatProvider for Unary<FloatHolder> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        let v = float_in(&self.input, context)?;
        Ok(match self.kind {
            "abs" => v.abs(),
            "ceil" => ((v as f64).ceil() as i32) as f32,
            "cos" => mc_core::math::mth::cos(v),
            "floor" => (v as f64).floor() as f32,
            "negate" => -v,
            "round" => round_f32(v) as f32,
            "sin" => mc_core::math::mth::sin(v),
            "sqrt" => mc_core::math::mth::sqrt(v),
            "truncate" => {
                if v > 0.0 {
                    (v as f64).floor() as f32
                } else {
                    (v as f64).ceil() as f32
                }
            }
            _ => unreachable!(),
        })
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "input", &self.input);
    }
}

impl ContextFloatProvider for Binary<FloatHolder> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        match self.kind {
            "sub" => Ok(float_in(&self.left, context)? - float_in(&self.right, context)?),
            "mod" => {
                let right = float_in(&self.right, context)?;
                if right == 0.0 {
                    Ok(f32::NAN)
                } else {
                    Ok(float_in(&self.left, context)? % right)
                }
            }
            "div" => Ok(float_in(&self.left, context)? / float_in(&self.right, context)?),
            "pow" => {
                let base = float_in(&self.left, context)?;
                let exponent = float_in(&self.right, context)?;
                if base == 0.0 && exponent == 0.0 {
                    Ok(f32::NAN)
                } else {
                    Ok(java_pow(f64::from(base), f64::from(exponent)) as f32)
                }
            }
            _ => unreachable!(),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        match self.kind {
            "pow" => {
                validate_holder_field(context, "base", &self.left);
                validate_holder_field(context, "exponent", &self.right);
            }
            _ => {
                validate_holder_field(context, "left", &self.left);
                validate_holder_field(context, "right", &self.right);
            }
        }
    }
}

/// `Math.pow` special cases that differ from Rust's `powf` (`pow(±1, ±∞)` is NaN in Java).
fn java_pow(a: f64, b: f64) -> f64 {
    if b.is_infinite() && a.abs() == 1.0 { f64::NAN } else { a.powf(b) }
}

impl ContextFloatProvider for Aggregate<FloatHolderSet> {
    fn type_name(&self) -> &'static str {
        self.kind
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        let inputs = self.inputs.holders(context.get_resolver());
        match self.kind {
            "avg" => {
                let (mut sum, mut count) = (0.0f32, 0i32);
                for h in &inputs {
                    sum += float_in(h, context)?;
                    count += 1;
                }
                Ok(sum / count as f32)
            }
            "length" => {
                let mut sum = 0.0f32;
                for h in &inputs {
                    let v = float_in(h, context)?;
                    sum += v * v;
                }
                Ok(mc_core::math::mth::sqrt(sum))
            }
            "max" => {
                let mut v = -f32::MAX;
                for h in &inputs {
                    v = java_max_f32(v, float_in(h, context)?);
                }
                Ok(v)
            }
            "min" => {
                let mut v = f32::MAX;
                for h in &inputs {
                    v = java_min_f32(v, float_in(h, context)?);
                }
                Ok(v)
            }
            "mul" => {
                let mut v = 1.0f32;
                for h in &inputs {
                    v *= float_in(h, context)?;
                }
                Ok(v)
            }
            "add" => {
                let mut v = 0.0f32;
                for h in &inputs {
                    v += float_in(h, context)?;
                }
                Ok(v)
            }
            _ => unreachable!(),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_set_min(context, "inputs", &self.inputs, 1);
    }
}

/// `Math.max(float, float)` (NaN wins, `0.0 > -0.0`).
fn java_max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { b } else { a }
    } else if a >= b {
        a
    } else {
        b
    }
}

/// `Math.min(float, float)`.
fn java_min_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { a } else { b }
    } else if a <= b {
        a
    } else {
        b
    }
}

// ---- conditional / dispatcher ---------------------------------------------------------------------------------------

/// `DispatcherProvider.Case`.
#[derive(Clone)]
pub struct Case<H: Clone + 'static> {
    pub condition: ConditionHolder,
    pub value: H,
}

fn case_codec<H: Clone + Send + Sync + 'static>(value: Codec<H>) -> Codec<Case<H>> {
    record!(Case<H> { condition: condition_holder_codec().field_of("condition"), value: value.field_of("value") }).codec()
}

/// `NumberDispatcher` (ints and floats).
#[derive(Clone)]
pub struct NumberDispatcher<H: Clone + 'static> {
    pub cases: Vec<Case<H>>,
    pub default_value: H,
}

fn dispatcher_codec<H: Clone + Send + Sync + 'static>(value: Codec<H>, default_case: H) -> MapCodec<NumberDispatcher<H>> {
    record!(NumberDispatcher<H> {
        cases: case_codec(value.clone()).list_of().field_of("cases"),
        default_value: value.optional_field_of_default_with("default", default_case),
    })
}

/// `ConditionalValue` (ints and floats).
#[derive(Clone)]
pub struct Conditional<H: Clone + 'static> {
    pub condition: ConditionHolder,
    pub on_true: H,
    pub on_false: H,
}

fn conditional_codec<H: Clone + Send + Sync + 'static>(value: Codec<H>, default_if_false: H) -> MapCodec<Conditional<H>> {
    record!(Conditional<H> {
        condition: condition_holder_codec().field_of("condition"),
        on_true: value.field_of("on_true"),
        on_false: value.optional_field_of_default_with("on_false", default_if_false),
    })
}

/// A `Holder` default of `optionalFieldOf(name, default)` (holders have no `PartialEq`: the default is never "equal").
trait OptionalDefaultWith<H> {
    fn optional_field_of_default_with(&self, name: &str, default: H) -> MapCodec<H>;
}

impl<H: Clone + Send + Sync + 'static> OptionalDefaultWith<H> for Codec<H> {
    fn optional_field_of_default_with(&self, name: &str, default: H) -> MapCodec<H> {
        self.optional_field_of(name).xmap(move |o| o.unwrap_or_else(|| default.clone()), |h: &H| Some(h.clone()))
    }
}

impl ContextIntProvider for NumberDispatcher<IntHolder> {
    fn type_name(&self) -> &'static str {
        "number_dispatcher"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        for case in &self.cases {
            if case.condition.value(context.get_resolver()).test(context) {
                return int_in(&case.value, context);
            }
        }
        int_in(&self.default_value, context)
    }
    fn validate(&self, context: &ValidationContext) {
        for (i, case) in self.cases.iter().enumerate() {
            let c = context.for_indexed_field("cases", i);
            validate_holder_field(&c, "condition", &case.condition);
            validate_holder_field(&c, "value", &case.value);
        }
        validate_holder_field(context, "default", &self.default_value);
    }
}

impl ContextFloatProvider for NumberDispatcher<FloatHolder> {
    fn type_name(&self) -> &'static str {
        "number_dispatcher"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        for case in &self.cases {
            if case.condition.value(context.get_resolver()).test(context) {
                return float_in(&case.value, context);
            }
        }
        float_in(&self.default_value, context)
    }
    fn validate(&self, context: &ValidationContext) {
        for (i, case) in self.cases.iter().enumerate() {
            let c = context.for_indexed_field("cases", i);
            validate_holder_field(&c, "condition", &case.condition);
            validate_holder_field(&c, "value", &case.value);
        }
        validate_holder_field(context, "default", &self.default_value);
    }
}

impl ContextIntProvider for Conditional<IntHolder> {
    fn type_name(&self) -> &'static str {
        "conditional"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let on = self.condition.value(context.get_resolver()).test(context);
        int_in(if on { &self.on_true } else { &self.on_false }, context)
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "condition", &self.condition);
        validate_holder_field(context, "on_true", &self.on_true);
        validate_holder_field(context, "on_false", &self.on_false);
    }
}

impl ContextFloatProvider for Conditional<FloatHolder> {
    fn type_name(&self) -> &'static str {
        "conditional"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        let on = self.condition.value(context.get_resolver()).test(context);
        float_in(if on { &self.on_true } else { &self.on_false }, context)
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "condition", &self.condition);
        validate_holder_field(context, "on_true", &self.on_true);
        validate_holder_field(context, "on_false", &self.on_false);
    }
}

// ---- weighted list ----------------------------------------------------------------------------------------------------

/// `net.minecraft.util.random.WeightedList` (flat/compact selectors select identically).
#[derive(Clone)]
pub struct WeightedList<E: Clone> {
    items: Vec<(E, i32)>,
    total_weight: i32,
}

impl<E: Clone> WeightedList<E> {
    pub fn of(items: Vec<(E, i32)>) -> WeightedList<E> {
        let total_weight = items.iter().fold(0i32, |a, (_, w)| a.wrapping_add(*w));
        WeightedList { items, total_weight }
    }
    pub fn is_empty(&self) -> bool {
        self.total_weight == 0
    }
    pub fn unwrap(&self) -> &[(E, i32)] {
        &self.items
    }
    /// `getRandomOrThrow(random)`.
    pub fn get_random_or_throw(&self, random: &mut dyn RandomSource) -> &E {
        assert!(self.total_weight != 0, "Weighted list has no elements");
        let mut selection = random.next_int_bounded(self.total_weight);
        for (value, weight) in &self.items {
            selection -= weight;
            if selection < 0 {
                return value;
            }
        }
        panic!("{selection} exceeded total weight");
    }
    /// `WeightedList.nonEmptyCodec(Codec.<E>)`.
    pub fn non_empty_codec(element: &Codec<E>) -> Codec<WeightedList<E>>
    where
        E: Send + Sync + 'static,
    {
        let entry = record!(WeightedEntry<E> { data: element.field_of("data"), weight: NON_NEGATIVE_INT.field_of("weight") }).codec();
        entry
            .list_of()
            .xmap(|l| WeightedList::of(l.into_iter().map(|e| (e.data, e.weight)).collect()), |w: &WeightedList<E>| {
                w.items.iter().map(|(d, wt)| WeightedEntry { data: d.clone(), weight: *wt }).collect()
            })
            .validate(|list| {
                if list.is_empty() {
                    DataResult::error_str("Weighted list must contain at least one entry with non-zero weight")
                } else {
                    DataResult::success(list)
                }
            })
    }
}

struct WeightedEntry<E> {
    data: E,
    weight: i32,
}

/// `WeightedListValue` (ints and floats).
#[derive(Clone)]
pub struct WeightedListValue<H: Clone + 'static> {
    pub distribution: WeightedList<H>,
}

fn weighted_codec<H: Clone + Send + Sync + 'static>(value: Codec<H>) -> MapCodec<WeightedListValue<H>> {
    record!(WeightedListValue<H> { distribution: WeightedList::non_empty_codec(&value).field_of("distribution") })
}

impl ContextIntProvider for WeightedListValue<IntHolder> {
    fn type_name(&self) -> &'static str {
        "weighted_list"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let chosen = self.distribution.get_random_or_throw(context.get_random()).clone();
        int_in(&chosen, context)
    }
    fn validate(&self, context: &ValidationContext) {
        let list: Vec<IntHolder> = self.distribution.unwrap().iter().map(|(h, _)| h.clone()).collect();
        validate_holder_list(context, "distribution", &list);
    }
}

impl ContextFloatProvider for WeightedListValue<FloatHolder> {
    fn type_name(&self) -> &'static str {
        "weighted_list"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        let chosen = self.distribution.get_random_or_throw(context.get_random()).clone();
        float_in(&chosen, context)
    }
    fn validate(&self, context: &ValidationContext) {
        let list: Vec<FloatHolder> = self.distribution.unwrap().iter().map(|(h, _)| h.clone()).collect();
        validate_holder_list(context, "distribution", &list);
    }
}

// ---- uniform / binomial / conversions ---------------------------------------------------------------------------------

/// ints `UniformGenerator`.
#[derive(Clone)]
pub struct IntUniform {
    pub min: IntHolder,
    pub max: IntHolder,
}

impl ContextIntProvider for IntUniform {
    fn type_name(&self) -> &'static str {
        "uniform"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let min = int_in(&self.min, context)?;
        let max = int_in(&self.max, context)?;
        Ok(mc_core::math::mth::next_int(context.get_random(), min, max))
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "min", &self.min);
        validate_holder_field(context, "max", &self.max);
    }
}

/// floats `UniformGenerator`.
#[derive(Clone)]
pub struct FloatUniform {
    pub min: FloatHolder,
    pub max: FloatHolder,
}

impl ContextFloatProvider for FloatUniform {
    fn type_name(&self) -> &'static str {
        "uniform"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        let min = float_in(&self.min, context)?;
        let max = float_in(&self.max, context)?;
        Ok(mc_core::math::mth::next_float(context.get_random(), min, max))
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "min", &self.min);
        validate_holder_field(context, "max", &self.max);
    }
}

/// `BinomialDistributionGenerator`.
#[derive(Clone)]
pub struct Binomial {
    pub n: IntHolder,
    pub p: FloatHolder,
}

impl ContextIntProvider for Binomial {
    fn type_name(&self) -> &'static str {
        "binomial"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let n = int_in(&self.n, context)?;
        let p = self.p.value(context.get_resolver()).get_float_or_throw(context)?;
        let random = context.get_random();
        let mut result = 0;
        for _ in 0..n {
            if random.next_float() < p {
                result += 1;
            }
        }
        Ok(result)
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "n", &self.n);
        validate_holder_field(context, "p", &self.p);
    }
}

/// `FromFloat` (int from float).
#[derive(Clone)]
pub struct FromFloat {
    pub input: FloatHolder,
}

impl ContextIntProvider for FromFloat {
    fn type_name(&self) -> &'static str {
        "from_float"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        float_to_int_safe(float_in(&self.input, context)?)
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "input", &self.input);
    }
}

/// `FromInt` (float from int).
#[derive(Clone)]
pub struct FromInt {
    pub input: IntHolder,
}

impl ContextFloatProvider for FromInt {
    fn type_name(&self) -> &'static str {
        "from_int"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        Ok(int_in(&self.input, context)? as f32)
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "input", &self.input);
    }
}

// ---- scoreboard / storage / enchantment level / environment attribute -------------------------------------------------

/// `ScoreboardValue`.
#[derive(Clone)]
pub struct ScoreboardValue {
    pub target: Arc<dyn ScoreboardNameProvider>,
    pub score: String,
    pub fallback: IntHolder,
}

impl ContextIntProvider for ScoreboardValue {
    fn type_name(&self) -> &'static str {
        "score"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        self.target.referenced_params()
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        let score = self.target.get_score_holder(context).and_then(|name| server_access::score(context.get_level(), &name, &self.score));
        match score {
            Some(v) => Ok(v),
            None => int_in(&self.fallback, context),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        self.target.validate(&context.for_field("target"));
        validate_holder_field(context, "fallback", &self.fallback);
    }
}

/// `StoredNumberAccess`.
#[derive(Clone)]
pub struct StoredNumberAccess {
    pub storage: mc_core::ident::Identifier,
    pub path: Arc<crate::commands::arguments::nbt_path_argument::NbtPath>,
}

impl StoredNumberAccess {
    pub fn map_codec() -> MapCodec<StoredNumberAccess> {
        record!(StoredNumberAccess {
            storage: mc_dfu::extra::IDENTIFIER.field_of("storage"),
            path: super::nbt::nbt_path_codec().field_of("path"),
        })
    }

    /// `getNumericTag(context)`.
    pub fn get_numeric_tag(&self, context: &LootContext) -> Option<Number> {
        let value = server_access::command_storage(context.get_level(), &self.storage);
        let selected = self.path.get(&Tag::Compound(value)).ok()?;
        if selected.len() == 1 { selected[0].as_number() } else { None }
    }
}

/// ints `StorageValue`.
#[derive(Clone)]
pub struct IntStorageValue {
    pub access: StoredNumberAccess,
    pub fallback: IntHolder,
}

impl ContextIntProvider for IntStorageValue {
    fn type_name(&self) -> &'static str {
        "storage"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        match self.access.get_numeric_tag(context) {
            Some(n) => Ok(n.boxed_int_value()),
            None => int_in(&self.fallback, context),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "fallback", &self.fallback);
    }
}

/// floats `StorageValue`.
#[derive(Clone)]
pub struct FloatStorageValue {
    pub access: StoredNumberAccess,
    pub fallback: FloatHolder,
}

impl ContextFloatProvider for FloatStorageValue {
    fn type_name(&self) -> &'static str {
        "storage"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        match self.access.get_numeric_tag(context) {
            Some(n) => Ok(n.float_value()),
            None => float_in(&self.fallback, context),
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "fallback", &self.fallback);
    }
}

/// `EnchantmentLevelProvider`.
#[derive(Clone)]
pub struct EnchantmentLevelProvider {
    pub amount: crate::world::item::enchantment::LevelBasedValue,
}

impl ContextFloatProvider for EnchantmentLevelProvider {
    fn type_name(&self) -> &'static str {
        "enchantment_level"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::EnchantmentLevel]
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        Ok(self.amount.calculate(context.params().enchantment_level.unwrap_or(0)))
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

/// `EnvironmentAttributeValue` (ints and floats): the attribute id; evaluated by the level's attribute system.
#[derive(Clone)]
pub struct EnvironmentAttributeValue {
    pub attribute: mc_core::ident::Identifier,
    pub as_float: bool,
}

impl EnvironmentAttributeValue {
    fn is_positional(&self) -> bool {
        server_access::environment_attribute_is_positional(&self.attribute)
    }
}

impl ContextIntProvider for EnvironmentAttributeValue {
    fn type_name(&self) -> &'static str {
        "environment_attribute"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        if self.is_positional() { vec![ContextKey::Origin] } else { vec![] }
    }
    fn get_int_unsafe(&self, context: &mut LootContext) -> IntResult {
        server_access::environment_attribute_int(context, &self.attribute).ok_or(ArithmeticException)
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

impl ContextFloatProvider for EnvironmentAttributeValue {
    fn type_name(&self) -> &'static str {
        "environment_attribute"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        if self.is_positional() { vec![ContextKey::Origin] } else { vec![] }
    }
    fn get_float_unsafe(&self, context: &mut LootContext) -> FloatResult {
        server_access::environment_attribute_float(context, &self.attribute).ok_or(ArithmeticException)
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

// ---- type tables ------------------------------------------------------------------------------------------------------

fn int_attribute_codec() -> MapCodec<Arc<dyn ContextIntProvider>> {
    server_access::environment_attribute_codec(false)
        .xmap(|attribute| Arc::new(EnvironmentAttributeValue { attribute, as_float: false }) as IntProviderRef, |p: &IntProviderRef| {
            p.as_any().downcast_ref::<EnvironmentAttributeValue>().expect("attribute provider").attribute.clone()
        })
}

fn float_attribute_codec() -> MapCodec<Arc<dyn ContextFloatProvider>> {
    server_access::environment_attribute_codec(true)
        .xmap(|attribute| Arc::new(EnvironmentAttributeValue { attribute, as_float: true }) as FloatProviderRef, |p: &FloatProviderRef| {
            p.as_any().downcast_ref::<EnvironmentAttributeValue>().expect("attribute provider").attribute.clone()
        })
}

impl ProviderTypes for dyn ContextIntProvider {
    fn table() -> Arc<TypeTable<Self>> {
        let h = int_holder_codec;
        TypeTable::new(
            "minecraft:context_int_provider_type",
            vec![
                ("abs", int_unary("abs")),
                ("avg", int_aggregate("avg")),
                (
                    "binomial",
                    lift(
                        record!(Binomial { n: h().field_of("n"), p: float_holder_codec().field_of("p") }),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<Binomial>(),
                    ),
                ),
                (
                    "conditional",
                    lift(
                        conditional_codec(h(), int_exactly(0)),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<Conditional<IntHolder>>(),
                    ),
                ),
                (
                    "constant",
                    lift(IntConstant::map_codec(), |v| Arc::new(v) as IntProviderRef, |t| t.as_any().downcast_ref::<IntConstant>()),
                ),
                ("sub", int_binary("sub")),
                ("environment_attribute", int_attribute_codec()),
                (
                    "from_float",
                    lift(
                        float_holder_codec().field_of("input").xmap(|input| FromFloat { input }, |f: &FromFloat| f.input.clone()),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<FromFloat>(),
                    ),
                ),
                ("max", int_aggregate("max")),
                ("min", int_aggregate("min")),
                ("floor_mod", int_binary("floor_mod")),
                ("floor_div", int_binary("floor_div")),
                ("mod", int_binary("mod")),
                ("div", int_binary("div")),
                ("negate", int_unary("negate")),
                (
                    "number_dispatcher",
                    lift(
                        dispatcher_codec(h(), int_exactly(0)),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<NumberDispatcher<IntHolder>>(),
                    ),
                ),
                ("pow", {
                    let c = record!(Binary<IntHolder> { kind: MapCodec::unit("pow"), left: h().field_of("base"), right: h().field_of("exponent") });
                    lift(c, |v| Arc::new(v) as IntProviderRef, |t| t.as_any().downcast_ref::<Binary<IntHolder>>())
                }),
                ("mul", int_aggregate("mul")),
                (
                    "score",
                    lift(
                        record!(ScoreboardValue {
                            target: scoreboard_name_provider_codec().field_of("target"),
                            score: mc_dfu::STRING.field_of("score"),
                            fallback: h().optional_field_of_default_with("fallback", int_exactly(0)),
                        }),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<ScoreboardValue>(),
                    ),
                ),
                (
                    "storage",
                    lift(
                        record!(IntStorageValue {
                            access: StoredNumberAccess::map_codec(),
                            fallback: h().optional_field_of_default_with("fallback", int_exactly(0)),
                        }),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<IntStorageValue>(),
                    ),
                ),
                ("add", int_aggregate("add")),
                (
                    "uniform",
                    lift(
                        record!(IntUniform { min: h().field_of("min"), max: h().field_of("max") }),
                        |v| Arc::new(v) as IntProviderRef,
                        |t| t.as_any().downcast_ref::<IntUniform>(),
                    ),
                ),
                (
                    "weighted_list",
                    lift(weighted_codec(h()), |v| Arc::new(v) as IntProviderRef, |t| t.as_any().downcast_ref::<WeightedListValue<IntHolder>>()),
                ),
            ],
        )
    }
}

impl ProviderTypes for dyn ContextFloatProvider {
    fn table() -> Arc<TypeTable<Self>> {
        let h = float_holder_codec;
        TypeTable::new(
            "minecraft:context_float_provider_type",
            vec![
                ("abs", float_unary("abs")),
                ("avg", float_aggregate("avg")),
                ("ceil", float_unary("ceil")),
                (
                    "conditional",
                    lift(
                        conditional_codec(h(), float_exactly(0.0)),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<Conditional<FloatHolder>>(),
                    ),
                ),
                (
                    "constant",
                    lift(FloatConstant::map_codec(), |v| Arc::new(v) as FloatProviderRef, |t| t.as_any().downcast_ref::<FloatConstant>()),
                ),
                ("cos", float_unary("cos")),
                ("sub", float_binary("sub")),
                (
                    "enchantment_level",
                    lift(
                        crate::world::item::enchantment::LevelBasedValue::codec()
                            .field_of("amount")
                            .xmap(|amount| EnchantmentLevelProvider { amount }, |p: &EnchantmentLevelProvider| p.amount.clone()),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<EnchantmentLevelProvider>(),
                    ),
                ),
                ("environment_attribute", float_attribute_codec()),
                ("floor", float_unary("floor")),
                (
                    "from_int",
                    lift(
                        int_holder_codec().field_of("input").xmap(|input| FromInt { input }, |f: &FromInt| f.input.clone()),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<FromInt>(),
                    ),
                ),
                ("length", float_aggregate("length")),
                ("max", float_aggregate("max")),
                ("min", float_aggregate("min")),
                ("mod", float_binary("mod")),
                ("negate", float_unary("negate")),
                (
                    "number_dispatcher",
                    lift(
                        dispatcher_codec(h(), float_exactly(0.0)),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<NumberDispatcher<FloatHolder>>(),
                    ),
                ),
                ("pow", {
                    let c = record!(Binary<FloatHolder> { kind: MapCodec::unit("pow"), left: h().field_of("base"), right: h().field_of("exponent") });
                    lift(c, |v| Arc::new(v) as FloatProviderRef, |t| t.as_any().downcast_ref::<Binary<FloatHolder>>())
                }),
                ("mul", float_aggregate("mul")),
                ("div", float_binary("div")),
                ("round", float_unary("round")),
                ("sin", float_unary("sin")),
                ("sqrt", float_unary("sqrt")),
                (
                    "storage",
                    lift(
                        record!(FloatStorageValue {
                            access: StoredNumberAccess::map_codec(),
                            fallback: h().optional_field_of_default_with("fallback", float_exactly(0.0)),
                        }),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<FloatStorageValue>(),
                    ),
                ),
                ("add", float_aggregate("add")),
                ("truncate", float_unary("truncate")),
                (
                    "uniform",
                    lift(
                        record!(FloatUniform { min: h().field_of("min"), max: h().field_of("max") }),
                        |v| Arc::new(v) as FloatProviderRef,
                        |t| t.as_any().downcast_ref::<FloatUniform>(),
                    ),
                ),
                (
                    "weighted_list",
                    lift(weighted_codec(h()), |v| Arc::new(v) as FloatProviderRef, |t| t.as_any().downcast_ref::<WeightedListValue<FloatHolder>>()),
                ),
            ],
        )
    }
}

// ---- ResolvableInt / ResolvableFloat (reference to the registries by key) ---------------------------------------------

/// `ResolvableInt`.
#[derive(Clone, PartialEq, Debug)]
pub enum ResolvableInt {
    Constant(i32),
    Reference(mc_core::ident::Identifier),
}

impl ResolvableInt {
    /// `CODEC`: an int, or a `context_int_provider` key.
    pub fn codec() -> Codec<ResolvableInt> {
        INT.either(&mc_dfu::extra::IDENTIFIER).xmap(
            |e| match e {
                Either::Left(v) => ResolvableInt::Constant(v),
                Either::Right(k) => ResolvableInt::Reference(k),
            },
            |r: &ResolvableInt| match r {
                ResolvableInt::Constant(v) => Either::Left(*v),
                ResolvableInt::Reference(k) => Either::Right(k.clone()),
            },
        )
    }

    /// `get(context, defaultValue)`.
    pub fn get(&self, context: &mut LootContext, default_value: i32) -> i32 {
        match self {
            ResolvableInt::Constant(v) => *v,
            ResolvableInt::Reference(key) => match context.get_resolver().get::<IntProviderRef>(keys::CONTEXT_INT_PROVIDER, key).cloned() {
                Some(p) => p.get_int(context),
                None => default_value,
            },
        }
    }
}

/// `ResolvableFloat`.
#[derive(Clone, PartialEq, Debug)]
pub enum ResolvableFloat {
    Constant(f32),
    Reference(mc_core::ident::Identifier),
}

impl ResolvableFloat {
    pub fn codec() -> Codec<ResolvableFloat> {
        FLOAT.either(&mc_dfu::extra::IDENTIFIER).xmap(
            |e| match e {
                Either::Left(v) => ResolvableFloat::Constant(v),
                Either::Right(k) => ResolvableFloat::Reference(k),
            },
            |r: &ResolvableFloat| match r {
                ResolvableFloat::Constant(v) => Either::Left(*v),
                ResolvableFloat::Reference(k) => Either::Right(k.clone()),
            },
        )
    }

    pub fn get(&self, context: &mut LootContext, default_value: f32) -> f32 {
        match self {
            ResolvableFloat::Constant(v) => *v,
            ResolvableFloat::Reference(key) => match context.get_resolver().get::<FloatProviderRef>(keys::CONTEXT_FLOAT_PROVIDER, key).cloned() {
                Some(p) => p.get_float(context),
                None => default_value,
            },
        }
    }
}
