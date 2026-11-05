//! DFU `Type<A>` (`com.mojang.datafixers.types.*`): the structural type tree with Java's exact
//! `equals(ignoreRecursionPoints, checkIndex)`, codec (read/write) semantics, finder search order and rewrite
//! (`all`/`one`/`everywhere`) behaviour. Values of these types are dynamic [`Value`]s.

use crate::family::RecursiveTypeFamily;
use crate::optic::TypedOptic;
use crate::rewrite::{OptimizationRule, RewriteResult, TypeRewriteRule, View, optic_view};
use crate::template::Template;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::{DataResult, Dyn, Dynamic, Msg, Ops};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

// ---- errors / matchers --------------------------------------------------------------------------------------------

/// `Type.FieldNotFoundException` (`Continue` is its `Type.Continue` subclass: "keep searching in the children").
#[derive(Clone, Debug, PartialEq)]
pub enum FindError {
    Continue,
    NotFound(String),
}

impl std::fmt::Display for FindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindError::Continue => f.write_str("Continue"),
            FindError::NotFound(m) => f.write_str(m),
        }
    }
}

/// `Type.TypeMatcher`: the two finders' matchers (`FieldFinder.Matcher`, `NamedChoiceFinder.Matcher`).
#[derive(Clone)]
pub(crate) enum Matcher {
    Field { name: Option<String>, ty: Type, result: Type },
    NamedChoice { name: String, ty: Type, result: Type },
}

impl Matcher {
    pub(crate) fn ty(&self) -> &Type {
        match self {
            Matcher::Field { ty, .. } | Matcher::NamedChoice { ty, .. } => ty,
        }
    }
    pub(crate) fn result(&self) -> &Type {
        match self {
            Matcher::Field { result, .. } | Matcher::NamedChoice { result, .. } => result,
        }
    }

    pub(crate) fn matches(&self, target: &Type) -> Result<TypedOptic, FindError> {
        match self {
            Matcher::Field { name, ty, result } => {
                if name.is_none() && ty.equals_with(target, true, false) {
                    return Ok(TypedOptic::id_of(target.clone(), result.clone(), target.clone(), result.clone()));
                }
                match target.kind() {
                    TypeKind::Tag(tag_name, element) => {
                        if Some(tag_name) != name.as_ref() {
                            return Err(FindError::NotFound(format!(
                                "Not found: \"{}\" (in type: {})",
                                name.as_deref().unwrap_or("null"),
                                target
                            )));
                        }
                        if ty != element {
                            return Err(FindError::NotFound(format!(
                                "Type error for field \"{}\": expected type: {}, actual type: {})",
                                tag_name, ty, element
                            )));
                        }
                        Ok(TypedOptic::id_of(target.clone(), Type::tag(tag_name, result), ty.clone(), result.clone()))
                    }
                    TypeKind::TaggedChoice(tc) if Some(&tc.name) == name.as_ref() => {
                        if *ty != tc.key_type {
                            return Err(FindError::NotFound(format!(
                                "Type error for field \"{}\": expected type: {}, actual type: {})",
                                tc.name, ty, tc.key_type
                            )));
                        }
                        if ty != result {
                            return Err(FindError::NotFound("TaggedChoiceType key type change is unsupported.".into()));
                        }
                        Ok(TypedOptic::proj1_raw(target.clone(), target.clone(), ty.clone(), ty.clone()))
                    }
                    _ => Err(FindError::Continue),
                }
            }
            Matcher::NamedChoice { name, ty, result } => match target.kind() {
                TypeKind::TaggedChoice(tc) => match tc.get(name) {
                    Some(element) => {
                        if ty != element {
                            Err(FindError::NotFound(format!(
                                "Type error for choice type \"{}\": expected type: {}, actual type: {})",
                                name, target, element
                            )))
                        } else {
                            Ok(TypedOptic::tagged(&TaggedChoiceType(target.clone()), name, ty, result))
                        }
                    }
                    None => Err(FindError::Continue),
                },
                TypeKind::Tag(..) => Err(FindError::NotFound("in tag".into())),
                _ => Err(FindError::Continue),
            },
        }
    }
}

// ---- hook functions -----------------------------------------------------------------------------------------------

/// `Hook.HookFunction`: a pre-read / post-write transformation of the raw data. Equality is identity (like the Java
/// anonymous classes): keep one `HookFunction` value (e.g. in a `LazyLock`) per Java constant.
#[derive(Clone)]
pub struct HookFunction(Arc<dyn Fn(&Ops, Dyn) -> Dyn + Send + Sync>);

impl HookFunction {
    pub fn new(f: impl Fn(&Ops, Dyn) -> Dyn + Send + Sync + 'static) -> HookFunction {
        HookFunction(Arc::new(f))
    }
    /// `HookFunction.IDENTITY`.
    pub fn identity() -> HookFunction {
        static IDENTITY: LazyLock<HookFunction> = LazyLock::new(|| HookFunction::new(|_, v| v));
        IDENTITY.clone()
    }
    pub fn apply(&self, ops: &Ops, value: Dyn) -> Dyn {
        (self.0)(ops, value)
    }
    pub(crate) fn addr(&self) -> usize {
        Arc::as_ptr(&self.0) as *const () as usize
    }
}

impl PartialEq for HookFunction {
    fn eq(&self, o: &HookFunction) -> bool {
        self.addr() == o.addr()
    }
}
impl Eq for HookFunction {}
impl Hash for HookFunction {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.addr().hash(h)
    }
}

// ---- type nodes ---------------------------------------------------------------------------------------------------

/// The primitive codecs behind `DSL.intType()` and friends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PrimKind {
    Bool,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    Str,
    /// `NamespacedSchema.namespacedString()`: reads through `ensureNamespaced`.
    NamespacedStr,
}

/// `NamespacedSchema.ensureNamespaced`.
pub fn ensure_namespaced(input: &str) -> String {
    match mc_core::ident::Identifier::try_parse(input) {
        Some(id) => id.to_string(),
        None => input.to_string(),
    }
}

impl PrimKind {
    fn name(self) -> &'static str {
        match self {
            PrimKind::Bool => "Bool",
            PrimKind::Byte => "Byte",
            PrimKind::Short => "Short",
            PrimKind::Int => "Int",
            PrimKind::Long => "Long",
            PrimKind::Float => "Float",
            PrimKind::Double => "Double",
            PrimKind::Str => "String",
            PrimKind::NamespacedStr => "NamespacedString",
        }
    }

    fn read(self, ops: &Ops, input: &Dyn) -> DataResult<Value> {
        match self {
            PrimKind::Bool => mc_dfu::BOOL.parse(ops, input).map(Value::Bool),
            PrimKind::Byte => mc_dfu::BYTE.parse(ops, input).map(Value::Byte),
            PrimKind::Short => mc_dfu::SHORT.parse(ops, input).map(Value::Short),
            PrimKind::Int => mc_dfu::INT.parse(ops, input).map(Value::Int),
            PrimKind::Long => mc_dfu::LONG.parse(ops, input).map(Value::Long),
            PrimKind::Float => mc_dfu::FLOAT.parse(ops, input).map(Value::Float),
            PrimKind::Double => mc_dfu::DOUBLE.parse(ops, input).map(Value::Double),
            PrimKind::Str => ops.get_string_value(input).map(Value::Str),
            PrimKind::NamespacedStr => ops.get_string_value(input).map(|s| Value::Str(ensure_namespaced(&s))),
        }
    }

    fn write(self, ops: &Ops, v: &Value) -> Dyn {
        match (self, v) {
            (PrimKind::Bool, Value::Bool(b)) => ops.create_boolean(*b),
            (PrimKind::Byte, Value::Byte(b)) => ops.create_byte(*b),
            (PrimKind::Short, Value::Short(b)) => ops.create_short(*b),
            (PrimKind::Int, Value::Int(b)) => ops.create_int(*b),
            (PrimKind::Long, Value::Long(b)) => ops.create_long(*b),
            (PrimKind::Float, Value::Float(b)) => ops.create_float(*b),
            (PrimKind::Double, Value::Double(b)) => ops.create_double(*b),
            (PrimKind::Str | PrimKind::NamespacedStr, Value::Str(s)) => ops.create_string(s.clone()),
            (k, v) => panic!("value {v:?} does not match primitive type {}", k.name()),
        }
    }
}

pub(crate) struct TaggedChoiceData {
    pub name: String,
    pub key_type: Type,
    /// In the iteration order of the fastutil `Object2ObjectOpenHashMap` Java holds.
    pub types: Vec<(String, Type)>,
    index: HashMap<String, usize>,
}

impl TaggedChoiceData {
    pub fn get(&self, key: &str) -> Option<&Type> {
        self.index.get(key).map(|&i| &self.types[i].1)
    }
}

pub(crate) enum TypeKind {
    Primitive(PrimKind),
    EmptyPart,
    Passthrough,
    Product(Type, Type),
    Sum(Type, Type),
    List(Type),
    CompoundList(Type, Type),
    Tag(String, Type),
    Named(String, Type),
    Check { name: String, index: i32, expected_index: i32, delegate: Type },
    Hook { delegate: Type, pre: HookFunction, post: HookFunction },
    TaggedChoice(TaggedChoiceData),
    RecursivePoint { family: Arc<RecursiveTypeFamily>, index: i32, unfold: OnceLock<Type> },
}

pub(crate) struct TypeNode {
    kind: TypeKind,
    template: OnceLock<Template>,
    hash: OnceLock<u64>,
}

/// A DFU `Type<?>`: cheap to clone, shared structure.
#[derive(Clone)]
pub struct Type(pub(crate) Arc<TypeNode>);

/// How the data left over after a decode relates to the decoder's input (`Pair.getSecond()` of `Decoder.decode`):
/// map codecs leave their input untouched, which is represented without copying.
pub(crate) enum Rest {
    Same,
    Val(Dyn),
}

impl Rest {
    fn view<'a>(&'a self, input: &'a Dyn) -> &'a Dyn {
        match self {
            Rest::Same => input,
            Rest::Val(d) => d,
        }
    }
    pub(crate) fn resolve(self, input: &Dyn) -> Dyn {
        match self {
            Rest::Same => input.clone(),
            Rest::Val(d) => d,
        }
    }
}

type Decoded = DataResult<(Value, Rest)>;

fn singleton(kind: TypeKind) -> Type {
    Type::new(kind)
}

static BOOL: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Bool)));
static INT: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Int)));
static LONG: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Long)));
static BYTE: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Byte)));
static SHORT: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Short)));
static FLOAT: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Float)));
static DOUBLE: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Double)));
static STRING: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::Str)));
static NAMESPACED_STRING: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Primitive(PrimKind::NamespacedStr)));
static EMPTY_PART: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::EmptyPart));
static EMPTY_PASSTHROUGH: LazyLock<Type> = LazyLock::new(|| singleton(TypeKind::Passthrough));

/// `DSL.taggedChoiceType` cache (`TAGGED_CHOICE_TYPE_CACHE`): equal (name, key type, types) give the very same type
/// instance, and with it the iteration order of the first construction.
static TAGGED_CHOICE_CACHE: LazyLock<Mutex<HashMap<TaggedChoiceKey, Type>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(PartialEq, Eq, Hash)]
struct TaggedChoiceKey {
    name: String,
    key_type: Type,
    sorted: Vec<(String, Type)>,
}

/// Iteration order of `new Object2ObjectOpenHashMap<>(m)` for a map `m` iterating as `entries`.
pub(crate) fn fast_order<V>(entries: Vec<(String, V)>) -> Vec<(String, V)> {
    mc_core::java_collections::fast_open_hash_order_string(entries.len(), entries)
}

impl Type {
    pub(crate) fn new(kind: TypeKind) -> Type {
        Type(Arc::new(TypeNode { kind, template: OnceLock::new(), hash: OnceLock::new() }))
    }

    pub(crate) fn kind(&self) -> &TypeKind {
        &self.0.kind
    }

    // ---- constructors (`DSL.*` for types) -----------------------------------------------------------------------

    pub fn bool() -> Type {
        BOOL.clone()
    }
    pub fn int() -> Type {
        INT.clone()
    }
    pub fn long() -> Type {
        LONG.clone()
    }
    pub fn byte() -> Type {
        BYTE.clone()
    }
    pub fn short() -> Type {
        SHORT.clone()
    }
    pub fn float() -> Type {
        FLOAT.clone()
    }
    pub fn double() -> Type {
        DOUBLE.clone()
    }
    pub fn string() -> Type {
        STRING.clone()
    }
    /// `NamespacedSchema.namespacedString()`.
    pub fn namespaced_string() -> Type {
        NAMESPACED_STRING.clone()
    }
    /// `DSL.emptyPartType()`.
    pub fn empty_part() -> Type {
        EMPTY_PART.clone()
    }
    /// `DSL.remainderType()`.
    pub fn remainder() -> Type {
        EMPTY_PASSTHROUGH.clone()
    }
    pub fn product(first: &Type, second: &Type) -> Type {
        Type::new(TypeKind::Product(first.clone(), second.clone()))
    }
    pub fn sum(first: &Type, second: &Type) -> Type {
        Type::new(TypeKind::Sum(first.clone(), second.clone()))
    }
    pub fn list(element: &Type) -> Type {
        Type::new(TypeKind::List(element.clone()))
    }
    pub fn compound_list(key: &Type, element: &Type) -> Type {
        Type::new(TypeKind::CompoundList(key.clone(), element.clone()))
    }
    pub fn tag(name: &str, element: &Type) -> Type {
        Type::new(TypeKind::Tag(name.to_string(), element.clone()))
    }
    pub fn named(name: &str, element: &Type) -> Type {
        Type::new(TypeKind::Named(name.to_string(), element.clone()))
    }
    pub fn check(name: &str, index: i32, expected_index: i32, delegate: &Type) -> Type {
        Type::new(TypeKind::Check { name: name.to_string(), index, expected_index, delegate: delegate.clone() })
    }
    pub fn hook(delegate: &Type, pre: &HookFunction, post: &HookFunction) -> Type {
        Type::new(TypeKind::Hook { delegate: delegate.clone(), pre: pre.clone(), post: post.clone() })
    }
    pub(crate) fn recursive_point(family: Arc<RecursiveTypeFamily>, index: i32) -> Type {
        Type::new(TypeKind::RecursivePoint { family, index, unfold: OnceLock::new() })
    }
    /// `DSL.taggedChoiceType(name, keyType, types)`: `types` in the iteration order of the Java map passed in.
    pub fn tagged_choice(name: &str, key_type: &Type, types: Vec<(String, Type)>) -> Type {
        let mut sorted = types.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let key = TaggedChoiceKey { name: name.to_string(), key_type: key_type.clone(), sorted };
        if let Some(t) = TAGGED_CHOICE_CACHE.lock().unwrap().get(&key) {
            return t.clone();
        }
        let ordered = fast_order(types);
        let index = ordered.iter().enumerate().map(|(i, (k, _))| (k.clone(), i)).collect();
        let ty = Type::new(TypeKind::TaggedChoice(TaggedChoiceData { name: name.to_string(), key_type: key_type.clone(), types: ordered, index }));
        TAGGED_CHOICE_CACHE.lock().unwrap().entry(key).or_insert(ty).clone()
    }

    // ---- inspection ---------------------------------------------------------------------------------------------

    pub fn is_recursive_point(&self) -> bool {
        matches!(self.0.kind, TypeKind::RecursivePoint { .. })
    }
    /// `(index, family)` of a `RecursivePointType`.
    pub fn recursive_point_index(&self) -> Option<i32> {
        match &self.0.kind {
            TypeKind::RecursivePoint { index, .. } => Some(*index),
            _ => None,
        }
    }
    pub(crate) fn family(&self) -> Option<&Arc<RecursiveTypeFamily>> {
        match &self.0.kind {
            TypeKind::RecursivePoint { family, .. } => Some(family),
            _ => None,
        }
    }
    /// `TagType.name()` / `element()`.
    pub fn as_tag(&self) -> Option<(&str, &Type)> {
        match &self.0.kind {
            TypeKind::Tag(n, e) => Some((n, e)),
            _ => None,
        }
    }
    /// `NamedType.name()` / `element()`.
    pub fn as_named(&self) -> Option<(&str, &Type)> {
        match &self.0.kind {
            TypeKind::Named(n, e) => Some((n, e)),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&Type> {
        match &self.0.kind {
            TypeKind::List(e) => Some(e),
            _ => None,
        }
    }
    pub fn as_product(&self) -> Option<(&Type, &Type)> {
        match &self.0.kind {
            TypeKind::Product(a, b) => Some((a, b)),
            _ => None,
        }
    }
    pub fn as_sum(&self) -> Option<(&Type, &Type)> {
        match &self.0.kind {
            TypeKind::Sum(a, b) => Some((a, b)),
            _ => None,
        }
    }
    pub fn as_compound_list(&self) -> Option<(&Type, &Type)> {
        match &self.0.kind {
            TypeKind::CompoundList(a, b) => Some((a, b)),
            _ => None,
        }
    }
    pub fn as_tagged_choice(&self) -> Option<TaggedChoiceType> {
        match &self.0.kind {
            TypeKind::TaggedChoice(_) => Some(TaggedChoiceType(self.clone())),
            _ => None,
        }
    }
    /// `RecursivePointType.unfold()` (identity for other types is NOT provided: Java requires a point).
    pub fn unfold(&self) -> Type {
        match &self.0.kind {
            TypeKind::RecursivePoint { family, index, unfold } => {
                unfold.get_or_init(|| family.template().apply_at(family, *index)).clone()
            }
            _ => panic!("Not recursive: {self}"),
        }
    }

    // ---- equality / hashing -------------------------------------------------------------------------------------

    /// `type.equals(other, ignoreRecursionPoints, checkIndex)`.
    pub fn equals_with(&self, other: &Type, ignore_recursion_points: bool, check_index: bool) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        let (i, c) = (ignore_recursion_points, check_index);
        use TypeKind::*;
        match (&self.0.kind, &other.0.kind) {
            (Product(a1, a2), Product(b1, b2)) | (Sum(a1, a2), Sum(b1, b2)) | (CompoundList(a1, a2), CompoundList(b1, b2)) => {
                a1.equals_with(b1, i, c) && a2.equals_with(b2, i, c)
            }
            (List(a), List(b)) => a.equals_with(b, i, c),
            (Tag(n1, a), Tag(n2, b)) | (Named(n1, a), Named(n2, b)) => n1 == n2 && a.equals_with(b, i, c),
            (
                Check { index: i1, expected_index: e1, delegate: d1, .. },
                Check { index: i2, expected_index: e2, delegate: d2, .. },
            ) => i1 == i2 && e1 == e2 && (!c || d1.equals_with(d2, i, c)),
            (Hook { delegate: d1, pre: p1, post: q1 }, Hook { delegate: d2, pre: p2, post: q2 }) => {
                d1.equals_with(d2, i, c) && p1 == p2 && q1 == q2
            }
            (TaggedChoice(a), TaggedChoice(b)) => {
                a.name == b.name
                    && a.key_type.equals_with(&b.key_type, i, c)
                    && a.types.len() == b.types.len()
                    && a.types.iter().all(|(k, t)| b.get(k).is_some_and(|u| t.equals_with(u, i, c)))
            }
            (RecursivePoint { family: f1, index: x1, .. }, RecursivePoint { family: f2, index: x2, .. }) => {
                (i || f1 == f2) && x1 == x2
            }
            // primitives, empty part and remainder are equal only to themselves (identity, checked above)
            _ => false,
        }
    }

    fn hash64(&self) -> u64 {
        *self.0.hash.get_or_init(|| {
            use TypeKind::*;
            let comb = |a: u64, b: u64| a.wrapping_mul(31).wrapping_add(b);
            let str_hash = |s: &str| mc_core::math::java::java_string_hash(s) as u32 as u64;
            match &self.0.kind {
                Primitive(_) | EmptyPart | Passthrough => Arc::as_ptr(&self.0) as usize as u64,
                Product(a, b) | Sum(a, b) | CompoundList(a, b) => comb(a.hash64(), b.hash64()),
                List(a) => a.hash64() ^ 0x5bd1e995,
                Tag(n, a) | Named(n, a) => comb(str_hash(n), a.hash64()),
                Check { index, expected_index, delegate, .. } => comb(comb(*index as u64, *expected_index as u64), delegate.hash64()),
                Hook { delegate, pre, post } => comb(comb(delegate.hash64(), pre.addr() as u64), post.addr() as u64),
                TaggedChoice(tc) => {
                    let entries = tc.types.iter().fold(0u64, |acc, (k, t)| acc.wrapping_add(str_hash(k) ^ t.hash64()));
                    comb(comb(str_hash(&tc.name), tc.key_type.hash64()), entries)
                }
                RecursivePoint { family, index, .. } => comb(family.template().hash64(), *index as u64),
            }
        })
    }

    // ---- templates ----------------------------------------------------------------------------------------------

    /// `type.template()`.
    pub fn template(&self) -> Template {
        self.0
            .template
            .get_or_init(|| {
                use TypeKind::*;
                match &self.0.kind {
                    Primitive(_) | EmptyPart | Passthrough => Template::const_type(self),
                    Product(a, b) => Template::product(&a.template(), &b.template()),
                    Sum(a, b) => Template::sum(&a.template(), &b.template()),
                    List(a) => Template::list(&a.template()),
                    CompoundList(k, v) => Template::compound_list(&k.template(), &v.template()),
                    Tag(n, a) => Template::tag(n, &a.template()),
                    Named(n, a) => Template::named(n, &a.template()),
                    Check { name, expected_index, delegate, .. } => Template::check(name, *expected_index, &delegate.template()),
                    Hook { delegate, pre, post } => Template::hook(&delegate.template(), pre, post),
                    TaggedChoice(tc) => {
                        let templates: Vec<(String, Template)> = tc.types.iter().map(|(k, t)| (k.clone(), t.template())).collect();
                        // `new Object2ObjectOpenHashMap<>(types.size())` filled in iteration order, then copied by `taggedChoice`
                        Template::tagged_choice(&tc.name, &tc.key_type, fast_order(templates))
                    }
                    RecursivePoint { index, .. } => Template::id(*index),
                }
            })
            .clone()
    }

    // ---- codec --------------------------------------------------------------------------------------------------

    /// `type.codec().decode(ops, input)`.
    pub(crate) fn decode(&self, ops: &Ops, input: &Dyn) -> Decoded {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(p) => p.read(ops, input).map(|v| (v, Rest::Val(Dyn::Empty))),
            EmptyPart => match ops.get_map(input) {
                DataResult::Success(_) => DataResult::success((Value::Unit, Rest::Same)),
                DataResult::Error { message, .. } => DataResult::error_msg(message, None),
            },
            Passthrough => DataResult::success((Value::Dynamic(Dynamic::new(*ops, input.clone())), Rest::Val(Dyn::Empty))),
            Product(first, second) => first.decode(ops, input).flat_map(|(a, rest1)| {
                second
                    .decode(ops, rest1.view(input))
                    .map(|(b, rest2)| (Value::pair(a, b), if matches!(rest2, Rest::Same) { rest1 } else { rest2 }))
            }),
            Sum(first, second) => {
                let first_read = first.decode(ops, input).map(|(a, r)| (Value::left(a), r));
                if first_read.is_success() {
                    return first_read;
                }
                let second_read = second.decode(ops, input).map(|(b, r)| (Value::right(b), r));
                if second_read.is_success() {
                    return second_read;
                }
                if first_read.has_result_or_partial() {
                    return first_read;
                }
                if second_read.has_result_or_partial() {
                    return second_read;
                }
                let (m1, m2) = (first_read.error_msg_ref().cloned().unwrap(), second_read.error_msg_ref().cloned().unwrap());
                DataResult::error(move || format!("Failed to parse either. First: {}; Second: {}", m1.get(), m2.get()))
            }
            List(element) => decode_list(element, ops, input),
            CompoundList(key, element) => decode_compound_list(key, element, ops, input),
            Tag(name, element) => match ops.get_map(input) {
                DataResult::Success(map) => match map.get(name) {
                    None => {
                        let n = name.clone();
                        DataResult::error(move || format!("No key {n} in MapLike[..]"))
                    }
                    Some(v) => element.decode(ops, v).map(|(a, _)| (a, Rest::Same)),
                },
                DataResult::Error { message, .. } => DataResult::error_msg(message, None),
            },
            Named(name, element) => element.decode(ops, input).map(|(v, r)| (Value::named(name.clone(), v), r)),
            Check { index, expected_index, delegate, .. } => {
                if index != expected_index {
                    let (i, e) = (*index, *expected_index);
                    DataResult::error(move || format!("Index mismatch: {i} != {e}"))
                } else {
                    delegate.decode(ops, input)
                }
            }
            Hook { delegate, pre, .. } => {
                let hooked = pre.apply(ops, input.clone());
                delegate.decode(ops, &hooked).map(|(v, r)| {
                    let r = match r {
                        Rest::Same => Rest::Val(hooked),
                        other => other,
                    };
                    (v, r)
                })
            }
            TaggedChoice(tc) => {
                let map = match ops.get_map(input) {
                    DataResult::Success(m) => m,
                    DataResult::Error { message, .. } => return DataResult::error_msg(message, None),
                };
                let key = match map.get(&tc.name) {
                    None => {
                        let n = tc.name.clone();
                        return DataResult::error(move || format!("No key {n} in MapLike[..]"));
                    }
                    Some(k) => match tc.key_type.decode(ops, k) {
                        DataResult::Success((k, _)) => k,
                        DataResult::Error { message, .. } => return DataResult::error_msg(message, None),
                    },
                };
                let name = key.as_str().to_string();
                match tc.get(&name) {
                    None => DataResult::error(move || format!("Unsupported key: {name}")),
                    Some(ty) => ty.decode(ops, input).map(|(v, _)| (Value::pair(key, v), Rest::Same)),
                }
            }
            RecursivePoint { .. } => self.unfold().decode(ops, input),
        }
    }

    /// `type.codec().encode(value, ops, prefix)`.
    pub(crate) fn encode(&self, ops: &Ops, value: &Value, prefix: Dyn) -> DataResult<Dyn> {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(p) => ops.merge_to_primitive(&prefix, p.write(ops, value)),
            EmptyPart => put_entries(ops, prefix, Vec::new()),
            Passthrough => encode_passthrough(ops, value.as_dynamic(), prefix),
            Product(first, second) => {
                let (a, b) = value.as_pair();
                second.encode(ops, b, prefix).flat_map(|r| first.encode(ops, a, r))
            }
            Sum(first, second) => match value {
                Value::Left(a) => first.encode(ops, a, prefix),
                Value::Right(b) => second.encode(ops, b, prefix),
                other => panic!("expected Either, got {other:?}"),
            },
            List(element) => {
                let mut b = ops.list_builder();
                for el in value.as_list() {
                    b.add_result(element.encode(ops, el, Dyn::Empty));
                }
                b.build(&prefix)
            }
            CompoundList(key, element) => {
                let mut b = ops.map_builder();
                for entry in value.as_list() {
                    let (k, v) = entry.as_pair();
                    b.add_results(key.encode(ops, k, Dyn::Empty), element.encode(ops, v, Dyn::Empty));
                }
                b.build(&prefix)
            }
            Tag(name, element) => match element.encode(ops, value, Dyn::Empty) {
                DataResult::Success(d) => put_entries(ops, prefix, vec![(Dyn::Str(name.clone()), d)]),
                DataResult::Error { message, partial } => match partial {
                    Some(d) => match put_entries(ops, prefix, vec![(Dyn::Str(name.clone()), d)]) {
                        DataResult::Success(r) => DataResult::error_msg(message, Some(r)),
                        DataResult::Error { message: m2, partial } => {
                            DataResult::error_msg(Msg::new(move || format!("{}; {}", message.get(), m2.get())), partial)
                        }
                    },
                    None => DataResult::error_msg(message, None),
                },
            },
            Named(name, element) => {
                let (n, v) = value.as_pair();
                if n.as_str() != name {
                    let (expected, got) = (name.clone(), n.as_str().to_string());
                    DataResult::error_partial(
                        move || format!("Named type name doesn't match: expected: {expected}, got: {got}"),
                        prefix,
                    )
                } else {
                    element.encode(ops, v, prefix)
                }
            }
            Check { delegate, .. } => delegate.encode(ops, value, prefix),
            Hook { delegate, post, .. } => delegate.encode(ops, value, prefix).map(|v| post.apply(ops, v)),
            TaggedChoice(tc) => {
                let (key, v) = value.as_pair();
                let key_name = key.as_str();
                let Some(ty) = tc.get(key_name) else {
                    let k = key_name.to_string();
                    return DataResult::error(move || format!("Unsupported key: {k}"));
                };
                // assumeMapUnsafe(type.codec()): encode the value, add its entries to the builder, then the type key
                let encoded = ty.encode(ops, v, Dyn::Empty);
                let mut b = ops.map_builder();
                let entries_result = encoded.flat_map(|d| match ops.get_map(&d) {
                    DataResult::Success(m) => DataResult::success(m.entries().to_vec()),
                    DataResult::Error { message, .. } => DataResult::error_msg(message, None),
                });
                match &entries_result {
                    DataResult::Success(entries) => {
                        for (k, v) in entries {
                            b.add_key(k.clone(), v.clone());
                        }
                    }
                    DataResult::Error { .. } => {
                        b.with_errors_from(&entries_result);
                    }
                }
                b.add_result(&tc.name, tc.key_type.encode(ops, key, Dyn::Empty));
                b.build(&prefix)
            }
            RecursivePoint { .. } => self.unfold().encode(ops, value, prefix),
        }
    }

    // ---- reading / writing whole typed values -------------------------------------------------------------------

    /// `type.read(dynamic)`.
    pub fn read(&self, input: &Dynamic) -> DataResult<(Value, Dynamic)> {
        let ops = input.ops;
        self.decode(&ops, &input.value).map(|(v, r)| (v, Dynamic::new(ops, r.resolve(&input.value))))
    }
    /// `type.readTyped(dynamic)`.
    pub fn read_typed(&self, input: &Dynamic) -> DataResult<(Typed, Dynamic)> {
        self.read(input).map(|(v, rest)| (Typed::new(self.clone(), input.ops, v), rest))
    }
    /// `type.readTyped(ops, input)`.
    pub fn read_typed_ops(&self, ops: &Ops, input: &Dyn) -> DataResult<(Typed, Dyn)> {
        self.decode(ops, input).map(|(v, r)| (Typed::new(self.clone(), *ops, v), r.resolve(input)))
    }
    /// `type.write(ops, value)`.
    pub fn write(&self, ops: &Ops, value: &Value) -> DataResult<Dyn> {
        self.encode(ops, value, Dyn::Empty)
    }
    /// `type.writeDynamic(ops, value)`.
    pub fn write_dynamic(&self, ops: &Ops, value: &Value) -> DataResult<Dynamic> {
        self.write(ops, value).map(|d| Dynamic::new(*ops, d))
    }

    /// `type.readAndWrite(ops, expectedType, rule, fRule, input)`.
    pub fn read_and_write(&self, ops: &Ops, expected: &Type, rule: &TypeRewriteRule, opt: OptimizationRule, input: &Dyn) -> DataResult<Dyn> {
        let Some(result) = self.rewrite(rule, opt) else {
            let input = input.clone();
            return DataResult::error_partial(move || "Could not build a rewrite rule".to_string(), input);
        };
        let view = result.view;
        if view.is_nop() {
            return DataResult::success(input.clone());
        }
        self.decode(ops, input).flat_map(|(value, rest)| {
            if !expected.equals_with(view.new_type(), true, true) {
                return DataResult::error_str("Rewritten type doesn't match");
            }
            let fixed = view.apply(ops, value);
            view.new_type().encode(ops, &fixed, rest.resolve(input))
        })
    }

    // ---- rewriting ----------------------------------------------------------------------------------------------

    /// `type.rewriteOrNop(rule)`.
    pub fn rewrite_or_nop(&self, rule: &TypeRewriteRule) -> RewriteResult {
        rule.rewrite(self).unwrap_or_else(|| RewriteResult::nop(self))
    }

    /// `type.rewrite(rule, fRule)` with the global `REWRITE_CACHE`.
    pub fn rewrite(&self, rule: &TypeRewriteRule, opt: OptimizationRule) -> Option<RewriteResult> {
        let key = (self.clone(), rule.clone(), opt);
        if let Some(r) = REWRITE_CACHE.lock().unwrap().get(&key) {
            return r.clone();
        }
        let result = rule.rewrite(self);
        REWRITE_CACHE.lock().unwrap().insert(key, result.clone());
        result
    }

    /// `type.all(rule, recurse, checkIndex)`.
    pub fn all(&self, rule: &TypeRewriteRule, recurse: bool, check_index: bool) -> RewriteResult {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(_) | EmptyPart | Passthrough => RewriteResult::nop(self),
            Product(first, second) => self.merge_product(first.rewrite_or_nop(rule), second.rewrite_or_nop(rule)),
            Sum(first, second) => self.merge_sum(first.rewrite_or_nop(rule), second.rewrite_or_nop(rule)),
            List(element) => self.fix_list(element, element.rewrite_or_nop(rule)),
            CompoundList(key, element) => self.merge_compound_list(key.rewrite_or_nop(rule), element.rewrite_or_nop(rule)),
            Tag(_, element) => self.wrap_tag(element.rewrite_or_nop(rule)),
            Named(name, element) => self.fix_named(name, element.rewrite_or_nop(rule)),
            Check { index, expected_index, delegate, .. } => {
                if check_index && index != expected_index {
                    RewriteResult::nop(self)
                } else {
                    self.fix_check(delegate.rewrite_or_nop(rule))
                }
            }
            Hook { delegate, .. } => self.fix_hook(delegate.rewrite_or_nop(rule)),
            TaggedChoice(tc) => self.all_tagged_choice(tc, rule),
            RecursivePoint { .. } => self.unfold().all(rule, recurse, check_index),
        }
    }

    /// `type.one(rule)`.
    pub fn one(&self, rule: &TypeRewriteRule) -> Option<RewriteResult> {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(_) | EmptyPart | Passthrough => None,
            Product(first, second) => rule
                .rewrite(first)
                .map(|v| fix_product_left(self, first, second, &v))
                .or_else(|| rule.rewrite(second).map(|v| fix_product_right(self, first, second, &v))),
            Sum(first, second) => rule
                .rewrite(first)
                .map(|v| fix_sum_left(self, first, second, &v))
                .or_else(|| rule.rewrite(second).map(|v| fix_sum_right(self, first, second, &v))),
            List(element) => rule.rewrite(element).map(|v| self.fix_list(element, v)),
            CompoundList(key, element) => rule
                .rewrite(key)
                .map(|v| fix_compound_keys(self, key, element, &v))
                .or_else(|| rule.rewrite(element).map(|v| fix_compound_values(self, key, element, &v))),
            Tag(_, element) => rule.rewrite(element).map(|v| self.wrap_tag(v)),
            Named(name, element) => rule.rewrite(element).map(|v| self.fix_named(name, v)),
            Check { delegate, .. } => rule.rewrite(delegate).map(|v| self.fix_check(v)),
            Hook { delegate, .. } => rule.rewrite(delegate).map(|v| self.fix_hook(v)),
            TaggedChoice(tc) => {
                let me = TaggedChoiceType(self.clone());
                tc.types.iter().find_map(|(key, ty)| rule.rewrite(ty).map(|r| tagged_element_result(key, &me, &r)))
            }
            RecursivePoint { .. } => self.unfold().one(rule),
        }
    }

    /// `type.everywhere(rule, optimizationRule, recurse, checkIndex)`.
    pub fn everywhere(&self, rule: &TypeRewriteRule, opt: OptimizationRule, recurse: bool, check_index: bool) -> Option<RewriteResult> {
        match &self.0.kind {
            TypeKind::Check { index, expected_index, .. } if check_index && index != expected_index => None,
            TypeKind::RecursivePoint { family, index, .. } => {
                if recurse && let Some(r) = family.everywhere(*index, rule, opt) {
                    return Some(r);
                }
                Some(RewriteResult::nop(self))
            }
            _ => {
                let rule2 = TypeRewriteRule::everywhere_step(rule, opt, recurse, check_index);
                self.rewrite(&rule2, opt)
            }
        }
    }

    /// `type.ifSame(type2, value)` for rewrite results (compares with `ignoreRecursionPoints = checkIndex = true`).
    pub fn if_same_rewrite(&self, ty: &Type, value: &RewriteResult) -> Option<RewriteResult> {
        if self.equals_with(ty, true, true) { Some(value.clone()) } else { None }
    }

    /// `type.ifSame(typed)`: the value when `typed` has an equal type.
    pub fn if_same(&self, typed: &Typed) -> Option<Value> {
        if self.equals_with(&typed.ty, true, true) { Some(typed.value.clone()) } else { None }
    }

    // ---- per-kind rewrite helpers -------------------------------------------------------------------------------

    pub(crate) fn merge_product(&self, left: RewriteResult, right: RewriteResult) -> RewriteResult {
        let (first, second) = self.as_product().unwrap();
        let v1 = fix_product_left(self, first, second, &left);
        let v2 = fix_product_right(v1.view.new_type(), left.view.new_type(), second, &right);
        v2.compose(&v1)
    }

    pub(crate) fn merge_sum(&self, left: RewriteResult, right: RewriteResult) -> RewriteResult {
        let (first, second) = self.as_sum().unwrap();
        let v1 = fix_sum_left(self, first, second, &left);
        let v2 = fix_sum_right(v1.view.new_type(), left.view.new_type(), second, &right);
        v2.compose(&v1)
    }

    pub(crate) fn merge_compound_list(&self, left: RewriteResult, right: RewriteResult) -> RewriteResult {
        let (key, element) = self.as_compound_list().unwrap();
        let v1 = fix_compound_keys(self, key, element, &left);
        let v2 = fix_compound_values(v1.view.new_type(), left.view.new_type(), element, &right);
        v2.compose(&v1)
    }

    pub(crate) fn fix_list(&self, element: &Type, view: RewriteResult) -> RewriteResult {
        let optic = TypedOptic::list(element, view.view.new_type());
        optic_view(self, &view, optic)
    }

    pub(crate) fn wrap_tag(&self, instance: RewriteResult) -> RewriteResult {
        if instance.view.is_nop() {
            return instance;
        }
        let (name, _) = self.as_tag().expect("not a tag type");
        let output = Type::tag(name, instance.view.new_type());
        let optic = TypedOptic::id_of(self.clone(), output, instance.view.ty().clone(), instance.view.new_type().clone());
        optic_view(self, &instance, optic)
    }

    /// `NamedType.fix(type, instance)`.
    pub(crate) fn fix_named(&self, name: &str, instance: RewriteResult) -> RewriteResult {
        if instance.view.is_nop() {
            return RewriteResult::nop(self);
        }
        let inner = TypedOptic::adapter(instance.view.ty(), instance.view.new_type());
        optic_view(self, &instance, wrap_named_optic(name, &inner))
    }

    /// `CheckType.fix(type, instance)`.
    pub(crate) fn fix_check(&self, instance: RewriteResult) -> RewriteResult {
        if instance.view.is_nop() {
            return RewriteResult::nop(self);
        }
        let inner = TypedOptic::adapter(instance.view.ty(), instance.view.new_type());
        optic_view(self, &instance, self.wrap_check_optic(&inner))
    }

    fn wrap_check_optic(&self, optic: &TypedOptic) -> TypedOptic {
        match &self.0.kind {
            TypeKind::Check { name, index, expected_index, .. } => {
                optic.cast_outer(self.clone(), Type::check(name, *index, *expected_index, optic.t_type()))
            }
            _ => panic!("not a check type"),
        }
    }

    /// `HookType.fix(type, instance)`.
    pub(crate) fn fix_hook(&self, instance: RewriteResult) -> RewriteResult {
        if instance.view.is_nop() {
            return RewriteResult::nop(self);
        }
        let TypeKind::Hook { pre, post, .. } = &self.0.kind else { panic!("not a hook type") };
        let inner = TypedOptic::adapter(instance.view.ty(), instance.view.new_type());
        optic_view(self, &instance, wrap_hook_optic(&inner, pre, post))
    }

    fn all_tagged_choice(&self, tc: &TaggedChoiceData, rule: &TypeRewriteRule) -> RewriteResult {
        let me = TaggedChoiceType(self.clone());
        let mut results: Vec<(String, RewriteResult)> = Vec::new();
        for (key, ty) in &tc.types {
            if let Some(r) = rule.rewrite(ty)
                && !r.view.is_nop()
            {
                results.push((key.clone(), r));
            }
        }
        match results.len() {
            0 => RewriteResult::nop(self),
            1 => {
                let (key, r) = &results[0];
                tagged_element_result(key, &me, r)
            }
            _ => {
                // `new Object2ObjectOpenHashMap<>(this.types)`, then `put` replaces values in place
                let mut new_types = fast_order(tc.types.clone());
                let mut rec = crate::rewrite::BitSet::new();
                for (key, r) in &results {
                    if let Some(slot) = new_types.iter_mut().find(|(k, _)| k == key) {
                        slot.1 = r.view.new_type().clone();
                    }
                    rec.or(r.rec_data);
                }
                let new_ty = Type::tagged_choice(&tc.name, &tc.key_type, new_types);
                RewriteResult::create(View::tagged_choice(results, self, &new_ty), rec)
            }
        }
    }

    // ---- finders ------------------------------------------------------------------------------------------------

    /// `type.findType(type, resultType, matcher, recurse)`.
    pub(crate) fn find_type(&self, matcher: &Matcher, recurse: bool) -> Result<TypedOptic, FindError> {
        match matcher.matches(self) {
            Ok(o) => Ok(o),
            Err(FindError::Continue) => self.find_type_in_children(matcher, recurse),
            Err(e) => Err(e),
        }
    }

    fn find_type_in_children(&self, matcher: &Matcher, recurse: bool) -> Result<TypedOptic, FindError> {
        use TypeKind::*;
        match &self.0.kind {
            Product(first, second) => match first.find_type(matcher, recurse) {
                Ok(o) => Ok(TypedOptic::proj1(o.s_type(), second, o.t_type()).compose(&o)),
                Err(_) => second.find_type(matcher, recurse).map(|o| TypedOptic::proj2(first, o.s_type(), o.t_type()).compose(&o)),
            },
            Sum(first, second) => {
                let a = first.find_type(matcher, recurse);
                let b = second.find_type(matcher, recurse);
                match (a, b) {
                    (Ok(l), Ok(r)) => Ok(TypedOptic::sum_merge(&l, &r)),
                    (Ok(o), Err(_)) => Ok(TypedOptic::inj1(o.s_type(), second, o.t_type()).compose(&o)),
                    (Err(_), Ok(o)) => Ok(TypedOptic::inj2(first, o.s_type(), o.t_type()).compose(&o)),
                    (Err(_), Err(e)) => Err(e),
                }
            }
            List(element) => element.find_type(matcher, recurse).map(|o| TypedOptic::list(o.s_type(), o.t_type()).compose(&o)),
            CompoundList(key, element) => match key.find_type(matcher, recurse) {
                Ok(o) => Ok(TypedOptic::compound_list_keys(o.s_type(), o.t_type(), element).compose(&o)),
                Err(_) => element
                    .find_type(matcher, recurse)
                    .map(|o| TypedOptic::compound_list_elements(key, o.s_type(), o.t_type()).compose(&o)),
            },
            Tag(name, element) => element
                .find_type(matcher, recurse)
                .map(|o| o.cast_outer(Type::tag(name, o.s_type()), Type::tag(name, o.t_type()))),
            Named(name, element) => element.find_type(matcher, recurse).map(|o| wrap_named_optic(name, &o)),
            Check { index, expected_index, delegate, .. } => {
                if index != expected_index {
                    Err(FindError::NotFound("Incorrect index in CheckType".into()))
                } else {
                    delegate.find_type(matcher, recurse).map(|o| self.wrap_check_optic(&o))
                }
            }
            Hook { delegate, pre, post } => delegate.find_type(matcher, recurse).map(|o| wrap_hook_optic(&o, pre, post)),
            TaggedChoice(tc) => self.find_in_tagged_choice(tc, matcher, recurse),
            RecursivePoint { family, index, .. } => family.find_type(*index, matcher, recurse).inspect(|o| {
                assert!(o.s_type() == self, "RecursivePointType.findTypeInChildren: optic sType differs from the point type");
            }),
            Primitive(_) | EmptyPart | Passthrough => Err(FindError::NotFound("No more children".into())),
        }
    }

    fn find_in_tagged_choice(&self, tc: &TaggedChoiceData, matcher: &Matcher, recurse: bool) -> Result<TypedOptic, FindError> {
        let optics: Vec<(String, TypedOptic)> = tc
            .types
            .iter()
            .filter_map(|(k, t)| t.find_type(matcher, recurse).ok().map(|o| (k.clone(), o)))
            .collect();
        match optics.len() {
            0 => Err(FindError::NotFound("Not found in any choices".into())),
            1 => {
                let (key, o) = &optics[0];
                Ok(TypedOptic::tagged(&TaggedChoiceType(self.clone()), key, o.s_type(), o.t_type()).compose(o))
            }
            _ => {
                // `new Object2ObjectOpenHashMap<>(this.types)` with the matched choices' new types set in place
                let mut new_types = fast_order(tc.types.clone());
                for (key, o) in &optics {
                    if let Some(slot) = new_types.iter_mut().find(|(k, _)| k == key) {
                        slot.1 = o.t_type().clone();
                    }
                }
                let t = Type::tagged_choice(&tc.name, &tc.key_type, new_types);
                Ok(TypedOptic::choice(self.clone(), t, matcher.ty().clone(), matcher.result().clone(), optics))
            }
        }
    }

    /// `type.findChoiceType(name, index)`.
    pub fn find_choice_type(&self, name: &str, index: i32) -> Option<TaggedChoiceType> {
        use TypeKind::*;
        match &self.0.kind {
            Product(a, b) | Sum(a, b) => a.find_choice_type(name, index).or_else(|| b.find_choice_type(name, index)),
            Named(_, e) => e.find_choice_type(name, index),
            Check { expected_index, delegate, .. } => {
                if index == *expected_index { delegate.find_choice_type(name, index) } else { None }
            }
            Hook { delegate, .. } => delegate.find_choice_type(name, index),
            TaggedChoice(tc) => {
                if tc.name == name { Some(TaggedChoiceType(self.clone())) } else { None }
            }
            RecursivePoint { index: i, .. } => self.unfold().find_choice_type(name, *i),
            _ => None,
        }
    }

    /// `type.findCheckedType(index)`.
    pub fn find_checked_type(&self, index: i32) -> Option<Type> {
        use TypeKind::*;
        match &self.0.kind {
            Product(a, b) | Sum(a, b) => a.find_checked_type(index).or_else(|| b.find_checked_type(index)),
            Named(_, e) => e.find_checked_type(index),
            Check { expected_index, delegate, .. } => {
                if index == *expected_index { Some(delegate.clone()) } else { None }
            }
            Hook { delegate, .. } => delegate.find_checked_type(index),
            TaggedChoice(tc) => tc.types.iter().find_map(|(_, t)| t.find_checked_type(index)),
            RecursivePoint { index: i, .. } => self.unfold().find_checked_type(*i),
            _ => None,
        }
    }

    /// `type.findFieldTypeOpt(name)`.
    pub fn find_field_type_opt(&self, name: &str) -> Option<Type> {
        use TypeKind::*;
        match &self.0.kind {
            Product(a, b) | Sum(a, b) => a.find_field_type_opt(name).or_else(|| b.find_field_type_opt(name)),
            Tag(n, e) => if n == name { Some(e.clone()) } else { None },
            Named(_, e) => e.find_field_type_opt(name),
            Check { index, expected_index, delegate, .. } => {
                if index == expected_index { delegate.find_field_type_opt(name) } else { None }
            }
            Hook { delegate, .. } => delegate.find_field_type_opt(name),
            TaggedChoice(tc) => tc.types.iter().find_map(|(_, t)| t.find_field_type_opt(name)),
            RecursivePoint { .. } => self.unfold().find_field_type_opt(name),
            _ => None,
        }
    }

    /// `type.findFieldType(name)`; panics like Java's `IllegalArgumentException("Field not found: name")`.
    #[track_caller]
    pub fn find_field_type(&self, name: &str) -> Type {
        self.find_field_type_opt(name).unwrap_or_else(|| panic!("Field not found: {name}"))
    }

    /// `type.findField(name)`.
    #[track_caller]
    pub fn find_field(&self, name: &str) -> crate::finder::OpticFinder {
        crate::finder::OpticFinder::field(Some(name), &self.find_field_type(name))
    }

    /// `type.finder()` (`DSL.typeFinder(type)`).
    pub fn finder(&self) -> crate::finder::OpticFinder {
        crate::finder::OpticFinder::field(None, self)
    }

    /// `type.getSetType(optic, newType)`.
    pub fn get_set_type(&self, optic: &crate::finder::OpticFinder, new_type: &Type) -> Type {
        optic.find_type(self, new_type, false).unwrap_or_else(|e| panic!("{e}")).t_type().clone()
    }

    /// `type.point(ops)`: the default value, when one exists.
    pub fn point(&self, ops: &Ops) -> Option<Value> {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(_) => None,
            EmptyPart => Some(Value::Unit),
            Passthrough => Some(Value::Dynamic(Dynamic::new(*ops, Dyn::Empty))),
            Product(a, b) => a.point(ops).and_then(|x| b.point(ops).map(|y| Value::pair(x, y))),
            Sum(a, b) => b.point(ops).map(Value::right).or_else(|| a.point(ops).map(Value::left)),
            List(_) | CompoundList(..) => Some(Value::List(Vec::new())),
            Tag(_, e) => e.point(ops),
            Named(n, e) => e.point(ops).map(|v| Value::named(n.clone(), v)),
            Check { index, expected_index, delegate, .. } => if index == expected_index { delegate.point(ops) } else { None },
            Hook { delegate, .. } => delegate.point(ops),
            TaggedChoice(tc) => tc.types.iter().find_map(|(k, t)| t.point(ops).map(|v| Value::named(k.clone(), v))),
            RecursivePoint { .. } => self.unfold().point(ops),
        }
    }

    /// `type.pointTyped(ops)`.
    pub fn point_typed(&self, ops: &Ops) -> Option<Typed> {
        self.point(ops).map(|v| Typed::new(self.clone(), *ops, v))
    }
}

impl PartialEq for Type {
    fn eq(&self, o: &Type) -> bool {
        self.equals_with(o, false, true)
    }
}
impl Eq for Type {}
impl Hash for Type {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.hash64().hash(h)
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TypeKind::*;
        match &self.0.kind {
            Primitive(p) => f.write_str(p.name()),
            EmptyPart => f.write_str("EmptyPart"),
            Passthrough => f.write_str("EmptyPartPassthrough"),
            Product(a, b) => write!(f, "({a}, {b})"),
            Sum(a, b) => write!(f, "({a} | {b})"),
            List(a) => write!(f, "List[{a}]"),
            CompoundList(k, v) => write!(f, "CompoundList[{k} -> {v}]"),
            Tag(n, e) => write!(f, "Tag[\"{n}\", {e}]"),
            Named(n, e) => write!(f, "NamedType[\"{n}\", {e}]"),
            Check { name, index, expected_index, delegate } => write!(f, "TypeTag[{index}~{expected_index}][{name}: {delegate}]"),
            Hook { delegate, .. } => write!(f, "HookType[{delegate}]"),
            TaggedChoice(tc) => {
                write!(f, "TaggedChoiceType[{}, ", tc.name)?;
                for (i, (k, t)) in tc.types.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", \n")?;
                    }
                    write!(f, "{k} -> {t}")?;
                }
                f.write_str("]\n")
            }
            RecursivePoint { family, index, .. } => write!(f, "MuType[{}_{}]", family.name(), index),
        }
    }
}

impl std::fmt::Debug for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

type RewriteKey = (Type, TypeRewriteRule, OptimizationRule);
static REWRITE_CACHE: LazyLock<Mutex<HashMap<RewriteKey, Option<RewriteResult>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));


// ---- free helpers shared by the per-kind rewrite code ------------------------------------------------------------

fn fix_product_left(ty: &Type, first: &Type, second: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::proj1(first, second, view.view.new_type()))
}
fn fix_product_right(ty: &Type, first: &Type, second: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::proj2(first, second, view.view.new_type()))
}
fn fix_sum_left(ty: &Type, first: &Type, second: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::inj1(first, second, view.view.new_type()))
}
fn fix_sum_right(ty: &Type, first: &Type, second: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::inj2(first, second, view.view.new_type()))
}
fn fix_compound_keys(ty: &Type, key: &Type, element: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::compound_list_keys(key, view.view.new_type(), element))
}
fn fix_compound_values(ty: &Type, key: &Type, element: &Type, view: &RewriteResult) -> RewriteResult {
    optic_view(ty, view, TypedOptic::compound_list_elements(key, element, view.view.new_type()))
}

/// `NamedType.wrapOptic(name, optic)`.
fn wrap_named_optic(name: &str, optic: &TypedOptic) -> TypedOptic {
    TypedOptic::proj2_raw(
        Type::named(name, optic.s_type()),
        Type::named(name, optic.t_type()),
        optic.s_type().clone(),
        optic.t_type().clone(),
    )
    .compose(optic)
}

/// `HookType.wrapOptic(optic, preRead, postWrite)`.
fn wrap_hook_optic(optic: &TypedOptic, pre: &HookFunction, post: &HookFunction) -> TypedOptic {
    optic.cast_outer(Type::hook(optic.s_type(), pre, post), Type::hook(optic.t_type(), pre, post))
}

/// `TaggedChoiceType.elementResult(key, type, result)`.
pub(crate) fn tagged_element_result(key: &str, ty: &TaggedChoiceType, result: &RewriteResult) -> RewriteResult {
    let optic = TypedOptic::tagged(ty, key, result.view.ty(), result.view.new_type());
    optic_view(ty.ty(), result, optic)
}


fn decode_list(element: &Type, ops: &Ops, input: &Dyn) -> Decoded {
    let list = match ops.get_list(input) {
        DataResult::Success(l) => l,
        DataResult::Error { message, .. } => return DataResult::error_msg(message, None),
    };
    let mut elements: Vec<Value> = Vec::with_capacity(list.len());
    let mut failed: Vec<Dyn> = Vec::new();
    let mut result: DataResult<()> = DataResult::success(());
    for value in list.iter() {
        let (step, el) = match element.decode(ops, value) {
            DataResult::Success((a, _)) => (DataResult::success(()), Some(a)),
            DataResult::Error { message, partial } => {
                failed.push(value.clone());
                (DataResult::error_msg(message, None), partial.map(|p| p.0))
            }
        };
        if let Some(a) = el {
            elements.push(a);
        }
        result = result.apply2(step, |_, _| ());
    }
    let errors = Rest::Val(ops.create_list(failed));
    match result {
        DataResult::Success(()) => DataResult::success((Value::List(elements), errors)),
        DataResult::Error { message, .. } => DataResult::error_msg(message, Some((Value::List(elements), errors))),
    }
}

fn decode_compound_list(key: &Type, element: &Type, ops: &Ops, input: &Dyn) -> Decoded {
    let entries = match ops.get_map_values(input) {
        DataResult::Success(e) => e,
        DataResult::Error { message, .. } => return DataResult::error_msg(message, None),
    };
    let mut read: Vec<Value> = Vec::new();
    let mut failed: Vec<(Dyn, Dyn)> = Vec::new();
    let mut result: DataResult<()> = DataResult::success(());
    for (k, v) in entries {
        let kr = key.decode(ops, k).map(|(a, _)| a);
        let vr = element.decode(ops, v).map(|(a, _)| a);
        let entry = kr.apply2(vr, |a, b| Value::pair(a, b));
        if entry.is_error() {
            Dyn::map_put(&mut failed, k.clone(), v.clone());
        }
        result = result.apply2(entry, |_, e| read.push(e));
    }
    let errors = Rest::Val(ops.create_map(failed));
    let pair = (Value::List(read), errors);
    match result {
        DataResult::Success(()) => DataResult::success(pair),
        DataResult::Error { message, .. } => DataResult::error_msg(message, Some(pair)),
    }
}

/// `mapBuilder().add(..).build(prefix)` with in-place merging into `prefix` (no copy of large prefixes).
fn put_entries(ops: &Ops, prefix: Dyn, entries: Vec<(Dyn, Dyn)>) -> DataResult<Dyn> {
    match prefix {
        Dyn::Empty => DataResult::success(ops.create_map(entries)),
        Dyn::Map(mut m) => {
            for (k, v) in entries {
                Dyn::map_put(&mut m, k, v);
            }
            DataResult::success(Dyn::Map(m))
        }
        other => {
            let o = other.clone();
            DataResult::error_partial(move || format!("mergeToMap called with not a map: {o}"), other)
        }
    }
}

/// `Codec.PASSTHROUGH.encode(input, ops, prefix)`.
fn encode_passthrough(ops: &Ops, input: &Dynamic, prefix: Dyn) -> DataResult<Dyn> {
    if input.value.is_empty_value() {
        return DataResult::success(prefix);
    }
    let casted = input.convert_value(ops);
    if prefix.is_empty_value() {
        return DataResult::success(casted);
    }
    mc_dfu::PASSTHROUGH.encode(ops, input, prefix)
}

// ---- tagged choice handle -----------------------------------------------------------------------------------------

/// `TaggedChoice.TaggedChoiceType<String>`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TaggedChoiceType(Type);

impl TaggedChoiceType {
    fn data(&self) -> &TaggedChoiceData {
        match &self.0.0.kind {
            TypeKind::TaggedChoice(tc) => tc,
            _ => unreachable!("TaggedChoiceType wraps a tagged choice"),
        }
    }
    pub fn ty(&self) -> &Type {
        &self.0
    }
    /// `getName()`.
    pub fn name(&self) -> &str {
        &self.data().name
    }
    /// `getKeyType()`.
    pub fn key_type(&self) -> &Type {
        &self.data().key_type
    }
    /// `types().get(key)`.
    pub fn get(&self, key: &str) -> Option<&Type> {
        self.data().get(key)
    }
    /// `hasType(key)`.
    pub fn has_type(&self, key: &str) -> bool {
        self.data().get(key).is_some()
    }
    /// `types()` in Java iteration order.
    pub fn types(&self) -> &[(String, Type)] {
        &self.data().types
    }

    /// `TypedOptic.replaceTagged(sType, key, aType, bType)`.
    pub(crate) fn replace_tagged(&self, key: &str, a: &Type, b: &Type) -> Type {
        if a == b {
            return self.0.clone();
        }
        let data = self.data();
        match data.get(key) {
            Some(t) if t == a => {}
            _ => panic!("Focused type doesn't match."),
        }
        // `Maps.newHashMap(sType.types())` (java.util.HashMap iteration order), `put(key, bType)`
        let copy: Vec<(String, Type)> = data.types.clone();
        let mut ordered = mc_core::java_collections::hash_map_ordered(
            copy,
            |e| mc_core::math::java::java_string_hash(&e.0),
            mc_core::java_collections::hash_map_copy_capacity(data.types.len()),
        );
        if let Some(slot) = ordered.iter_mut().find(|(k, _)| k == key) {
            slot.1 = b.clone();
        }
        Type::tagged_choice(&data.name, &data.key_type, ordered)
    }
}

impl std::fmt::Display for TaggedChoiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
