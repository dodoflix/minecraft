//! `DataFix`: one registered fix. Java subclasses implement `makeRule()`; here the rule builder is a closure that gets
//! the `DataFix` back (for `input_schema()`, `output_schema()` and the `fix_type_everywhere*` helpers).
//!
//! ```ignore
//! DataFix::new(schema, /*changes_type*/ false, |fix| {
//!     let ty = fix.input_schema().get_type(&References::ITEM_STACK);
//!     fix.fix_type_everywhere_typed("MyFix", &ty, |typed| typed)
//! })
//! ```

use crate::rewrite::{BitSet, OptimizationRule, RewriteResult, TypeRewriteRule, View};
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::{Dynamic, Ops};
use std::sync::{Arc, OnceLock};

struct DataFixInner {
    output_schema: Schema,
    changes_type: bool,
    make_rule: Box<dyn Fn(&DataFix) -> TypeRewriteRule + Send + Sync>,
    rule: OnceLock<TypeRewriteRule>,
}

/// `DataFix`; cheap to clone.
#[derive(Clone)]
pub struct DataFix(Arc<DataFixInner>);

impl DataFix {
    /// `new DataFix(outputSchema, changesType)` with `makeRule()` given by `make_rule` (called lazily, once).
    pub fn new(
        output_schema: &Schema,
        changes_type: bool,
        make_rule: impl Fn(&DataFix) -> TypeRewriteRule + Send + Sync + 'static,
    ) -> DataFix {
        DataFix(Arc::new(DataFixInner {
            output_schema: output_schema.clone(),
            changes_type,
            make_rule: Box::new(make_rule),
            rule: OnceLock::new(),
        }))
    }

    /// `getVersionKey()`.
    pub fn version_key(&self) -> i32 {
        self.0.output_schema.version_key()
    }

    /// `getRule()`.
    pub fn get_rule(&self) -> TypeRewriteRule {
        self.0.rule.get_or_init(|| (self.0.make_rule)(self)).clone()
    }

    /// `getInputSchema()`.
    pub fn input_schema(&self) -> Schema {
        if self.0.changes_type {
            self.0.output_schema.parent().expect("a type-changing fix needs a parent schema").clone()
        } else {
            self.0.output_schema.clone()
        }
    }

    /// `getOutputSchema()`.
    pub fn output_schema(&self) -> Schema {
        self.0.output_schema.clone()
    }

    // ---- rule helpers (`fixTypeEverywhere*`) --------------------------------------------------------------------

    /// `fixTypeEverywhere(name, type, function)`: `function(ops, value)` on every value of `ty`.
    pub fn fix_type_everywhere(
        &self,
        name: &str,
        ty: &Type,
        f: impl Fn(&Ops, Value) -> Value + Send + Sync + 'static,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_new(name, ty, ty, f)
    }

    /// `fixTypeEverywhere(name, type, newType, function)`.
    pub fn fix_type_everywhere_new(
        &self,
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(&Ops, Value) -> Value + Send + Sync + 'static,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_bits(name, ty, new_type, f, BitSet::new())
    }

    /// `fixTypeEverywhere(name, type, newType, function, bitSet)`.
    pub fn fix_type_everywhere_bits(
        &self,
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(&Ops, Value) -> Value + Send + Sync + 'static,
        bits: BitSet,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_view(ty, DataFix::unchecked(name, ty, new_type, f, bits))
    }

    /// `convertUnchecked(name, type, newType)`.
    pub fn convert_unchecked(&self, name: &str, ty: &Type, new_type: &Type) -> TypeRewriteRule {
        self.fix_type_everywhere_new(name, ty, new_type, |_, v| v)
    }

    /// `writeAndRead(name, type, newType)`.
    pub fn write_and_read(&self, name: &str, ty: &Type, new_type: &Type) -> TypeRewriteRule {
        self.write_fix_and_read(name, ty, new_type, |d| d)
    }

    /// `writeFixAndRead(name, type, newType, fix)`: encode with the (patched) old type, fix the `Dynamic`, decode
    /// with the new type.
    pub fn write_fix_and_read(
        &self,
        name: &str,
        ty: &Type,
        new_type: &Type,
        fix: impl Fn(Dynamic) -> Dynamic + Send + Sync + 'static,
    ) -> TypeRewriteRule {
        let patched: Arc<OnceLock<Type>> = Arc::new(OnceLock::new());
        let (patched2, name2, new_type2) = (patched.clone(), name.to_string(), new_type.clone());
        let view = DataFix::unchecked(
            name,
            ty,
            new_type,
            move |ops, input| {
                let patched_type = patched2.get().expect("patched type not set yet");
                let Some(written) = log_partial(patched_type.write_dynamic(ops, &input)) else {
                    panic!("Could not write the object in {name2}");
                };
                let fixed = fix(written);
                let Some((typed, _)) = log_partial(new_type2.read_typed(&fixed)) else {
                    panic!("Could not read the new object in {name2}");
                };
                typed.value
            },
            BitSet::new(),
        );
        let rule = self.fix_type_everywhere_view(ty, view);
        let _ = patched.set(ty.all(&rule, true, false).view.new_type().clone());
        rule
    }

    /// `fixTypeEverywhereTyped(name, type, function)`.
    pub fn fix_type_everywhere_typed(
        &self,
        name: &str,
        ty: &Type,
        f: impl Fn(Typed) -> Typed + Send + Sync + 'static,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_typed_new(name, ty, ty, f)
    }

    /// `fixTypeEverywhereTyped(name, type, newType, function)`.
    pub fn fix_type_everywhere_typed_new(
        &self,
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(Typed) -> Typed + Send + Sync + 'static,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_view(ty, DataFix::checked(name, ty, new_type, f, BitSet::new()))
    }

    /// `fixTypeEverywhereTyped(name, type, newType, function, bitSet)`.
    pub fn fix_type_everywhere_typed_bits(
        &self,
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(Typed) -> Typed + Send + Sync + 'static,
        bits: BitSet,
    ) -> TypeRewriteRule {
        self.fix_type_everywhere_view(ty, DataFix::checked(name, ty, new_type, f, bits))
    }

    /// `fixTypeEverywhere(type, view)`.
    pub fn fix_type_everywhere_view(&self, ty: &Type, view: RewriteResult) -> TypeRewriteRule {
        TypeRewriteRule::everywhere(TypeRewriteRule::if_same(ty, view), OptimizationRule::Standard, true, true)
    }

    // ---- views --------------------------------------------------------------------------------------------------

    /// `DataFix.unchecked(..)`: a rewrite result around a plain function.
    pub fn unchecked(
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(&Ops, Value) -> Value + Send + Sync + 'static,
        bits: BitSet,
    ) -> RewriteResult {
        RewriteResult::create(View::create(name, ty, new_type, f), bits)
    }

    /// `DataFix.checked(..)`: like `unchecked` over `Typed`, verifying the result type
    /// (`Dynamic type check failed` otherwise).
    pub fn checked(
        name: &str,
        ty: &Type,
        new_type: &Type,
        f: impl Fn(Typed) -> Typed + Send + Sync + 'static,
        bits: BitSet,
    ) -> RewriteResult {
        let (ty2, new_type2) = (ty.clone(), new_type.clone());
        RewriteResult::create(
            View::create(name, ty, new_type, move |ops, a| {
                let result = f(Typed::new(ty2.clone(), *ops, a));
                if !new_type2.equals_with(&result.ty, true, false) {
                    panic!("Dynamic type check failed: {} not equal to {}", new_type2, result.ty);
                }
                result.value
            }),
            bits,
        )
    }
}

/// `result.resultOrPartial(LOGGER::error)`.
fn log_partial<A>(result: mc_dfu::DataResult<A>) -> Option<A> {
    if let Some(m) = result.error_message() {
        log::error!("{m}");
    }
    result.result_or_partial()
}
