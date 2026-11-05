//! `View`, `RewriteResult`, `TypeRewriteRule` and the point-free function representation behind them.
//!
//! Java keeps rewrite functions as `PointFree` expression trees (so the optimiser can fuse them) and evaluates them with
//! `evalCached().apply(ops)`. The optimiser (`PointFreeRule`) only changes performance, so it is not ported; the tree is
//! kept (`PointFree`) because rule/cache-key equality (`Fun` equality is by *name*, which decides whether two fixes
//! share a cached rewrite) and `isNop` are observable, and evaluated to a closure on first use.

use crate::family::RecursiveTypeFamily;
use crate::optic::TypedOptic;
use crate::ty::Type;
use crate::value::Value;
use mc_dfu::Ops;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

/// `java.util.BitSet` over a recursive family's indices (families have far fewer than 64 members).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct BitSet(pub u64);

impl BitSet {
    pub fn new() -> BitSet {
        BitSet(0)
    }
    pub fn of(index: i32) -> BitSet {
        let mut b = BitSet(0);
        b.set(index);
        b
    }
    pub fn set(&mut self, index: i32) {
        assert!((0..64).contains(&index), "recursive family index out of range: {index}");
        self.0 |= 1 << index;
    }
    pub fn or(&mut self, other: BitSet) {
        self.0 |= other.0;
    }
    pub fn get(&self, index: i32) -> bool {
        (0..64).contains(&index) && self.0 & (1 << index) != 0
    }
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

/// `evalCached().apply(ops)` of a view: `A -> B` on dynamic values.
pub type ViewFn = Arc<dyn Fn(&Ops, Value) -> Value + Send + Sync>;

/// `IntFunction<RewriteResult<?, ?>>`.
pub type IntFn = Arc<dyn Fn(i32) -> RewriteResult + Send + Sync>;

/// `Algebra` / `ListAlgebra`: one rewrite result per family member.
pub type Algebra = Arc<Vec<RewriteResult>>;

pub(crate) struct FoldShared {
    family: Arc<RecursiveTypeFamily>,
    new_family: Arc<RecursiveTypeFamily>,
    algebra: Algebra,
    this: Weak<FoldShared>,
    hmapped: OnceLock<IntFn>,
    per_index: Mutex<HashMap<i32, ViewFn>>,
}

impl FoldShared {
    /// `RecursiveTypeFamily.fold(algebra, newFamily)`.
    pub(crate) fn fold_fn(family: &Arc<RecursiveTypeFamily>, algebra: Algebra, new_family: &Arc<RecursiveTypeFamily>) -> IntFn {
        let shared = Arc::new_cyclic(|this| FoldShared {
            family: family.clone(),
            new_family: new_family.clone(),
            algebra,
            this: this.clone(),
            hmapped: OnceLock::new(),
            per_index: Mutex::new(HashMap::new()),
        });
        shared.as_int_fn()
    }

    fn as_int_fn(self: &Arc<FoldShared>) -> IntFn {
        let shared = self.clone();
        Arc::new(move |index| {
            let result = &shared.algebra[index as usize];
            let a = shared.family.apply(index);
            let b = shared.new_family.apply(index);
            RewriteResult::create(
                View(PointFree::new(PfKind::Fold { shared: shared.clone(), index }, a, b)),
                result.rec_data,
            )
        })
    }

    /// `Fold.eval` for `index`: `algebra[index].function ∘ hmap(template, fold)[index].function`, evaluated once.
    fn func(&self, index: i32) -> ViewFn {
        if let Some(f) = self.per_index.lock().unwrap().get(&index) {
            return f.clone();
        }
        let hmapped = self.hmapped.get_or_init(|| {
            let this = self.this.upgrade().expect("fold dropped while evaluating");
            self.family.template().hmap(&self.family, this.as_int_fn())
        });
        let result = hmapped(index);
        let op = &self.algebra[index as usize];
        let composed = PointFree::comp(&op.view.0, &result.view.0);
        let f = composed.eval();
        self.per_index.lock().unwrap().insert(index, f.clone());
        f
    }
}

pub(crate) enum PfKind {
    /// `Functions.id`.
    Id,
    /// `Functions.in`: identity from the unfolded type to the recursion point.
    In,
    /// `Functions.fun(name, ..)`: a user function (equality by name and types).
    Fun { name: String, f: ViewFn },
    /// `Comp`: `functions[0]` is applied last.
    Comp(Vec<PointFree>),
    /// `Apply(ProfunctorTransformer(optic), f)`: `f` mapped over the optic's foci.
    App { optic: TypedOptic, f: PointFree },
    /// `Fold` (cata over a recursive family).
    Fold { shared: Arc<FoldShared>, index: i32 },
    /// `TaggedChoiceType.RewriteFunc`.
    TaggedChoice { results: Vec<(String, RewriteResult)> },
}

pub(crate) struct PfNode {
    kind: PfKind,
    ty: Type,
    new_ty: Type,
    eval: OnceLock<ViewFn>,
    hash: OnceLock<u64>,
}

/// `PointFree<Function<A, B>>`.
#[derive(Clone)]
pub struct PointFree(Arc<PfNode>);

impl PointFree {
    pub(crate) fn new(kind: PfKind, ty: Type, new_ty: Type) -> PointFree {
        PointFree(Arc::new(PfNode { kind, ty, new_ty, eval: OnceLock::new(), hash: OnceLock::new() }))
    }

    pub fn id(ty: &Type) -> PointFree {
        PointFree::new(PfKind::Id, ty.clone(), ty.clone())
    }

    pub fn is_id(&self) -> bool {
        matches!(self.0.kind, PfKind::Id)
    }

    /// `Functions.comp(f1, f2)` = `f1 ∘ f2` (flattening nested compositions, dropping ids).
    pub fn comp(f1: &PointFree, f2: &PointFree) -> PointFree {
        if f1.is_id() {
            return f2.clone();
        }
        if f2.is_id() {
            return f1.clone();
        }
        let mut functions: Vec<PointFree> = Vec::new();
        for f in [f1, f2] {
            match &f.0.kind {
                PfKind::Comp(v) => functions.extend(v.iter().cloned()),
                _ => functions.push(f.clone()),
            }
        }
        let ty = functions[functions.len() - 1].0.ty.clone();
        let new_ty = functions[0].0.new_ty.clone();
        PointFree::new(PfKind::Comp(functions), ty, new_ty)
    }

    pub fn ty(&self) -> &Type {
        &self.0.ty
    }
    pub fn new_ty(&self) -> &Type {
        &self.0.new_ty
    }

    pub fn eval(&self) -> ViewFn {
        self.0
            .eval
            .get_or_init(|| match &self.0.kind {
                PfKind::Id | PfKind::In => Arc::new(|_: &Ops, v: Value| v),
                PfKind::Fun { f, .. } => f.clone(),
                PfKind::Comp(fs) => {
                    let evals: Vec<ViewFn> = fs.iter().map(PointFree::eval).collect();
                    Arc::new(move |ops: &Ops, mut v: Value| {
                        for f in evals.iter().rev() {
                            v = f(ops, v);
                        }
                        v
                    })
                }
                PfKind::App { optic, f } => {
                    let optic = optic.clone();
                    let inner = f.eval();
                    Arc::new(move |ops: &Ops, v: Value| optic.update(v, &mut |x| inner(ops, x)))
                }
                PfKind::Fold { shared, index } => {
                    let (shared, index) = (shared.clone(), *index);
                    Arc::new(move |ops: &Ops, v: Value| shared.func(index)(ops, v))
                }
                PfKind::TaggedChoice { results } => {
                    let evals: Vec<(String, ViewFn)> = results.iter().map(|(k, r)| (k.clone(), r.view.0.eval())).collect();
                    Arc::new(move |ops: &Ops, v: Value| {
                        let (k, x) = v.into_pair();
                        match evals.iter().find(|(name, _)| matches!(&k, Value::Str(s) if s == name)) {
                            Some((_, f)) => Value::pair(k, f(ops, x)),
                            None => Value::pair(k, x),
                        }
                    })
                }
            })
            .clone()
    }

    fn hash64(&self) -> u64 {
        *self.0.hash.get_or_init(|| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            match &self.0.kind {
                PfKind::Id => 0u8.hash(&mut h),
                PfKind::In => 1u8.hash(&mut h),
                PfKind::Fun { name, .. } => {
                    2u8.hash(&mut h);
                    name.hash(&mut h);
                }
                PfKind::Comp(fs) => {
                    3u8.hash(&mut h);
                    for f in fs {
                        f.hash64().hash(&mut h);
                    }
                }
                PfKind::App { optic, f } => {
                    4u8.hash(&mut h);
                    optic.hash(&mut h);
                    f.hash64().hash(&mut h);
                }
                PfKind::Fold { shared, .. } => {
                    5u8.hash(&mut h);
                    shared.algebra.len().hash(&mut h);
                }
                PfKind::TaggedChoice { results } => {
                    6u8.hash(&mut h);
                    results.len().hash(&mut h);
                }
            }
            self.0.ty.hash(&mut h);
            self.0.new_ty.hash(&mut h);
            h.finish()
        })
    }
}

impl PartialEq for PointFree {
    fn eq(&self, o: &PointFree) -> bool {
        if Arc::ptr_eq(&self.0, &o.0) {
            return true;
        }
        let (a, b) = (&*self.0, &*o.0);
        let types = || a.ty == b.ty && a.new_ty == b.new_ty;
        match (&a.kind, &b.kind) {
            (PfKind::Id, PfKind::Id) | (PfKind::In, PfKind::In) => types(),
            (PfKind::Fun { name: n1, .. }, PfKind::Fun { name: n2, .. }) => n1 == n2 && types(),
            (PfKind::Comp(x), PfKind::Comp(y)) => x == y,
            (PfKind::App { optic: o1, f: f1 }, PfKind::App { optic: o2, f: f2 }) => o1 == o2 && f1 == f2,
            (PfKind::Fold { shared: s1, .. }, PfKind::Fold { shared: s2, .. }) => {
                types() && (Arc::ptr_eq(s1, s2) || *s1.algebra == *s2.algebra)
            }
            (PfKind::TaggedChoice { results: r1 }, PfKind::TaggedChoice { results: r2 }) => {
                types() && r1.len() == r2.len() && r1.iter().all(|(k, v)| r2.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            _ => false,
        }
    }
}

impl Hash for PointFree {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.hash64().hash(h)
    }
}

/// `View<A, B>`: a rewrite function with its input and output type.
#[derive(Clone, PartialEq)]
pub struct View(pub(crate) PointFree);

impl Hash for View {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.hash(h)
    }
}

impl View {
    /// `View.nopView(type)`.
    pub fn nop_view(ty: &Type) -> View {
        View(PointFree::id(ty))
    }
    /// `View.create(name, type, newType, function)`.
    pub fn create(
        name: &str,
        ty: &Type,
        new_ty: &Type,
        f: impl Fn(&Ops, Value) -> Value + Send + Sync + 'static,
    ) -> View {
        View(PointFree::new(PfKind::Fun { name: name.to_string(), f: Arc::new(f) }, ty.clone(), new_ty.clone()))
    }
    /// `Functions.app(profunctorTransformer(optic), function)`.
    pub(crate) fn app(optic: TypedOptic, inner: &View) -> View {
        let (s, t) = (optic.s_type().clone(), optic.t_type().clone());
        View(PointFree::new(PfKind::App { optic, f: inner.0.clone() }, s, t))
    }
    /// `View.create(Functions.in(type))`.
    pub(crate) fn in_(point: &Type, unfolded: &Type) -> View {
        View(PointFree::new(PfKind::In, unfolded.clone(), point.clone()))
    }
    pub(crate) fn tagged_choice(results: Vec<(String, RewriteResult)>, ty: &Type, new_ty: &Type) -> View {
        View(PointFree::new(PfKind::TaggedChoice { results }, ty.clone(), new_ty.clone()))
    }

    /// `view.type()`.
    pub fn ty(&self) -> &Type {
        self.0.ty()
    }
    /// `view.newType()`.
    pub fn new_type(&self) -> &Type {
        self.0.new_ty()
    }
    pub fn is_nop(&self) -> bool {
        self.0.is_id()
    }
    /// `this.compose(that)`: `that` first, then `this`.
    pub fn compose(&self, that: &View) -> View {
        if self.is_nop() {
            that.clone()
        } else if that.is_nop() {
            self.clone()
        } else {
            View(PointFree::comp(&self.0, &that.0))
        }
    }
    /// `view.function().evalCached().apply(ops).apply(value)`.
    pub fn apply(&self, ops: &Ops, value: Value) -> Value {
        (self.0.eval())(ops, value)
    }
}

/// `RewriteResult<A, B>`: a view and the recursion points (`recData`) it touched.
#[derive(Clone)]
pub struct RewriteResult {
    pub view: View,
    pub rec_data: BitSet,
}

impl PartialEq for RewriteResult {
    fn eq(&self, o: &RewriteResult) -> bool {
        self.view == o.view
    }
}
impl Eq for RewriteResult {}
impl Hash for RewriteResult {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.view.hash(h)
    }
}

impl RewriteResult {
    pub fn create(view: View, rec_data: BitSet) -> RewriteResult {
        RewriteResult { view, rec_data }
    }
    /// `RewriteResult.nop(type)`.
    pub fn nop(ty: &Type) -> RewriteResult {
        RewriteResult { view: View::nop_view(ty), rec_data: BitSet::new() }
    }
    /// `this.compose(that)`: `that` first, then `this`.
    pub fn compose(&self, that: &RewriteResult) -> RewriteResult {
        let mut data = self.rec_data;
        if self.view.ty().is_recursive_point() && that.view.ty().is_recursive_point() {
            data.or(that.rec_data);
        }
        RewriteResult { view: self.view.compose(&that.view), rec_data: data }
    }
}

/// `Type.opticView(type, view, optic)`.
pub(crate) fn optic_view(ty: &Type, view: &RewriteResult, optic: TypedOptic) -> RewriteResult {
    if view.view.is_nop() {
        RewriteResult::nop(ty)
    } else {
        RewriteResult::create(View::app(optic, &view.view), view.rec_data)
    }
}

/// `DataFixerUpper.OPTIMIZATION_RULE` vs `PointFreeRule.nop()`. Only part of rule identity here: the optimiser is not
/// ported (it never changes results).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OptimizationRule {
    Standard,
    Nop,
}

// ---- TypeRewriteRule -----------------------------------------------------------------------------------------------

static NEXT_LAZY_ID: AtomicU64 = AtomicU64::new(1);

/// `OrElse.second` (a `Supplier<TypeRewriteRule>`): `TypeRewriteRule::nop` is one shared supplier, every capturing
/// lambda is its own identity.
#[derive(Clone)]
enum Second {
    Nop,
    Lazy { id: u64, get: Arc<dyn Fn() -> TypeRewriteRule + Send + Sync> },
}

impl Second {
    fn get(&self) -> TypeRewriteRule {
        match self {
            Second::Nop => TypeRewriteRule::nop(),
            Second::Lazy { get, .. } => get(),
        }
    }
}

impl PartialEq for Second {
    fn eq(&self, o: &Second) -> bool {
        match (self, o) {
            (Second::Nop, Second::Nop) => true,
            (Second::Lazy { id: a, .. }, Second::Lazy { id: b, .. }) => a == b,
            _ => false,
        }
    }
}

enum RuleKind {
    Nop,
    Seq(Vec<TypeRewriteRule>),
    OrElse(TypeRewriteRule, Second),
    All { rule: TypeRewriteRule, recurse: bool, check_index: bool },
    One(TypeRewriteRule),
    Everywhere { rule: TypeRewriteRule, opt: OptimizationRule, recurse: bool, check_index: bool },
    IfSame { target: Type, value: RewriteResult },
}

struct RuleNode {
    kind: RuleKind,
    hash: u64,
}

/// `TypeRewriteRule`.
#[derive(Clone)]
pub struct TypeRewriteRule(Arc<RuleNode>);

impl PartialEq for TypeRewriteRule {
    fn eq(&self, o: &TypeRewriteRule) -> bool {
        if Arc::ptr_eq(&self.0, &o.0) {
            return true;
        }
        if self.0.hash != o.0.hash {
            return false;
        }
        match (&self.0.kind, &o.0.kind) {
            (RuleKind::Nop, RuleKind::Nop) => true,
            (RuleKind::Seq(a), RuleKind::Seq(b)) => a == b,
            (RuleKind::OrElse(a1, a2), RuleKind::OrElse(b1, b2)) => a1 == b1 && a2 == b2,
            (RuleKind::All { rule: r1, recurse: a1, check_index: c1 }, RuleKind::All { rule: r2, recurse: a2, check_index: c2 }) => {
                r1 == r2 && a1 == a2 && c1 == c2
            }
            (RuleKind::One(a), RuleKind::One(b)) => a == b,
            (
                RuleKind::Everywhere { rule: r1, opt: o1, recurse: a1, check_index: c1 },
                RuleKind::Everywhere { rule: r2, opt: o2, recurse: a2, check_index: c2 },
            ) => r1 == r2 && o1 == o2 && a1 == a2 && c1 == c2,
            (RuleKind::IfSame { target: t1, value: v1 }, RuleKind::IfSame { target: t2, value: v2 }) => t1 == t2 && v1 == v2,
            _ => false,
        }
    }
}
impl Eq for TypeRewriteRule {}
impl Hash for TypeRewriteRule {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.hash.hash(h)
    }
}

fn mix(parts: &[u64]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

impl TypeRewriteRule {
    fn make(kind: RuleKind) -> TypeRewriteRule {
        let hash = match &kind {
            RuleKind::Nop => 1,
            RuleKind::Seq(rules) => mix(&std::iter::once(2).chain(rules.iter().map(|r| r.0.hash)).collect::<Vec<_>>()),
            RuleKind::OrElse(first, second) => mix(&[3, first.0.hash, matches!(second, Second::Nop) as u64]),
            RuleKind::All { rule, recurse, check_index } => mix(&[4, rule.0.hash, *recurse as u64, *check_index as u64]),
            RuleKind::One(rule) => mix(&[5, rule.0.hash]),
            RuleKind::Everywhere { rule, opt, recurse, check_index } => {
                mix(&[6, rule.0.hash, *opt as u64, *recurse as u64, *check_index as u64])
            }
            RuleKind::IfSame { target, value } => mix(&[7, hash_of(target), hash_of(value)]),
        };
        TypeRewriteRule(Arc::new(RuleNode { kind, hash }))
    }

    /// `TypeRewriteRule.nop()` (a singleton: fixes returning it are skipped by `DataFixerUpper.getRule`).
    pub fn nop() -> TypeRewriteRule {
        static NOP: LazyLock<TypeRewriteRule> = LazyLock::new(|| TypeRewriteRule::make(RuleKind::Nop));
        NOP.clone()
    }
    pub fn is_nop(&self) -> bool {
        matches!(self.0.kind, RuleKind::Nop)
    }
    /// `TypeRewriteRule.seq(List)`.
    pub fn seq_list(rules: Vec<TypeRewriteRule>) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::Seq(rules))
    }
    /// `TypeRewriteRule.seq(first, second)`.
    pub fn seq(first: TypeRewriteRule, second: TypeRewriteRule) -> TypeRewriteRule {
        if first.is_nop() {
            second
        } else if second.is_nop() {
            first
        } else {
            TypeRewriteRule::seq_list(vec![first, second])
        }
    }
    /// `TypeRewriteRule.seq(first, rest...)`: right-nested, like the varargs overload.
    pub fn seq_all(first: TypeRewriteRule, rest: Vec<TypeRewriteRule>) -> TypeRewriteRule {
        let mut rest = rest;
        match rest.pop() {
            None => first,
            Some(mut tail) => {
                while let Some(r) = rest.pop() {
                    tail = TypeRewriteRule::seq(r, tail);
                }
                TypeRewriteRule::seq(first, tail)
            }
        }
    }
    /// `TypeRewriteRule.orElse(first, second)` with a constant second rule.
    pub fn or_else(first: TypeRewriteRule, second: TypeRewriteRule) -> TypeRewriteRule {
        TypeRewriteRule::or_else_lazy(first, move || second.clone())
    }
    /// `TypeRewriteRule.orElse(first, Supplier)`; every call is a new supplier identity.
    pub fn or_else_lazy(first: TypeRewriteRule, second: impl Fn() -> TypeRewriteRule + Send + Sync + 'static) -> TypeRewriteRule {
        let id = NEXT_LAZY_ID.fetch_add(1, Ordering::Relaxed);
        TypeRewriteRule::make(RuleKind::OrElse(first, Second::Lazy { id, get: Arc::new(second) }))
    }
    /// `orElse(rule, TypeRewriteRule::nop)` (the shared nop supplier, as used by `Type.everywhere`).
    fn or_else_nop(first: TypeRewriteRule) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::OrElse(first, Second::Nop))
    }
    pub fn all(rule: TypeRewriteRule, recurse: bool, check_index: bool) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::All { rule, recurse, check_index })
    }
    pub fn one(rule: TypeRewriteRule) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::One(rule))
    }
    /// `TypeRewriteRule.once(rule)` = `orElse(rule, () -> one(once(rule)))`.
    pub fn once(rule: TypeRewriteRule) -> TypeRewriteRule {
        let r2 = rule.clone();
        TypeRewriteRule::or_else_lazy(rule, move || TypeRewriteRule::one(TypeRewriteRule::once(r2.clone())))
    }
    pub fn everywhere(rule: TypeRewriteRule, opt: OptimizationRule, recurse: bool, check_index: bool) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::Everywhere { rule, opt, recurse, check_index })
    }
    /// `TypeRewriteRule.ifSame(targetType, value)`.
    pub fn if_same(target: &Type, value: RewriteResult) -> TypeRewriteRule {
        TypeRewriteRule::make(RuleKind::IfSame { target: target.clone(), value })
    }

    /// `rule.rewrite(type)`.
    pub fn rewrite(&self, ty: &Type) -> Option<RewriteResult> {
        match &self.0.kind {
            RuleKind::Nop => Some(RewriteResult::nop(ty)),
            RuleKind::Seq(rules) => {
                let mut result = RewriteResult::nop(ty);
                for rule in rules {
                    let next = rule.rewrite(result.view.new_type())?;
                    result = next.compose(&result);
                }
                Some(result)
            }
            RuleKind::OrElse(first, second) => first.rewrite(ty).or_else(|| second.get().rewrite(ty)),
            RuleKind::All { rule, recurse, check_index } => Some(ty.all(rule, *recurse, *check_index)),
            RuleKind::One(rule) => ty.one(rule),
            RuleKind::Everywhere { rule, opt, recurse, check_index } => ty.everywhere(rule, *opt, *recurse, *check_index),
            RuleKind::IfSame { target, value } => ty.if_same_rewrite(target, value),
        }
    }

    /// The rule `Type.everywhere` rewrites with at every level (`seq(orElse(rule, nop), all(everywhere(rule)))`).
    pub(crate) fn everywhere_step(rule: &TypeRewriteRule, opt: OptimizationRule, recurse: bool, check_index: bool) -> TypeRewriteRule {
        TypeRewriteRule::seq(
            TypeRewriteRule::or_else_nop(rule.clone()),
            TypeRewriteRule::all(TypeRewriteRule::everywhere(rule.clone(), opt, recurse, check_index), recurse, check_index),
        )
    }
}
