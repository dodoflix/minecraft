//! The Java `com.mojang.serialization.Dynamic` / `DynamicLike` / `OptionalDynamic` API as inherent methods on
//! [`Dynamic`] and [`OptionalDynamic`], over the existing [`Ops`] (`NbtOps`/`JsonOps`/`HashOps`).
//!
//! # Java -> Rust call mapping (mechanical)
//!
//! | Java | Rust |
//! |---|---|
//! | `d.get("x")` | `d.get("x") -> OptionalDynamic` (`get_ref("x") -> Option<&Dyn>` borrows, no clone) |
//! | `od.get()` (no argument) | `od.get_result() -> &DataResult<Dynamic>` |
//! | `od.result()` / `od.result().isPresent()` | `od.result() -> Option<Dynamic>` (consumes) / `od.is_present()`; `result_ref()` borrows |
//! | `od.map(f)` / `od.flatMap(f)` / `od.into(f)` | `map`/`flat_map`/`map` (closures get `&Dynamic`); `d.into(f)` is `d.apply(f)` |
//! | `asNumber()` `asString()` `asBoolean()` | `as_number() -> DataResult<JNum>`, `as_string() -> DataResult<String>`, `as_boolean() -> DataResult<bool>` |
//! | `asNumber(def)` | `as_number_or(JNum)` |
//! | `asInt(def)` `asLong` `asFloat` `asDouble` `asByte` `asShort` `asBoolean(def)` `asString(def)` | `as_int_or(i32)` `as_long_or` `as_float_or` `as_double_or` `as_byte_or` `as_short_or` `as_boolean_or(bool)` `as_string_or(&str) -> String` |
//! | `asStreamOpt()` / `asStream()` | `as_stream_opt() -> DataResult<Vec<Dynamic>>` / `as_stream() -> Vec<Dynamic>` |
//! | `asMapOpt()` | `as_map_opt() -> DataResult<Vec<(Dynamic, Dynamic)>>`; `getMapValues()` is `get_map_values()` (same shape) |
//! | `asListOpt(f)` / `asList(f)` | `as_list_opt(f) -> DataResult<Vec<U>>` / `as_list(f) -> Vec<U>`; `f: FnMut(Dynamic) -> U` |
//! | `asMapOpt(kf, vf)` / `asMap(kf, vf)` | `as_map_opt_with` / `as_map` returning `Vec<(K, V)>` in insertion order (Java `ImmutableMap`; duplicate keys panic like `build()`) |
//! | `asByteBufferOpt()` `asIntStreamOpt()` `asLongStreamOpt()` and the non-`Opt` forms | `as_byte_buffer_opt() -> DataResult<Vec<i8>>`, `as_int_stream_opt() -> DataResult<Vec<i32>>`, `as_long_stream_opt()`; `as_byte_buffer()` etc. -> `Vec` |
//! | `getGeneric(k)` `getElement(s)` `getElementGeneric(k)` (+ default forms) | `get_generic(&Dyn)` `get_element(&str)` `get_element_generic(&Dyn)` -> `DataResult<Dyn>`; `get_element_or(&str, Dyn)` `get_element_generic_or(&Dyn, Dyn)` |
//! | `createX(..)`, `emptyMap()`, `emptyList()` | `create_byte/short/int/long/float/double/boolean/string/numeric`, `empty_map()`, `empty_list()`; `createList(Stream)` -> `create_list(IntoIterator<Item = Dynamic>)`; `createMap(Map)` -> `create_map(IntoIterator<Item = (Dynamic, Dynamic)>)` (caller supplies the Java map's iteration order, keys unique); `createByteList(ByteBuffer)` / `createIntList(IntStream)` / `createLongList(LongStream)` -> `create_byte_list` / `create_int_list` / `create_long_list(IntoIterator)` |
//! | `d.set(k, v)` `remove` `update(k, f)` `updateGeneric(k, f)` | `set(&str, Dynamic)`, `remove(&str)`, `update(&str, FnOnce(Dynamic) -> Dynamic)`, `update_generic(&Dyn, FnOnce(Dyn) -> Dyn)`; all `&self -> Dynamic` (pure, like Java) |
//! | `setFieldIfPresent` `renameField` `replaceField` `renameAndFixField` | `set_field_if_present(&str, Option<Dynamic>)`, `rename_field`, `replace_field(old, new, Option<Dynamic>)`, `rename_and_fix_field(old, new, FnOnce(Dynamic) -> Dynamic)` |
//! | `Dynamic.copyField(..)` `copyAndFixField(..)` | `Dynamic::copy_field(&src, "a", &dst, "b")`, `Dynamic::copy_and_fix_field(.., fixer)` |
//! | `d.merge(v)` / `d.merge(k, v)` | `merge(&Dynamic)` / `merge_entry(&Dynamic, &Dynamic)` -> `OptionalDynamic` |
//! | `updateMapValues(f)` | `update_map_values(FnMut((Dynamic, Dynamic)) -> (Dynamic, Dynamic))` |
//! | `d.map(f)` (value mapper) | `map_value(FnOnce(&Dyn) -> Dyn)` |
//! | `getValue()` `getOps()` | `value() -> &Dyn` (`into_value()` moves), `ops() -> Ops` |
//! | `castTyped(ops)` / `cast(ops)` | `cast_typed(&Ops) -> &Dynamic` / `cast(&Ops) -> &Dyn`; panic `Dynamic type doesn't match` when the ops differ |
//! | `convert(ops)` | `convert(&Ops) -> Dynamic` (on the raw-value type `Dyn` it is `Dyn::convert_value`) |
//! | `decode(codec)` `read(codec)` `readList(codec)` `readMap(kc, vc)` | `decode -> DataResult<(A, Dyn)>`, `read -> DataResult<A>`, `read_list(&Codec<E>)`, `read_map(&Codec<K>, &Codec<V>)`; the lambda overloads are `read_list_with`, `read_map_with`, `read_map_fold` |
//! | `equals` / `hashCode` / `toString` | `==` (Java semantics: ops equal and values equal, map order ignored, `Float.equals` bit rules) / `hash_code() -> i32` / `Display` = `NBT[{snbt}]`, `JSON[{json}]` |
//!
//! Java methods whose Java signature takes a `Dynamic<?>` take a `Dynamic` by value here (move it or `.clone()`).
//!
//! # Map order
//!
//! NBT-kind `Dyn::Map`s list their entries in `CompoundTag` (`HashMap`) iteration order; all mutating operations
//! reproduce Java's order (see `crate::nbt_order`).
//!
//! shortcut: `Dynamic.hashCode()` includes `ops.hashCode()`, an identity hash that is random per JVM run in Java (as are the
//! hashes of `EndTag`/`JsonNull`); here the constant [`IDENTITY_HASH`] (= what `-XX:hashCode=2` gives, used for the goldens).
//! It only influences tie-breaking order inside [`Dynamic::update_map_values`]' intermediate `HashMap`.

/// Stand-in for Java identity hash codes.
pub const IDENTITY_HASH: i32 = 1;

use crate::codec::{Codec, Dynamic};
use crate::dynv::{Dyn, JNum};
use crate::ops::Ops;
use crate::result::DataResult;
use mc_core::math::java::java_string_hash;
use std::fmt;

// ---- Java `equals` / `hashCode` of the value tree -----------------------------------------------------------------

fn float_bits(v: f32) -> i32 {
    if v.is_nan() {
        0x7fc0_0000
    } else {
        v.to_bits() as i32
    }
}

fn double_bits(v: f64) -> i64 {
    if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits() as i64
    }
}

fn long_hash(v: i64) -> i32 {
    (v ^ ((v as u64) >> 32) as i64) as i32
}

/// `Number` classes `JsonPrimitive` treats as integral.
fn json_integral(d: &Dyn) -> Option<i64> {
    match d {
        Dyn::Byte(v) => Some(*v as i64),
        Dyn::Short(v) => Some(*v as i64),
        Dyn::Int(v) => Some(*v as i64),
        Dyn::Long(v) => Some(*v),
        _ => None,
    }
}

fn json_number(d: &Dyn) -> Option<JNum> {
    Some(match d {
        Dyn::Byte(v) => JNum::Byte(*v),
        Dyn::Short(v) => JNum::Short(*v),
        Dyn::Int(v) => JNum::Int(*v),
        Dyn::Long(v) => JNum::Long(*v),
        Dyn::Float(v) => JNum::Float(*v),
        Dyn::Double(v) => JNum::Double(*v),
        Dyn::JsonNum(t) => JNum::Lazy(t.clone()),
        _ => return None,
    })
}

impl Dyn {
    /// `equals` of the Java value this tree stands for: `Tag.equals` (`json == false`) or `JsonElement.equals`.
    /// Maps compare without regard to entry order; floats compare by `Float.floatToIntBits`.
    pub fn java_eq(&self, other: &Dyn, json: bool) -> bool {
        if json && let (Some(a), Some(b)) = (json_number(self), json_number(other)) {
            return match (json_integral(self), json_integral(other)) {
                (Some(x), Some(y)) => x == y,
                _ => {
                    let (x, y) = (a.double_value(), b.double_value());
                    x == y || (x.is_nan() && y.is_nan())
                }
            };
        }
        match (self, other) {
            (Dyn::Empty, Dyn::Empty) => true,
            (Dyn::Bool(a), Dyn::Bool(b)) => a == b,
            (Dyn::Byte(a), Dyn::Byte(b)) => a == b,
            (Dyn::Short(a), Dyn::Short(b)) => a == b,
            (Dyn::Int(a), Dyn::Int(b)) => a == b,
            (Dyn::Long(a), Dyn::Long(b)) => a == b,
            (Dyn::Float(a), Dyn::Float(b)) => float_bits(*a) == float_bits(*b),
            (Dyn::Double(a), Dyn::Double(b)) => double_bits(*a) == double_bits(*b),
            (Dyn::JsonNum(a), Dyn::JsonNum(b)) => a == b,
            (Dyn::Str(a), Dyn::Str(b)) => a == b,
            (Dyn::List(a), Dyn::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.java_eq(y, json))
            }
            (Dyn::Map(a), Dyn::Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| match k {
                        Dyn::Str(s) => b
                            .iter()
                            .find(|(k2, _)| k2.as_str() == Some(s))
                            .is_some_and(|(_, v2)| v.java_eq(v2, json)),
                        _ => b
                            .iter()
                            .any(|(k2, v2)| k.java_eq(k2, json) && v.java_eq(v2, json)),
                    })
            }
            (Dyn::Bytes(a), Dyn::Bytes(b)) => a == b,
            (Dyn::Ints(a), Dyn::Ints(b)) => a == b,
            (Dyn::Longs(a), Dyn::Longs(b)) => a == b,
            _ => false,
        }
    }

    /// `hashCode()` of the Java value (`Tag`s: records / `Arrays.hashCode` / `HashMap` / `ArrayList`; `JsonElement`s
    /// with `json == true`). Java's `EndTag`/`JsonNull` identity hashes are modelled as [`IDENTITY_HASH`].
    pub fn java_hash(&self, json: bool) -> i32 {
        if json {
            if let Some(v) = json_integral(self) {
                return long_hash(v);
            }
            if let Some(n) = json_number(self) {
                return long_hash(double_bits(n.double_value()));
            }
        }
        match self {
            Dyn::Empty => IDENTITY_HASH,
            Dyn::Bool(true) => 1231,
            Dyn::Bool(false) => 1237,
            Dyn::Byte(v) => *v as i32,
            Dyn::Short(v) => *v as i32,
            Dyn::Int(v) => *v,
            Dyn::Long(v) => long_hash(*v),
            Dyn::Float(v) => float_bits(*v),
            Dyn::Double(v) => long_hash(double_bits(*v)),
            Dyn::JsonNum(t) => java_string_hash(t),
            Dyn::Str(s) => java_string_hash(s),
            Dyn::List(l) => l.iter().fold(1i32, |h, e| {
                h.wrapping_mul(31).wrapping_add(e.java_hash(json))
            }),
            Dyn::Map(m) => m.iter().fold(0i32, |h, (k, v)| {
                h.wrapping_add(k.java_hash(json) ^ v.java_hash(json))
            }),
            Dyn::Bytes(v) => v
                .iter()
                .fold(1i32, |h, e| h.wrapping_mul(31).wrapping_add(*e as i32)),
            Dyn::Ints(v) => v
                .iter()
                .fold(1i32, |h, e| h.wrapping_mul(31).wrapping_add(*e)),
            Dyn::Longs(v) => v
                .iter()
                .fold(1i32, |h, e| h.wrapping_mul(31).wrapping_add(long_hash(*e))),
        }
    }
}

impl PartialEq for Dynamic {
    /// `Dynamic.equals`: same ops and equal values (`Tag.equals` / `JsonElement.equals` semantics).
    fn eq(&self, other: &Dynamic) -> bool {
        self.ops == other.ops && self.value.java_eq(&other.value, self.ops.is_json())
    }
}

impl fmt::Display for Dynamic {
    /// `Dynamic.toString()`: `%s[%s]` of the ops name and the value's `toString()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}]", self.ops.name(), self.ops.show(&self.value))
    }
}

// ---- required members of `DynamicLike` -------------------------------------------------------------------------------

impl Dynamic {
    /// `new Dynamic<>(ops)`: the ops' `empty()` value.
    pub fn empty(ops: Ops) -> Dynamic {
        Dynamic {
            ops,
            value: ops.empty(),
        }
    }
    /// `getOps()`.
    pub fn ops(&self) -> Ops {
        self.ops
    }
    /// `getValue()`.
    pub fn value(&self) -> &Dyn {
        &self.value
    }
    pub fn into_value(self) -> Dyn {
        self.value
    }
    /// `Dynamic.map(function)`: a new `Dynamic` over `function(value)`.
    pub fn map_value(&self, function: impl FnOnce(&Dyn) -> Dyn) -> Dynamic {
        Dynamic {
            ops: self.ops,
            value: function(&self.value),
        }
    }
    /// `castTyped(ops)`; panics with `Dynamic type doesn't match` (Java `IllegalStateException`).
    pub fn cast_typed(&self, ops: &Ops) -> &Dynamic {
        assert!(self.ops == *ops, "Dynamic type doesn't match");
        self
    }
    /// `cast(ops)`.
    pub fn cast(&self, ops: &Ops) -> &Dyn {
        &self.cast_typed(ops).value
    }
    /// `Dynamic.hashCode()` (see the module docs for the `ops.hashCode()` constant).
    pub fn hash_code(&self) -> i32 {
        self.value
            .java_hash(self.ops.is_json())
            .wrapping_mul(31)
            .wrapping_add(IDENTITY_HASH)
    }
    /// `Dynamic.convert(outOps)`.
    pub fn convert(&self, out: &Ops) -> Dynamic {
        Dynamic {
            ops: *out,
            value: self.convert_value(out),
        }
    }
    /// `into(action)` (named `apply`: `into` would clash with `Into::into`).
    pub fn apply<V>(&self, action: impl FnOnce(&Dynamic) -> V) -> V {
        action(self)
    }

    fn wrap(&self, v: &Dyn) -> Dynamic {
        Dynamic {
            ops: self.ops,
            value: v.clone(),
        }
    }

    pub fn as_number(&self) -> DataResult<JNum> {
        self.ops.get_number_value(&self.value)
    }
    pub fn as_string(&self) -> DataResult<String> {
        self.ops.get_string_value(&self.value)
    }
    pub fn as_boolean(&self) -> DataResult<bool> {
        self.ops.get_boolean_value(&self.value)
    }
    pub fn as_stream_opt(&self) -> DataResult<Vec<Dynamic>> {
        self.ops
            .get_stream(&self.value)
            .map(|s| s.iter().map(|e| self.wrap(e)).collect())
    }
    pub fn as_map_opt(&self) -> DataResult<Vec<(Dynamic, Dynamic)>> {
        self.ops.get_map_values(&self.value).map(|m| {
            m.iter()
                .map(|(k, v)| (self.wrap(k), self.wrap(v)))
                .collect()
        })
    }
    pub fn as_byte_buffer_opt(&self) -> DataResult<Vec<i8>> {
        self.ops.get_byte_buffer(&self.value)
    }
    pub fn as_int_stream_opt(&self) -> DataResult<Vec<i32>> {
        self.ops.get_int_stream(&self.value)
    }
    pub fn as_long_stream_opt(&self) -> DataResult<Vec<i64>> {
        self.ops.get_long_stream(&self.value)
    }

    /// `get(key)`: absent keys (and JSON `null` values) give an error `OptionalDynamic` (`key missing: ..`).
    pub fn get(&self, key: &str) -> OptionalDynamic {
        let delegate = self
            .ops
            .get_map(&self.value)
            .flat_map(|m| match m.get(key) {
                Some(v) => DataResult::success(self.wrap(v)),
                None => {
                    let (key, value) = (key.to_string(), self.ops.show(&self.value));
                    DataResult::error(move || format!("key missing: {key} in {value}"))
                }
            });
        OptionalDynamic {
            ops: self.ops,
            delegate,
        }
    }
    /// Borrowing `get(key).result()`: the child value, if the key is present (JSON `null` reads as absent).
    pub fn get_ref(&self, key: &str) -> Option<&Dyn> {
        match &self.value {
            Dyn::Map(m) if !self.ops.is_hash() => {
                let v = m
                    .iter()
                    .find(|(k, _)| k.as_str() == Some(key))
                    .map(|(_, v)| v)?;
                if self.ops.is_json() && v.is_empty_value() {
                    None
                } else {
                    Some(v)
                }
            }
            _ => None,
        }
    }
    /// The string value without cloning (`None` unless this is a string).
    pub fn as_str(&self) -> Option<&str> {
        self.value.as_str()
    }
    /// `getGeneric(key)`.
    pub fn get_generic(&self, key: &Dyn) -> DataResult<Dyn> {
        self.ops.get_generic(&self.value, key)
    }
    /// `getElement(key)`.
    pub fn get_element(&self, key: &str) -> DataResult<Dyn> {
        self.get_element_generic(&self.ops.create_string(key))
    }
    /// `getElementGeneric(key)`.
    pub fn get_element_generic(&self, key: &Dyn) -> DataResult<Dyn> {
        self.ops.get_generic(&self.value, key)
    }
    /// `DynamicLike.decode(decoder)`.
    pub fn decode<A: 'static>(&self, codec: &Codec<A>) -> DataResult<(A, Dyn)> {
        codec.decode(&self.ops, &self.value)
    }
}

// ---- `OptionalDynamic` ------------------------------------------------------------------------------------------------

/// `OptionalDynamic<T>`: the result of `Dynamic.get(key)`; every accessor behaves as on the wrapped `Dynamic` and fails
/// with the lookup error when the key was missing.
#[derive(Clone, Debug)]
pub struct OptionalDynamic {
    ops: Ops,
    delegate: DataResult<Dynamic>,
}

impl OptionalDynamic {
    pub fn new(ops: Ops, delegate: DataResult<Dynamic>) -> Self {
        OptionalDynamic { ops, delegate }
    }
    pub fn ops(&self) -> Ops {
        self.ops
    }
    /// `get()` (no argument): the underlying `DataResult`.
    pub fn get_result(&self) -> &DataResult<Dynamic> {
        &self.delegate
    }
    pub fn into_data_result(self) -> DataResult<Dynamic> {
        self.delegate
    }
    /// `result()`: the `Dynamic` when the lookup succeeded.
    pub fn result(self) -> Option<Dynamic> {
        self.delegate.result()
    }
    /// Borrowing `result()`.
    pub fn result_ref(&self) -> Option<&Dynamic> {
        self.delegate.result_ref()
    }
    /// `result().isPresent()`.
    pub fn is_present(&self) -> bool {
        self.delegate.is_success()
    }
    /// `map(mapper)`.
    pub fn map<U>(&self, mapper: impl FnOnce(&Dynamic) -> U) -> DataResult<U> {
        self.delegate.as_ref().map(mapper)
    }
    /// `flatMap(mapper)`.
    pub fn flat_map<U>(&self, mapper: impl FnOnce(&Dynamic) -> DataResult<U>) -> DataResult<U> {
        self.delegate.as_ref().flat_map(mapper)
    }
    pub fn or_else_empty_map(self) -> Dynamic {
        let ops = self.ops;
        self.result()
            .unwrap_or_else(|| Dynamic::empty(ops).empty_map())
    }
    pub fn or_else_empty_list(self) -> Dynamic {
        let ops = self.ops;
        self.result()
            .unwrap_or_else(|| Dynamic::empty(ops).empty_list())
    }

    pub fn as_number(&self) -> DataResult<JNum> {
        self.flat_map(|d| d.as_number())
    }
    pub fn as_string(&self) -> DataResult<String> {
        self.flat_map(|d| d.as_string())
    }
    pub fn as_boolean(&self) -> DataResult<bool> {
        self.flat_map(|d| d.as_boolean())
    }
    pub fn as_stream_opt(&self) -> DataResult<Vec<Dynamic>> {
        self.flat_map(|d| d.as_stream_opt())
    }
    pub fn as_map_opt(&self) -> DataResult<Vec<(Dynamic, Dynamic)>> {
        self.flat_map(|d| d.as_map_opt())
    }
    pub fn as_byte_buffer_opt(&self) -> DataResult<Vec<i8>> {
        self.flat_map(|d| d.as_byte_buffer_opt())
    }
    pub fn as_int_stream_opt(&self) -> DataResult<Vec<i32>> {
        self.flat_map(|d| d.as_int_stream_opt())
    }
    pub fn as_long_stream_opt(&self) -> DataResult<Vec<i64>> {
        self.flat_map(|d| d.as_long_stream_opt())
    }
    pub fn get(&self, key: &str) -> OptionalDynamic {
        OptionalDynamic {
            ops: self.ops,
            delegate: self.delegate.as_ref().flat_map(|d| d.get(key).delegate),
        }
    }
    /// Borrowing `get(key).result()` (see [`Dynamic::get_ref`]).
    pub fn get_ref(&self, key: &str) -> Option<&Dyn> {
        self.result_ref()?.get_ref(key)
    }
    pub fn get_generic(&self, key: &Dyn) -> DataResult<Dyn> {
        self.flat_map(|d| d.get_generic(key))
    }
    pub fn get_element(&self, key: &str) -> DataResult<Dyn> {
        self.flat_map(|d| d.get_element(key))
    }
    pub fn get_element_generic(&self, key: &Dyn) -> DataResult<Dyn> {
        self.flat_map(|d| d.get_element_generic(key))
    }
    pub fn decode<A: 'static>(&self, codec: &Codec<A>) -> DataResult<(A, Dyn)> {
        self.flat_map(|d| d.decode(codec))
    }
}

// ---- provided members of `DynamicLike`, shared by both types ------------------------------------------------------------

macro_rules! dynamic_like_defaults {
    ($ty:ty) => {
        impl $ty {
            /// `asNumber(default)`.
            pub fn as_number_or(&self, default: JNum) -> JNum {
                self.as_number().result().unwrap_or(default)
            }
            /// `asInt(default)`: `Number.intValue()` of the number, or `default`.
            pub fn as_int_or(&self, default: i32) -> i32 {
                self.as_number().result().map_or(default, |n| n.int_value())
            }
            pub fn as_long_or(&self, default: i64) -> i64 {
                self.as_number()
                    .result()
                    .map_or(default, |n| n.long_value())
            }
            pub fn as_float_or(&self, default: f32) -> f32 {
                self.as_number()
                    .result()
                    .map_or(default, |n| n.float_value())
            }
            pub fn as_double_or(&self, default: f64) -> f64 {
                self.as_number()
                    .result()
                    .map_or(default, |n| n.double_value())
            }
            pub fn as_byte_or(&self, default: i8) -> i8 {
                self.as_number()
                    .result()
                    .map_or(default, |n| n.byte_value())
            }
            pub fn as_short_or(&self, default: i16) -> i16 {
                self.as_number()
                    .result()
                    .map_or(default, |n| n.short_value())
            }
            pub fn as_boolean_or(&self, default: bool) -> bool {
                self.as_boolean().result().unwrap_or(default)
            }
            /// `asString(default)`.
            pub fn as_string_or(&self, default: &str) -> String {
                self.as_string()
                    .result()
                    .unwrap_or_else(|| default.to_string())
            }
            /// `asStream()`.
            pub fn as_stream(&self) -> Vec<Dynamic> {
                self.as_stream_opt().result().unwrap_or_default()
            }
            /// `asByteBuffer()`.
            pub fn as_byte_buffer(&self) -> Vec<i8> {
                self.as_byte_buffer_opt().result().unwrap_or_default()
            }
            pub fn as_int_stream(&self) -> Vec<i32> {
                self.as_int_stream_opt().result().unwrap_or_default()
            }
            pub fn as_long_stream(&self) -> Vec<i64> {
                self.as_long_stream_opt().result().unwrap_or_default()
            }
            /// `asListOpt(deserializer)`.
            pub fn as_list_opt<U>(
                &self,
                deserializer: impl FnMut(Dynamic) -> U,
            ) -> DataResult<Vec<U>> {
                self.as_stream_opt()
                    .map(|s| s.into_iter().map(deserializer).collect())
            }
            /// `asList(deserializer)`.
            pub fn as_list<U>(&self, deserializer: impl FnMut(Dynamic) -> U) -> Vec<U> {
                self.as_list_opt(deserializer).result().unwrap_or_default()
            }
            /// `asMapOpt(keyDeserializer, valueDeserializer)` (Java `ImmutableMap`: duplicate keys panic).
            pub fn as_map_opt_with<K: PartialEq + fmt::Debug, V>(
                &self,
                mut key_deserializer: impl FnMut(Dynamic) -> K,
                mut value_deserializer: impl FnMut(Dynamic) -> V,
            ) -> DataResult<Vec<(K, V)>> {
                self.as_map_opt().map(|entries| {
                    let mut out: Vec<(K, V)> = Vec::with_capacity(entries.len());
                    for (k, v) in entries {
                        let k = key_deserializer(k);
                        assert!(
                            out.iter().all(|(o, _)| *o != k),
                            "Multiple entries with same key: {k:?}"
                        );
                        out.push((k, value_deserializer(v)));
                    }
                    out
                })
            }
            /// `asMap(keyDeserializer, valueDeserializer)`.
            pub fn as_map<K: PartialEq + fmt::Debug, V>(
                &self,
                key_deserializer: impl FnMut(Dynamic) -> K,
                value_deserializer: impl FnMut(Dynamic) -> V,
            ) -> Vec<(K, V)> {
                self.as_map_opt_with(key_deserializer, value_deserializer)
                    .result()
                    .unwrap_or_default()
            }
            /// `getElement(key, default)`.
            pub fn get_element_or(&self, key: &str, default: Dyn) -> Dyn {
                self.get_element(key).result().unwrap_or(default)
            }
            /// `getElementGeneric(key, default)`.
            pub fn get_element_generic_or(&self, key: &Dyn, default: Dyn) -> Dyn {
                self.get_element_generic(key).result().unwrap_or(default)
            }

            /// `read(decoder)`.
            pub fn read<A: 'static>(&self, codec: &Codec<A>) -> DataResult<A> {
                self.decode(codec).map(|(a, _)| a)
            }
            /// `readList(decoder)`: every element decoded; errors accumulate like `ListBox.flip`.
            pub fn read_list<E: 'static>(&self, codec: &Codec<E>) -> DataResult<Vec<E>> {
                self.read_list_with(|d| d.read(codec))
            }
            /// `readList(function)`.
            pub fn read_list_with<E>(
                &self,
                mut decoder: impl FnMut(&Dynamic) -> DataResult<E>,
            ) -> DataResult<Vec<E>> {
                self.as_stream_opt().flat_map(|items| {
                    items
                        .iter()
                        .fold(DataResult::success(Vec::new()), |acc, d| {
                            acc.apply2(decoder(d), |mut v, e| {
                                v.push(e);
                                v
                            })
                        })
                })
            }
            /// `readMap(keyDecoder, valueDecoder)`.
            pub fn read_map<K: 'static, V: 'static>(
                &self,
                key_codec: &Codec<K>,
                value_codec: &Codec<V>,
            ) -> DataResult<Vec<(K, V)>> {
                self.read_map_with(key_codec, |_| value_codec.clone())
            }
            /// `readMap(keyDecoder, Function<K, Decoder<V>>)`.
            pub fn read_map_with<K: 'static, V: 'static>(
                &self,
                key_codec: &Codec<K>,
                value_codec: impl Fn(&K) -> Codec<V>,
            ) -> DataResult<Vec<(K, V)>> {
                self.as_map_opt().flat_map(|entries| {
                    entries
                        .iter()
                        .fold(DataResult::success(Vec::new()), |acc, (k, v)| {
                            let pair = k.read(key_codec).flat_map(|key| {
                                v.read(&value_codec(&key)).map(|value| (key, value))
                            });
                            acc.apply2(pair, |mut l, p| {
                                l.push(p);
                                l
                            })
                        })
                })
            }
            /// `readMap(empty, combiner)`: folds `combiner(acc, key, value)` over the entries while it succeeds.
            pub fn read_map_fold<R>(
                &self,
                empty: DataResult<R>,
                mut combiner: impl FnMut(R, Dynamic, Dynamic) -> DataResult<R>,
            ) -> DataResult<R> {
                self.as_map_opt().flat_map(|entries| {
                    entries
                        .into_iter()
                        .fold(empty, |acc, (k, v)| acc.flat_map(|r| combiner(r, k, v)))
                })
            }

            /// `emptyMap()`.
            pub fn empty_map(&self) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().empty_map(),
                }
            }
            /// `emptyList()`.
            pub fn empty_list(&self) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().empty_list(),
                }
            }
            pub fn create_numeric(&self, n: &JNum) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_numeric(n),
                }
            }
            pub fn create_byte(&self, v: i8) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_byte(v),
                }
            }
            pub fn create_short(&self, v: i16) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_short(v),
                }
            }
            pub fn create_int(&self, v: i32) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_int(v),
                }
            }
            pub fn create_long(&self, v: i64) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_long(v),
                }
            }
            pub fn create_float(&self, v: f32) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_float(v),
                }
            }
            pub fn create_double(&self, v: f64) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_double(v),
                }
            }
            pub fn create_boolean(&self, v: bool) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_boolean(v),
                }
            }
            pub fn create_string(&self, v: impl Into<String>) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_string(v),
                }
            }
            /// `createList(Stream<Dynamic<?>>)`; elements must share this ops.
            pub fn create_list(&self, items: impl IntoIterator<Item = Dynamic>) -> Dynamic {
                let ops = self.ops();
                Dynamic {
                    ops,
                    value: ops
                        .create_list(items.into_iter().map(|d| d.cast(&ops).clone()).collect()),
                }
            }
            /// `createMap(Map<Dynamic<?>, Dynamic<?>>)`: `entries` in the Java map's iteration order, unique keys.
            pub fn create_map(
                &self,
                entries: impl IntoIterator<Item = (Dynamic, Dynamic)>,
            ) -> Dynamic {
                let ops = self.ops();
                Dynamic {
                    ops,
                    value: ops.create_map(
                        entries
                            .into_iter()
                            .map(|(k, v)| (k.cast(&ops).clone(), v.cast(&ops).clone()))
                            .collect(),
                    ),
                }
            }
            /// `createByteList(ByteBuffer)`.
            pub fn create_byte_list(&self, v: impl IntoIterator<Item = i8>) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_byte_list(v.into_iter().collect()),
                }
            }
            /// `createIntList(IntStream)`.
            pub fn create_int_list(&self, v: impl IntoIterator<Item = i32>) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_int_list(v.into_iter().collect()),
                }
            }
            /// `createLongList(LongStream)`.
            pub fn create_long_list(&self, v: impl IntoIterator<Item = i64>) -> Dynamic {
                Dynamic {
                    ops: self.ops(),
                    value: self.ops().create_long_list(v.into_iter().collect()),
                }
            }
        }
    };
}

dynamic_like_defaults!(Dynamic);
dynamic_like_defaults!(OptionalDynamic);

// ---- `Dynamic`-only operations --------------------------------------------------------------------------------------------

impl Dynamic {
    /// `merge(value)`: `mergeToList(this, value)`.
    pub fn merge(&self, value: &Dynamic) -> OptionalDynamic {
        let merged = self
            .ops
            .merge_to_list_one(&self.value, value.cast(&self.ops).clone());
        OptionalDynamic {
            ops: self.ops,
            delegate: merged.map(|m| Dynamic {
                ops: self.ops,
                value: m,
            }),
        }
    }
    /// `merge(key, value)`: `mergeToMap(this, key, value)`.
    pub fn merge_entry(&self, key: &Dynamic, value: &Dynamic) -> OptionalDynamic {
        let merged = self.ops.merge_to_map_one(
            &self.value,
            key.cast(&self.ops).clone(),
            value.cast(&self.ops).clone(),
        );
        OptionalDynamic {
            ops: self.ops,
            delegate: merged.map(|m| Dynamic {
                ops: self.ops,
                value: m,
            }),
        }
    }

    /// `getMapValues()`: the entries in the ops' iteration order.
    pub fn get_map_values(&self) -> DataResult<Vec<(Dynamic, Dynamic)>> {
        self.ops.get_map_values(&self.value).map(|m| {
            m.iter()
                .map(|(k, v)| (self.wrap(k), self.wrap(v)))
                .collect()
        })
    }

    /// `updateMapValues(updater)`: the entries pass through a Java `HashMap<Dynamic, Dynamic>` (`Collectors.toMap`) before
    /// `createMap`, so the result's order follows `Dynamic.hashCode()` buckets. Duplicate resulting keys panic
    /// (`IllegalStateException: Duplicate key ..`). A non-map value is returned unchanged.
    pub fn update_map_values(
        &self,
        mut updater: impl FnMut((Dynamic, Dynamic)) -> (Dynamic, Dynamic),
    ) -> Dynamic {
        let Some(entries) = self.get_map_values().result() else {
            return self.clone();
        };
        let mut pairs: Vec<(Dynamic, Dynamic)> = Vec::with_capacity(entries.len());
        for entry in entries {
            let (k, v) = updater(entry);
            k.cast_typed(&self.ops);
            v.cast_typed(&self.ops);
            if let Some((_, old)) = pairs.iter().find(|(pk, _)| *pk == k) {
                panic!("Duplicate key {k} (attempted merging values {old} and {v})");
            }
            pairs.push((k, v));
        }
        let hashes: Vec<i32> = pairs.iter().map(|(k, _)| k.hash_code()).collect();
        let order = mc_core::java_collections::hash_map_order(&hashes, 16);
        let mut slots: Vec<Option<(Dynamic, Dynamic)>> = pairs.into_iter().map(Some).collect();
        self.create_map(order.into_iter().filter_map(|i| slots[i].take()))
    }

    /// `remove(key)`.
    pub fn remove(&self, key: &str) -> Dynamic {
        Dynamic {
            ops: self.ops,
            value: self.ops.remove(&self.value, key),
        }
    }
    /// `set(key, value)`: `mergeToMap(..).result().orElse(this)`; a non-map target is returned unchanged.
    pub fn set(&self, key: &str, value: Dynamic) -> Dynamic {
        Dynamic {
            ops: self.ops,
            value: self
                .ops
                .set(&self.value, key, value.cast(&self.ops).clone()),
        }
    }
    /// `update(key, function)`: applies `function` to the present value; absent keys leave `self` unchanged.
    pub fn update(&self, key: &str, function: impl FnOnce(Dynamic) -> Dynamic) -> Dynamic {
        let ops = self.ops;
        Dynamic {
            ops,
            value: ops.update(&self.value, key, |v| {
                let out = function(Dynamic { ops, value: v });
                out.cast(&ops).clone()
            }),
        }
    }
    /// `updateGeneric(key, function)`.
    pub fn update_generic(&self, key: &Dyn, function: impl FnOnce(Dyn) -> Dyn) -> Dynamic {
        Dynamic {
            ops: self.ops,
            value: self.ops.update_generic(&self.value, key, function),
        }
    }
    /// `setFieldIfPresent(field, value)`.
    pub fn set_field_if_present(&self, field: &str, value: Option<Dynamic>) -> Dynamic {
        match value {
            None => self.clone(),
            Some(v) => self.set(field, v),
        }
    }
    /// `renameField(old, new)`.
    pub fn rename_field(&self, old_field_name: &str, new_field_name: &str) -> Dynamic {
        self.rename_and_fix_field(old_field_name, new_field_name, |d| d)
    }
    /// `replaceField(old, new, newValue)`.
    pub fn replace_field(
        &self,
        old_field_name: &str,
        new_field_name: &str,
        new_value: Option<Dynamic>,
    ) -> Dynamic {
        self.remove(old_field_name)
            .set_field_if_present(new_field_name, new_value)
    }
    /// `renameAndFixField(old, new, fixer)`.
    pub fn rename_and_fix_field(
        &self,
        old_field_name: &str,
        new_field_name: &str,
        fixer: impl FnOnce(Dynamic) -> Dynamic,
    ) -> Dynamic {
        self.remove(old_field_name)
            .set_field_if_present(new_field_name, self.get(old_field_name).result().map(fixer))
    }

    /// `Dynamic.copyField(source, sourceField, target, targetField)`.
    pub fn copy_field(
        source: &Dynamic,
        source_field_name: &str,
        target: &Dynamic,
        target_field_name: &str,
    ) -> Dynamic {
        Dynamic::copy_and_fix_field(source, source_field_name, target, target_field_name, |d| d)
    }
    /// `Dynamic.copyAndFixField(..)`.
    pub fn copy_and_fix_field(
        source: &Dynamic,
        source_field_name: &str,
        target: &Dynamic,
        target_field_name: &str,
        fixer: impl FnOnce(Dynamic) -> Dynamic,
    ) -> Dynamic {
        match source.get(source_field_name).result() {
            Some(v) => target.set(target_field_name, fixer(v)),
            None => target.clone(),
        }
    }
}
