//! `TypedOptic`: a type-level description (`sType`/`tType`/`aType`/`bType`) of a focus into a `Value`, plus its
//! (dynamic) behaviour.
//!
//! DFU represents focuses as profunctor optics evaluated with different applicative instances (`Forget`, `ForgetOpt`,
//! `ReForgetC`, `IdF`, `Const`, `FunctionType`). All of them boil down to two operations on the dynamic [`Value`]:
//! *visit* every focused value, and *update* every focused value; both are implemented once by [`TypedOptic::visit`]
//! and [`TypedOptic::update`]. The optic-bound lattice (`Cartesian`/`Cocartesian`/`AffineP`/`TraversalP`) only decides
//! which Java instance may evaluate an optic (a failing `upCast` throws `IllegalArgumentException`); it is not tracked
//! here, so misuse that throws in Java (e.g. `Typed.get` through an `optional`) simply yields the first focus.

use crate::ty::{TaggedChoiceType, Type};
use crate::value::Value;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// One primitive optic (`Optics.*` / `InjTagged` / the merged optics built by `Sum` and `TaggedChoice`).
#[derive(Clone)]
pub(crate) enum Optic {
    /// `Optics.id()`.
    Id,
    /// `Optics.proj1()` (lens on the first of a `Pair`).
    Proj1,
    /// `Optics.proj2()`.
    Proj2,
    /// `Optics.inj1()` (prism on `Either.left`).
    Inj1,
    /// `Optics.inj2()`.
    Inj2,
    /// `Optics.listTraversal()`.
    ListTraversal,
    /// `InjTagged(key)`: prism on a tagged choice entry with that key.
    InjTagged(String),
    /// `Sum.SumType.mergeOptics`: left optic on `Either.left`, right optic on `Either.right`.
    Sum(Box<TypedOptic>, Box<TypedOptic>),
    /// `TaggedChoiceType.findTypeInChildren` with several matching choices: per-key optic, other keys untouched.
    Choice(Arc<Vec<(String, TypedOptic)>>),
}

impl PartialEq for Optic {
    fn eq(&self, o: &Optic) -> bool {
        use Optic::*;
        match (self, o) {
            (Id, Id) | (Proj1, Proj1) | (Proj2, Proj2) | (Inj1, Inj1) | (Inj2, Inj2) | (ListTraversal, ListTraversal) => true,
            (InjTagged(a), InjTagged(b)) => a == b,
            (Sum(a1, a2), Sum(b1, b2)) => a1 == b1 && a2 == b2,
            (Choice(a), Choice(b)) => {
                a.len() == b.len() && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            _ => false,
        }
    }
}

impl Optic {
    fn tag(&self) -> u8 {
        match self {
            Optic::Id => 0,
            Optic::Proj1 => 1,
            Optic::Proj2 => 2,
            Optic::Inj1 => 3,
            Optic::Inj2 => 4,
            Optic::ListTraversal => 5,
            Optic::InjTagged(_) => 6,
            Optic::Sum(..) => 8,
            Optic::Choice(_) => 9,
        }
    }

    fn update(&self, s: Value, f: &mut dyn FnMut(Value) -> Value) -> Value {
        match self {
            Optic::Id => f(s),
            Optic::Proj1 => {
                let (a, b) = s.into_pair();
                Value::pair(f(a), b)
            }
            Optic::Proj2 => {
                let (a, b) = s.into_pair();
                Value::pair(a, f(b))
            }
            Optic::Inj1 => match s {
                Value::Left(a) => Value::left(f(*a)),
                other => other,
            },
            Optic::Inj2 => match s {
                Value::Right(b) => Value::right(f(*b)),
                other => other,
            },
            Optic::ListTraversal => Value::List(s.into_list().into_iter().map(|v| f(v)).collect()),
            Optic::InjTagged(key) => {
                let (k, v) = s.into_pair();
                if matches!(&k, Value::Str(s) if s == key) { Value::pair(k, f(v)) } else { Value::pair(k, v) }
            }
            Optic::Sum(l, r) => match s {
                Value::Left(a) => Value::left(l.update(*a, f)),
                Value::Right(b) => Value::right(r.update(*b, f)),
                other => panic!("expected Either, got {other:?}"),
            },
            Optic::Choice(optics) => {
                let (k, v) = s.into_pair();
                match optics.iter().find(|(name, _)| matches!(&k, Value::Str(s) if s == name)) {
                    Some((_, o)) => Value::pair(k, o.update(v, f)),
                    None => Value::pair(k, v),
                }
            }
        }
    }

    fn visit(&self, s: &Value, f: &mut dyn FnMut(&Value)) {
        match self {
            Optic::Id => f(s),
            Optic::Proj1 => f(s.as_pair().0),
            Optic::Proj2 => f(s.as_pair().1),
            Optic::Inj1 => {
                if let Value::Left(a) = s {
                    f(a)
                }
            }
            Optic::Inj2 => {
                if let Value::Right(b) = s {
                    f(b)
                }
            }
            Optic::ListTraversal => s.as_list().iter().for_each(|v| f(v)),
            Optic::InjTagged(key) => {
                let (k, v) = s.as_pair();
                if matches!(k, Value::Str(s) if s == key) {
                    f(v)
                }
            }
            Optic::Sum(l, r) => match s {
                Value::Left(a) => l.visit(a, f),
                Value::Right(b) => r.visit(b, f),
                other => panic!("expected Either, got {other:?}"),
            },
            Optic::Choice(optics) => {
                let (k, v) = s.as_pair();
                if let Some((_, o)) = optics.iter().find(|(name, _)| matches!(k, Value::Str(s) if s == name)) {
                    o.visit(v, f)
                }
            }
        }
    }
}

/// `TypedOptic.Element`.
#[derive(Clone)]
pub(crate) struct Element {
    pub s: Type,
    pub t: Type,
    pub a: Type,
    pub b: Type,
    pub optic: Optic,
}

impl PartialEq for Element {
    fn eq(&self, o: &Element) -> bool {
        self.s == o.s && self.t == o.t && self.a == o.a && self.b == o.b && self.optic == o.optic
    }
}

/// `TypedOptic<S, T, A, B>`: composition of one or more elements (outermost first).
#[derive(Clone)]
pub struct TypedOptic {
    elements: Arc<Vec<Element>>,
}

impl PartialEq for TypedOptic {
    fn eq(&self, o: &TypedOptic) -> bool {
        Arc::ptr_eq(&self.elements, &o.elements) || *self.elements == *o.elements
    }
}
impl Eq for TypedOptic {}

impl Hash for TypedOptic {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.elements.len().hash(h);
        for e in self.elements.iter() {
            e.s.hash(h);
            e.t.hash(h);
            e.optic.tag().hash(h);
        }
    }
}

impl TypedOptic {
    fn single(s: Type, t: Type, a: Type, b: Type, optic: Optic) -> TypedOptic {
        TypedOptic { elements: Arc::new(vec![Element { s, t, a, b, optic }]) }
    }

    pub fn s_type(&self) -> &Type {
        &self.elements[0].s
    }
    pub fn t_type(&self) -> &Type {
        &self.elements[0].t
    }
    pub fn a_type(&self) -> &Type {
        &self.elements[self.elements.len() - 1].a
    }
    pub fn b_type(&self) -> &Type {
        &self.elements[self.elements.len() - 1].b
    }

    /// `TypedOptic.compose(other)`: `self` is the outer optic.
    pub fn compose(&self, other: &TypedOptic) -> TypedOptic {
        let mut v = Vec::with_capacity(self.elements.len() + other.elements.len());
        v.extend(self.elements.iter().cloned());
        v.extend(other.elements.iter().cloned());
        TypedOptic { elements: Arc::new(v) }
    }

    /// `castOuterUnchecked(sType, tType)`.
    pub fn cast_outer(&self, s: Type, t: Type) -> TypedOptic {
        let mut v: Vec<Element> = (*self.elements).clone();
        v[0].s = s;
        v[0].t = t;
        TypedOptic { elements: Arc::new(v) }
    }

    // ---- constructors (`TypedOptic.*`) -------------------------------------------------------------------------

    /// `TypedOptic.adapter(sType, tType)`.
    pub fn adapter(s: &Type, t: &Type) -> TypedOptic {
        TypedOptic::single(s.clone(), t.clone(), s.clone(), t.clone(), Optic::Id)
    }
    pub fn proj1(f: &Type, g: &Type, new_type: &Type) -> TypedOptic {
        TypedOptic::single(Type::product(f, g), Type::product(new_type, g), f.clone(), new_type.clone(), Optic::Proj1)
    }
    pub fn proj2(f: &Type, g: &Type, new_type: &Type) -> TypedOptic {
        TypedOptic::single(Type::product(f, g), Type::product(f, new_type), g.clone(), new_type.clone(), Optic::Proj2)
    }
    pub fn inj1(f: &Type, g: &Type, new_type: &Type) -> TypedOptic {
        TypedOptic::single(Type::sum(f, g), Type::sum(new_type, g), f.clone(), new_type.clone(), Optic::Inj1)
    }
    pub fn inj2(f: &Type, g: &Type, new_type: &Type) -> TypedOptic {
        TypedOptic::single(Type::sum(f, g), Type::sum(f, new_type), g.clone(), new_type.clone(), Optic::Inj2)
    }
    /// `TypedOptic.list(aType, bType)`.
    pub fn list(a: &Type, b: &Type) -> TypedOptic {
        TypedOptic::single(Type::list(a), Type::list(b), a.clone(), b.clone(), Optic::ListTraversal)
    }
    /// `TypedOptic.compoundListKeys(aType, bType, valueType)`.
    pub fn compound_list_keys(a: &Type, b: &Type, value: &Type) -> TypedOptic {
        TypedOptic::single(
            Type::compound_list(a, value),
            Type::compound_list(b, value),
            Type::product(a, value),
            Type::product(b, value),
            Optic::ListTraversal,
        )
        .compose(&TypedOptic::single(Type::product(a, value), Type::product(b, value), a.clone(), b.clone(), Optic::Proj1))
    }
    /// `TypedOptic.compoundListElements(keyType, aType, bType)`.
    pub fn compound_list_elements(key: &Type, a: &Type, b: &Type) -> TypedOptic {
        TypedOptic::single(
            Type::compound_list(key, a),
            Type::compound_list(key, b),
            Type::product(key, a),
            Type::product(key, b),
            Optic::ListTraversal,
        )
        .compose(&TypedOptic::single(Type::product(key, a), Type::product(key, b), a.clone(), b.clone(), Optic::Proj2))
    }
    /// `TypedOptic.tagged(sType, key, aType, bType)`.
    pub fn tagged(s: &TaggedChoiceType, key: &str, a: &Type, b: &Type) -> TypedOptic {
        let t = s.replace_tagged(key, a, b);
        TypedOptic::single(s.ty().clone(), t, a.clone(), b.clone(), Optic::InjTagged(key.to_string()))
    }

    // ---- crate-internal constructors used by the type implementations -------------------------------------------

    pub(crate) fn id_of(s: Type, t: Type, a: Type, b: Type) -> TypedOptic {
        TypedOptic::single(s, t, a, b, Optic::Id)
    }
    pub(crate) fn proj2_raw(s: Type, t: Type, a: Type, b: Type) -> TypedOptic {
        TypedOptic::single(s, t, a, b, Optic::Proj2)
    }
    pub(crate) fn proj1_raw(s: Type, t: Type, a: Type, b: Type) -> TypedOptic {
        TypedOptic::single(s, t, a, b, Optic::Proj1)
    }
    pub(crate) fn sum_merge(l: &TypedOptic, r: &TypedOptic) -> TypedOptic {
        TypedOptic::single(
            Type::sum(l.s_type(), r.s_type()),
            Type::sum(l.t_type(), r.t_type()),
            l.a_type().clone(),
            l.b_type().clone(),
            Optic::Sum(Box::new(l.clone()), Box::new(r.clone())),
        )
    }
    pub(crate) fn choice(s: Type, t: Type, a: Type, b: Type, optics: Vec<(String, TypedOptic)>) -> TypedOptic {
        TypedOptic::single(s, t, a, b, Optic::Choice(Arc::new(optics)))
    }

    // ---- evaluation ---------------------------------------------------------------------------------------------

    /// Replace every focus `x` of `s` by `f(x)` (`Traversal.wander(IdF, ..)`).
    pub fn update(&self, s: Value, f: &mut dyn FnMut(Value) -> Value) -> Value {
        self.update_from(0, s, f)
    }

    fn update_from(&self, k: usize, s: Value, f: &mut dyn FnMut(Value) -> Value) -> Value {
        match self.elements.get(k) {
            None => f(s),
            Some(e) => e.optic.update(s, &mut |inner| self.update_from(k + 1, inner, &mut *f)),
        }
    }

    /// Call `f` with every focus of `s`, in traversal order.
    pub fn visit(&self, s: &Value, f: &mut dyn FnMut(&Value)) {
        self.visit_from(0, s, f)
    }

    fn visit_from(&self, k: usize, s: &Value, f: &mut dyn FnMut(&Value)) {
        match self.elements.get(k) {
            None => f(s),
            Some(e) => e.optic.visit(s, &mut |inner| self.visit_from(k + 1, inner, &mut *f)),
        }
    }

    /// First focus (`Forget` / `ForgetOpt`).
    pub fn first(&self, s: &Value) -> Option<Value> {
        let mut out = None;
        self.visit(s, &mut |v| {
            if out.is_none() {
                out = Some(v.clone())
            }
        });
        out
    }

    /// All foci (`Const` over the list monoid).
    pub fn all(&self, s: &Value) -> Vec<Value> {
        let mut out = Vec::new();
        self.visit(s, &mut |v| out.push(v.clone()));
        out
    }
}
