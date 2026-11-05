//! `Value`: the dynamic mirror of the Java runtime representation of a DFU `Typed<A>`'s `A`.
//!
//! | DFU type                      | Java `A`                | `Value`                               |
//! |-------------------------------|-------------------------|---------------------------------------|
//! | `DSL.string()`, `intType()`…  | `String`, `Integer`…    | `Str`, `Int`, …                       |
//! | `DSL.remainderType()`         | `Dynamic<?>`            | `Dynamic`                             |
//! | `and(a, b)` (Product)         | `Pair<A, B>`            | `Pair`                                |
//! | `or(a, b)` (Sum)              | `Either<A, B>`          | `Left` / `Right`                      |
//! | `optional(a)`                 | `Either<A, Unit>`       | `Left(a)` = present, `Right(Unit)`    |
//! | `list(a)`, `compoundList`     | `List<A>`, `List<Pair>` | `List`                                |
//! | `named(n, a)`                 | `Pair<String, A>`       | `Pair(Str(n), a)`                     |
//! | `taggedChoice` (entity/…)     | `Pair<K, ?>`            | `Pair(Str(key), value)`               |
//! | `field(n, a)`, check, hook    | `A`                     | `a` itself                            |
//! | `emptyPart`                   | `Unit`                  | `Unit`                                |

use mc_dfu::{Dyn, Dynamic};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(String),
    /// `Dynamic<?>` (the type of `DSL.remainderType()`): a value together with the ops that built it.
    Dynamic(Dynamic),
    /// `Pair<A, B>` (Product, `named`, tagged choice entries).
    Pair(Box<(Value, Value)>),
    /// `Either.left`; `optional(x)` present.
    Left(Box<Value>),
    /// `Either.right`; `optional(x)` absent is `Right(Unit)`.
    Right(Box<Value>),
    List(Vec<Value>),
}

#[cold]
#[track_caller]
fn mismatch(want: &str, got: &Value) -> ! {
    panic!("expected {want}, got {got:?}")
}

impl Value {
    pub fn pair(a: Value, b: Value) -> Value {
        Value::Pair(Box::new((a, b)))
    }
    pub fn left(a: Value) -> Value {
        Value::Left(Box::new(a))
    }
    pub fn right(b: Value) -> Value {
        Value::Right(Box::new(b))
    }
    /// `Optional.of` encoded for an `optional(..)` type.
    pub fn some(a: Value) -> Value {
        Value::left(a)
    }
    /// `Optional.empty` encoded for an `optional(..)` type.
    pub fn none() -> Value {
        Value::right(Value::Unit)
    }
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }
    /// `Pair.of(name, value)` as stored by `DSL.named(name, ..)` / tagged choices.
    pub fn named(name: impl Into<String>, value: Value) -> Value {
        Value::pair(Value::Str(name.into()), value)
    }
    pub fn dynamic(d: Dynamic) -> Value {
        Value::Dynamic(d)
    }

    #[track_caller]
    pub fn into_pair(self) -> (Value, Value) {
        match self {
            Value::Pair(p) => *p,
            other => mismatch("Pair", &other),
        }
    }
    #[track_caller]
    pub fn as_pair(&self) -> (&Value, &Value) {
        match self {
            Value::Pair(p) => (&p.0, &p.1),
            other => mismatch("Pair", other),
        }
    }
    /// `Pair::getFirst`.
    #[track_caller]
    pub fn first(&self) -> &Value {
        self.as_pair().0
    }
    /// `Pair::getSecond`.
    #[track_caller]
    pub fn second(&self) -> &Value {
        self.as_pair().1
    }
    /// `pair.mapFirst(f)`.
    #[track_caller]
    pub fn map_first(self, f: impl FnOnce(Value) -> Value) -> Value {
        let (a, b) = self.into_pair();
        Value::pair(f(a), b)
    }
    /// `pair.mapSecond(f)`.
    #[track_caller]
    pub fn map_second(self, f: impl FnOnce(Value) -> Value) -> Value {
        let (a, b) = self.into_pair();
        Value::pair(a, f(b))
    }
    /// A `named(..)` / tagged-choice entry as `(name, value)`.
    #[track_caller]
    pub fn into_named(self) -> (String, Value) {
        let (a, b) = self.into_pair();
        (a.into_string(), b)
    }

    /// `optional(x)` value as `Option` (`Left` = present).
    #[track_caller]
    pub fn into_option(self) -> Option<Value> {
        match self {
            Value::Left(a) => Some(*a),
            Value::Right(_) => None,
            other => mismatch("Either", &other),
        }
    }
    #[track_caller]
    pub fn as_option(&self) -> Option<&Value> {
        match self {
            Value::Left(a) => Some(a),
            Value::Right(_) => None,
            other => mismatch("Either", other),
        }
    }
    pub fn from_option(o: Option<Value>) -> Value {
        o.map_or_else(Value::none, Value::some)
    }
    pub fn is_left(&self) -> bool {
        matches!(self, Value::Left(_))
    }

    #[track_caller]
    pub fn as_str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            other => mismatch("String", other),
        }
    }
    #[track_caller]
    pub fn into_string(self) -> String {
        match self {
            Value::Str(s) => s,
            other => mismatch("String", &other),
        }
    }
    #[track_caller]
    pub fn as_int(&self) -> i32 {
        match self {
            Value::Int(v) => *v,
            other => mismatch("Int", other),
        }
    }
    #[track_caller]
    pub fn as_long(&self) -> i64 {
        match self {
            Value::Long(v) => *v,
            other => mismatch("Long", other),
        }
    }
    #[track_caller]
    pub fn as_byte(&self) -> i8 {
        match self {
            Value::Byte(v) => *v,
            other => mismatch("Byte", other),
        }
    }
    #[track_caller]
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(v) => *v,
            other => mismatch("Boolean", other),
        }
    }
    #[track_caller]
    pub fn as_dynamic(&self) -> &Dynamic {
        match self {
            Value::Dynamic(d) => d,
            other => mismatch("Dynamic", other),
        }
    }
    #[track_caller]
    pub fn into_dynamic(self) -> Dynamic {
        match self {
            Value::Dynamic(d) => d,
            other => mismatch("Dynamic", &other),
        }
    }
    #[track_caller]
    pub fn into_list(self) -> Vec<Value> {
        match self {
            Value::List(l) => l,
            other => mismatch("List", &other),
        }
    }
    #[track_caller]
    pub fn as_list(&self) -> &[Value] {
        match self {
            Value::List(l) => l,
            other => mismatch("List", other),
        }
    }
}

impl From<Dynamic> for Value {
    fn from(d: Dynamic) -> Value {
        Value::Dynamic(d)
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_string())
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Value {
        Value::Int(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Value {
        Value::Long(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Value {
        Value::List(v)
    }
}
impl From<Option<Value>> for Value {
    fn from(v: Option<Value>) -> Value {
        Value::from_option(v)
    }
}
impl From<(Value, Value)> for Value {
    fn from(p: (Value, Value)) -> Value {
        Value::pair(p.0, p.1)
    }
}

/// `Dynamic` of the empty value for `ops` (`new Dynamic<>(ops)`, what `Type.point` yields for the remainder).
pub fn empty_dynamic(ops: mc_dfu::Ops) -> Dynamic {
    Dynamic::new(ops, Dyn::Empty)
}
