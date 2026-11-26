//! A small declarative stand-in for the DFU codecs of data-driven registry entries.
//!
//! A [`Schema`] describes a codec (record fields with their numeric types, ranges, enums, registry references, ...).
//! [`Schema::convert`] performs `codec.parse(JsonOps, json)` followed by `codec.encodeStart(NbtOps, value)` in one
//! pass: it validates the JSON like the codec would (vanilla's DFU error texts) and returns the NBT vanilla puts on the wire
//! (booleans as bytes, `Codec.INT` as int, `Codec.FLOAT` as float, defaults omitted, holder sets in their compact form, ...).
//! Typed Rust values are then read from the validated JSON by the registry's decoder.

use std::cell::RefCell;

use mc_core::ident::Identifier;
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_nbt::{CompoundTag, ListTag, Tag};
use serde_json::Value;

/// Answer of a registry lookup during decoding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lookup {
    Yes,
    No,
    /// The registry is being loaded in the same stage: vanilla hands out an unbound holder and fails at `freeze` if it stays unbound.
    Pending,
    /// No such registry in this context.
    NoRegistry,
}

/// What a decoder may ask about other registries (`RegistryOps.RegistryInfoLookup`).
pub trait RegistryLookup {
    fn contains(&self, registry: &str, id: &Identifier) -> Lookup;
    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup;
}

/// `RegistryOps` view handed to decoders: lookups plus the references that stayed unresolved (checked at registry freeze).
pub struct RegistryDecodeContext<'a> {
    pub lookup: &'a dyn RegistryLookup,
    /// The `noise` / `density_function` / `noise_settings` registries of the stage being loaded (`mc_worldgen::data`).
    pub worldgen: Option<&'a mc_worldgen::data::WorldgenRegistries>,
    /// Id of the entry being decoded.
    pub entry: Option<Identifier>,
    /// The JSON was converted from NBT: a number is a valid boolean (`NbtOps.getBooleanValue`).
    pub numeric_booleans: bool,
    /// The JSON converted from NBT and the tag it came from: a failure message prints a value of it as the tag.
    pub typed_source: Option<(&'a Value, &'a Tag)>,
    /// A decode of an entry that was already decoded (for its network form) does not report promoted partial results again.
    quiet: std::cell::Cell<bool>,
    pub(crate) unbound_values: RefCell<Vec<(String, Identifier)>>,
    pub(crate) unbound_tags: RefCell<Vec<(String, Identifier)>>,
}

impl<'a> RegistryDecodeContext<'a> {
    pub fn new(lookup: &'a dyn RegistryLookup) -> RegistryDecodeContext<'a> {
        RegistryDecodeContext {
            lookup,
            worldgen: None,
            entry: None,
            numeric_booleans: false,
            typed_source: None,
            quiet: std::cell::Cell::new(false),
            unbound_values: RefCell::new(Vec::new()),
            unbound_tags: RefCell::new(Vec::new()),
        }
    }

    /// A value as the failure messages print it: `JsonOps` prints JSON, `NbtOps` the tag.
    ///
    /// shortcut: without [`typed_source`](Self::typed_source) the JSON of an NBT value has lost its number types, so a
    /// float or a byte prints as the int or the double it decoded to; only the stems of `world_gen_settings.dat` have a
    /// source.
    pub(crate) fn input_text(&self, v: &Value) -> String {
        fn tag(v: &Value) -> Tag {
            match v {
                Value::Number(n) => match n.as_i64() {
                    Some(i) => i32::try_from(i).map_or(Tag::Long(i), Tag::Int),
                    None => Tag::Double(n.as_f64().unwrap_or(0.0)),
                },
                Value::Array(items) => Tag::List(items.iter().map(tag).collect::<Vec<_>>().into()),
                Value::Object(map) => {
                    let mut out = CompoundTag::new();
                    for (key, value) in map {
                        out.put(key.clone(), tag(value));
                    }
                    Tag::Compound(out)
                }
                other => json_to_nbt(other),
            }
        }
        /// The node of `tag` that `target` (a node of the tree `json`) was converted from.
        fn source<'t>(json: &Value, tag: &'t Tag, target: &Value) -> Option<&'t Tag> {
            if std::ptr::eq(json, target) {
                return Some(tag);
            }
            match (json, tag) {
                (Value::Object(map), Tag::Compound(c)) => map
                    .iter()
                    .find_map(|(key, json)| source(json, c.get(key)?, target)),
                (Value::Array(items), Tag::List(list)) => items
                    .iter()
                    .zip(list.iter())
                    .find_map(|(json, tag)| source(json, tag, target)),
                _ => None,
            }
        }
        if !self.numeric_booleans {
            return compact(v);
        }
        match self
            .typed_source
            .and_then(|(json, root)| source(json, root, v))
        {
            Some(found) => found.to_string(),
            None => tag(v).to_string(),
        }
    }

    /// `MapLike` failure on a value that is no map: `JsonOps` and `NbtOps` word it differently.
    pub(crate) fn not_a_map(&self, v: &Value) -> String {
        let kind = if self.numeric_booleans {
            "map"
        } else {
            "JSON object"
        };
        format!("Not a {kind}: {}", self.input_text(v))
    }

    /// A value of the wrong type: `NbtOps` words the error without the value (`Not a number`; a list keeps it), `JsonOps`
    /// with it (`Not a number: "x"`; `json` is the kind it names).
    pub(crate) fn not_a(&self, nbt: &str, json: &str, v: &Value) -> String {
        if !self.numeric_booleans {
            format!("Not a {json}: {}", compact(v))
        } else if nbt == "list" {
            format!("Not a list: {}", self.input_text(v))
        } else {
            format!("Not a {nbt}")
        }
    }

    /// Runs `f` without reporting promoted partial results (the entry was reported by its first decode).
    pub fn quietly<R>(&self, f: impl FnOnce() -> R) -> R {
        let before = self.quiet.replace(true);
        let result = f();
        self.quiet.set(before);
        result
    }

    /// Whether a promoted partial result is reported now.
    pub(crate) fn is_quiet(&self) -> bool {
        self.quiet.get()
    }

    /// `RegistryFileCodec` reference check: `Ok` if `id` exists in (or will be registered to) `registry`.
    pub fn check_holder(&self, registry: &str, id: &Identifier) -> Result<(), String> {
        match self.lookup.contains(registry, id) {
            Lookup::Yes => Ok(()),
            Lookup::Pending => {
                self.unbound_values
                    .borrow_mut()
                    .push((registry.to_owned(), id.clone()));
                Ok(())
            }
            Lookup::No => Err(format!(
                "Failed to get element {id} from registry {}",
                mc_protocol::registry_context::qualified_registry(registry)
            )),
            Lookup::NoRegistry => Err(format!("Registry does not exist: {registry}")),
        }
    }

    /// `HolderSetCodec.lookupTag`.
    pub fn check_tag(&self, registry: &str, tag: &Identifier) -> Result<(), String> {
        match self.lookup.tag_exists(registry, tag) {
            Lookup::Yes => Ok(()),
            Lookup::Pending => {
                self.unbound_tags
                    .borrow_mut()
                    .push((registry.to_owned(), tag.clone()));
                Ok(())
            }
            Lookup::No => Err(format!(
                "Missing tag: '{tag}' in '{}'",
                mc_protocol::registry_context::qualified_registry(registry)
            )),
            Lookup::NoRegistry => Err(format!(
                "Registry {registry} is not available in this context"
            )),
        }
    }
}

/// How a record field is read.
#[derive(Clone)]
pub enum FieldKind {
    /// `fieldOf`
    Required,
    /// `optionalFieldOf(name)`: absent is fine, present-but-invalid fails.
    Optional,
    /// `lenientOptionalFieldOf(name)`: present-but-invalid is treated as absent.
    Lenient,
    /// `optionalFieldOf(name, default)`: absent decodes to the default (JSON), encoding omits values equal to it.
    Default(Value),
    /// As [`FieldKind::Default`] but the value is always encoded (`ExtraCodecs.optionalAlwaysPresentFieldOf`, or a
    /// required field the data format lets authors omit).
    DefaultAlways(Value),
}

#[derive(Clone)]
pub struct Field {
    pub name: &'static str,
    pub schema: Schema,
    pub kind: FieldKind,
}

pub fn req(name: &'static str, schema: Schema) -> Field {
    Field {
        name,
        schema,
        kind: FieldKind::Required,
    }
}

pub fn opt(name: &'static str, schema: Schema) -> Field {
    Field {
        name,
        schema,
        kind: FieldKind::Optional,
    }
}

pub fn lenient(name: &'static str, schema: Schema) -> Field {
    Field {
        name,
        schema,
        kind: FieldKind::Lenient,
    }
}

pub fn default(name: &'static str, schema: Schema, default: Value) -> Field {
    Field {
        name,
        schema,
        kind: FieldKind::Default(default),
    }
}

pub fn default_always(name: &'static str, schema: Schema, default: Value) -> Field {
    Field {
        name,
        schema,
        kind: FieldKind::DefaultAlways(default),
    }
}

/// A failed `DataResult`: its message and the partial result (`DataResult.Error.partialValue`) the codec still decoded.
/// Partial results decide what list size checks and `validate` steps see after an element failed.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub message: String,
    pub partial: Option<Tag>,
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure {
            message,
            partial: None,
        }
    }
}

/// Marks an `IllegalArgumentException` vanilla throws while decoding (a constructor check) instead of returning a
/// `DataResult` error: it aborts the whole entry (no other message is collected) and names its own class in the load error.
/// The marker is closed so that it can still be found once a codec outside the [`Schema`] engine joined other errors
/// to it.
const THROWN: &str = "\u{0}thrown: ";
const THROWN_END: char = '\u{0}';

/// The error of an `IllegalArgumentException(message)` thrown while decoding.
pub fn illegal_argument(message: &str) -> String {
    format!("{THROWN}{message}{THROWN_END}")
}

/// `(class, message)` of an error that holds one made by [`illegal_argument`]: the exception ends the decoding, so what
/// came before or after it in the message is not part of vanilla's.
pub fn split_thrown(error: &str) -> Option<(&'static str, &str)> {
    let rest = &error[error.find(THROWN)? + THROWN.len()..];
    let message = rest
        .split_once(THROWN_END)
        .map_or(rest, |(message, _)| message);
    Some(("java.lang.IllegalArgumentException", message))
}

impl Failure {
    pub(crate) fn is_thrown(&self) -> bool {
        self.message.starts_with(THROWN)
    }
}

/// `DataResult.flatMap`: `f` also runs on the partial result of a failure; its error is appended and its partial result
/// replaces the failure's, its success keeps the failure with the mapped partial result.
pub fn flat_map<E: Into<Failure>>(
    result: Result<Tag, Failure>,
    f: impl Fn(&Tag) -> Result<Tag, E>,
) -> Result<Tag, Failure> {
    match result {
        Ok(tag) => f(&tag).map_err(Into::into),
        Err(failure) if failure.is_thrown() => Err(failure),
        Err(failure) => match failure.partial.as_ref().map(&f) {
            None => Err(failure),
            Some(Ok(mapped)) => Err(Failure {
                message: failure.message,
                partial: Some(mapped),
            }),
            Some(Err(error)) => {
                let error: Failure = error.into();
                if error.is_thrown() {
                    Err(error)
                } else {
                    Err(Failure {
                        message: format!("{}; {}", failure.message, error.message),
                        partial: error.partial,
                    })
                }
            }
        },
    }
}

/// `Codec.validate`: [`flat_map`] with a check that keeps the value.
pub fn validate(
    result: Result<Tag, Failure>,
    check: impl Fn(&Tag) -> Result<(), String>,
) -> Result<Tag, Failure> {
    flat_map(result, |tag| check(tag).map(|()| tag.clone()))
}

/// `Codec.xor` over two decoded alternatives. Exactly one may succeed; when both do, the result is an error that keeps
/// the first value and `both` prints the two (vanilla prints `Pair.toString`s). When both fail the errors are
/// listed second alternative first, and a partial result survives only if both alternatives have one (`apply2`).
pub fn xor(
    first: Result<Tag, Failure>,
    second: Result<Tag, Failure>,
    both: impl FnOnce(&Tag, &Tag) -> String,
) -> Result<Tag, Failure> {
    match (first, second) {
        (Err(thrown), _) | (_, Err(thrown)) if thrown.is_thrown() => Err(thrown),
        (Ok(first), Ok(second)) => Err(Failure {
            message: both(&first, &second),
            partial: Some(first),
        }),
        (Ok(tag), Err(_)) | (Err(_), Ok(tag)) => Ok(tag),
        (Err(first), Err(second)) => Err(Failure {
            message: format!("{}; {}", second.message, first.message),
            partial: first.partial.and(second.partial),
        }),
    }
}

const FIXED_SIZE_2: &str = "Input is not a list of 2 elements";

/// `ExtraCodecs.intervalCodec(point, lower, upper, make, ..)`: a bare point, a `[lower, upper]` list or a
/// `{lower, upper}` object. `make` builds the interval from two decoded points (`InclusiveRange.create`) and returns what
/// the codec encodes it back to: the single point when both bounds are equal, else the two-element list.
pub fn interval_codec(
    point: Schema,
    lower: &'static str,
    upper: &'static str,
    make: impl Fn(&Tag, &Tag) -> Result<Tag, String> + Clone + Send + Sync + 'static,
) -> Schema {
    let from_point = {
        let (point, make) = (point.clone(), make.clone());
        Schema::custom_partial(move |v, ctx| flat_map(point.decode(v, ctx), |tag| make(tag, tag)))
    };
    let from_list = {
        let (point, make) = (point.clone(), make.clone());
        Schema::custom_partial(move |v, ctx| {
            flat_map(
                Schema::list_of(point.clone()).decode(v, ctx),
                |list| match list {
                    Tag::List(list) if list.len() == 2 => {
                        make(&list[0], &list[1]).map_err(Failure::from)
                    }
                    // `Util.fixedSize` keeps the first two elements of a longer list as the partial result
                    Tag::List(list) if list.len() > 2 => Err(match make(&list[0], &list[1]) {
                        Ok(partial) => Failure {
                            message: FIXED_SIZE_2.to_owned(),
                            partial: Some(partial),
                        },
                        Err(error) => Failure::from(format!("{FIXED_SIZE_2}; {error}")),
                    }),
                    _ => Err(Failure::from(FIXED_SIZE_2.to_owned())),
                },
            )
        })
    };
    let from_object = Schema::custom_partial(move |v, ctx| {
        let bounds = Schema::Record(vec![req(lower, point.clone()), req(upper, point.clone())]);
        flat_map(bounds.decode(v, ctx), |record| match record {
            Tag::Compound(c) => match (c.get(lower), c.get(upper)) {
                (Some(min), Some(max)) => make(min, max),
                _ => Err(ctx.not_a_map(v)),
            },
            _ => Err(ctx.not_a_map(v)),
        })
    });
    Schema::either(from_point, Schema::either(from_list, from_object))
}

/// `parse` + `encodeStart` of a hand-written codec.
pub type CustomCodec =
    std::sync::Arc<dyn Fn(&Value, &RegistryDecodeContext) -> Result<Tag, Failure> + Send + Sync>;

/// The element codec of an inline holder (`DIRECT_CODEC`): `parse` + `encodeStart`.
pub type DirectCodec = fn(&Value, &RegistryDecodeContext) -> Result<Tag, Failure>;

/// A codec description, see the module docs.
#[derive(Clone)]
pub enum Schema {
    Bool,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    IntRange(i32, i32),
    FloatRange(f32, f32),
    DoubleRange(f64, f64),
    String,
    Identifier,
    /// `StringRepresentable.fromEnum`: one of the serialized names.
    Enum(&'static [&'static str]),
    /// `BuiltInRegistries.X.byNameCodec()`: the id of an entry of a static registry.
    StaticRegistry(&'static str),
    /// `RegistryFileCodec` reference to an entry of `registry`.
    Holder(&'static str),
    /// `RegistryCodecs.homogeneousList` / `holderSet`; `always_list` is `HolderSetCodec`'s `alwaysUseList`.
    HolderSet {
        registry: &'static str,
        always_list: bool,
    },
    /// `RegistryFileCodec` with inline definitions allowed (`RegistryCodecs.holder(key, DIRECT_CODEC)`): a reference, or
    /// anything else decoded by `direct` (the element codec's `parse` + `encodeStart`).
    HolderOrDirect {
        registry: &'static str,
        direct: DirectCodec,
    },
    /// `RegistryCodecs.holderSet(key, DIRECT_CODEC)`: like [`Schema::HolderSet`] with inline elements.
    HolderSetOrDirect {
        registry: &'static str,
        always_list: bool,
        direct: DirectCodec,
    },
    List(Box<Schema>),
    /// `Codec.listOf(min, max)`: elements past `max` are not decoded, a list that is too long fails with its length alone.
    SizedList(Box<Schema>, usize, usize),
    /// A list with at least one element (`ExtraCodecs.nonEmptyList`).
    NonEmptyList(Box<Schema>),
    Record(Vec<Field>),
    /// `Codec.simpleMap` / `unboundedMap`: object with codec-validated keys.
    Map(Box<Schema>, Box<Schema>),
    /// `Codec.either(first, second)` (also `withAlternative`).
    Either(Box<Schema>, Box<Schema>),
    /// `Codec.dispatch(typeKey, ..)`: `type` (an id) selects the record schema; the `type` key is re-emitted as an identifier.
    Dispatch {
        key: &'static str,
        resolve: fn(&str) -> Option<Schema>,
    },
    /// `ComponentSerialization.CODEC`
    Text,
    /// `Style.Serializer.CODEC`
    Style,
    /// No schema: `JsonOps.convertTo(NbtOps)` (numbers narrowed to the smallest tag).
    Raw,
    /// Arbitrary codec.
    Custom(CustomCodec),
}

fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn number<'v>(v: &'v Value, ctx: &RegistryDecodeContext) -> Result<&'v serde_json::Number, String> {
    match v {
        Value::Number(n) => Ok(n),
        _ => Err(ctx.not_a("number", "number", v)),
    }
}

/// `Number.intValue()`.
pub fn int_value(n: &serde_json::Number) -> i32 {
    n.as_i64()
        .map(|i| i as i32)
        .or_else(|| n.as_f64().map(|f| f as i32))
        .unwrap_or(0)
}

fn long_value(n: &serde_json::Number) -> i64 {
    n.as_i64()
        .or_else(|| n.as_f64().map(|f| f as i64))
        .unwrap_or(0)
}

fn double_value(n: &serde_json::Number) -> f64 {
    n.as_f64().unwrap_or(0.0)
}

/// `JsonOps.convertTo(NbtOps.INSTANCE, json)`.
pub fn json_to_nbt(v: &Value) -> Tag {
    match v {
        Value::Null => Tag::End,
        Value::Bool(b) => Tag::Byte(*b as i8),
        Value::String(s) => Tag::String(s.clone().into()),
        Value::Number(n) => {
            // `BigDecimal.longValueExact()`: integral values (even `1.0`) narrow to the smallest integer tag
            if let Some(l) = n.as_i64() {
                return narrow_long(l);
            }
            let d = n.as_f64().unwrap_or(0.0);
            if d.fract() == 0.0 && d.abs() < 9.2e18 {
                return narrow_long(d as i64);
            }
            let f = d as f32;
            if f as f64 == d {
                Tag::Float(f)
            } else {
                Tag::Double(d)
            }
        }
        Value::Array(list) => {
            let mut out = ListTag::new();
            for item in list {
                out.push(json_to_nbt(item));
            }
            Tag::List(out)
        }
        Value::Object(map) => {
            let mut out = CompoundTag::new();
            for (k, item) in map {
                out.put(k.clone(), json_to_nbt(item));
            }
            Tag::Compound(out)
        }
    }
}

fn narrow_long(l: i64) -> Tag {
    if l as i8 as i64 == l {
        Tag::Byte(l as i8)
    } else if l as i16 as i64 == l {
        Tag::Short(l as i16)
    } else if l as i32 as i64 == l {
        Tag::Int(l as i32)
    } else {
        Tag::Long(l)
    }
}

fn range_error<T: std::fmt::Display>(v: T, min: T, max: T) -> String {
    format!("Value {v} outside of range [{min}:{max}]")
}

impl Schema {
    pub fn custom(
        f: impl Fn(&Value, &RegistryDecodeContext) -> Result<Tag, String> + Send + Sync + 'static,
    ) -> Schema {
        Schema::custom_partial(move |v, ctx| f(v, ctx).map_err(Failure::from))
    }

    /// [`Schema::custom`] for a codec whose failures carry a partial result.
    pub fn custom_partial(
        f: impl Fn(&Value, &RegistryDecodeContext) -> Result<Tag, Failure> + Send + Sync + 'static,
    ) -> Schema {
        Schema::Custom(std::sync::Arc::new(f))
    }

    pub fn sized_list(item: Schema, min: usize, max: usize) -> Schema {
        Schema::SizedList(Box::new(item), min, max)
    }

    pub fn either(first: Schema, second: Schema) -> Schema {
        Schema::Either(Box::new(first), Box::new(second))
    }

    pub fn list_of(item: Schema) -> Schema {
        Schema::List(Box::new(item))
    }

    /// `Codec.dispatch` on a registry of types (`byNameCodec().dispatch("type", ..)`): a `type` that is a valid identifier
    /// but names no entry of the static registry `registry` fails with the registry's message; the rest is
    /// [`Schema::Dispatch`].
    pub fn dispatch(registry: &'static str, resolve: fn(&str) -> Option<Schema>) -> Schema {
        Schema::custom_partial(move |v, ctx| {
            if !v.is_object() {
                return Err(Failure::from(ctx.not_a_map(v)));
            }
            let unknown = v
                .get("type")
                .and_then(Value::as_str)
                .and_then(|t| Identifier::read(t).ok())
                .map(|id| id.to_string())
                .filter(|id| resolve(id).is_none());
            if let Some(id) = unknown {
                return Err(Failure::from(format!(
                    "Unknown registry key in ResourceKey[minecraft:root / minecraft:{registry}]: {id}"
                )));
            }
            Schema::Dispatch {
                key: "type",
                resolve,
            }
            .decode(v, ctx)
        })
    }

    pub fn map_of(key: Schema, value: Schema) -> Schema {
        Schema::Map(Box::new(key), Box::new(value))
    }

    /// `parse` + `encodeStart`: validate `v` and produce the canonical NBT.
    pub fn convert(&self, v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, String> {
        match self {
            Schema::Bool => match v {
                Value::Bool(b) => Ok(Tag::Byte(*b as i8)),
                // `JsonOps` has no numeric booleans; `NbtOps` (stored data converted to JSON) does
                Value::Number(n) if ctx.numeric_booleans => {
                    Ok(Tag::Byte((int_value(n) as i8 != 0) as i8))
                }
                _ => Err(format!("Not a boolean: {}", compact(v))),
            },
            Schema::Byte => Ok(Tag::Byte(int_value(number(v, ctx)?) as i8)),
            Schema::Short => Ok(Tag::Short(int_value(number(v, ctx)?) as i16)),
            Schema::Int => Ok(Tag::Int(int_value(number(v, ctx)?))),
            Schema::Long => Ok(Tag::Long(long_value(number(v, ctx)?))),
            Schema::Float => Ok(Tag::Float(double_value(number(v, ctx)?) as f32)),
            Schema::Double => Ok(Tag::Double(double_value(number(v, ctx)?))),
            Schema::IntRange(min, max) => {
                let n = int_value(number(v, ctx)?);
                if n >= *min && n <= *max {
                    Ok(Tag::Int(n))
                } else {
                    Err(range_error(n, *min, *max))
                }
            }
            Schema::FloatRange(min, max) => {
                let f = double_value(number(v, ctx)?) as f32;
                if f >= *min && f <= *max {
                    Ok(Tag::Float(f))
                } else {
                    Err(format!(
                        "Value {} outside of range [{}:{}]",
                        java_float_to_string(f),
                        java_float_to_string(*min),
                        java_float_to_string(*max)
                    ))
                }
            }
            Schema::DoubleRange(min, max) => {
                let d = double_value(number(v, ctx)?);
                if d >= *min && d <= *max {
                    Ok(Tag::Double(d))
                } else {
                    Err(format!(
                        "Value {} outside of range [{}:{}]",
                        java_double_to_string(d),
                        java_double_to_string(*min),
                        java_double_to_string(*max)
                    ))
                }
            }
            Schema::String => v
                .as_str()
                .map(|s| Tag::String(s.to_owned().into()))
                .ok_or_else(|| ctx.not_a("string", "string", v)),
            Schema::Identifier => {
                let s = v.as_str().ok_or_else(|| ctx.not_a("string", "string", v))?;
                Ok(Tag::String(Identifier::read(s)?.to_string().into()))
            }
            Schema::Enum(names) => {
                let s = v.as_str().ok_or_else(|| ctx.not_a("string", "string", v))?;
                if names.contains(&s) {
                    Ok(Tag::String(s.to_owned().into()))
                } else {
                    Err(format!("Unknown element name:{s}"))
                }
            }
            Schema::StaticRegistry(registry) => {
                let s = v.as_str().ok_or_else(|| ctx.not_a("string", "string", v))?;
                let id = Identifier::read(s)?;
                match ctx.lookup.contains(registry, &id) {
                    Lookup::Yes | Lookup::Pending => Ok(Tag::String(id.to_string().into())),
                    _ => Err(format!(
                        "Unknown registry key in ResourceKey[minecraft:root / {registry}]: ResourceKey[{registry} / {id}]"
                    )),
                }
            }
            Schema::Holder(registry) => match v {
                Value::String(s) => {
                    let id = Identifier::read(s)?;
                    ctx.check_holder(registry, &id)?;
                    Ok(Tag::String(id.to_string().into()))
                }
                _ => Err("Inline definitions not allowed here".to_owned()),
            },
            Schema::HolderSet {
                registry,
                always_list,
            } => convert_holder_set(registry, *always_list, None, v, ctx),
            Schema::HolderSetOrDirect {
                registry,
                always_list,
                direct,
            } => convert_holder_set(registry, *always_list, Some(*direct), v, ctx),
            Schema::List(_)
            | Schema::SizedList(..)
            | Schema::NonEmptyList(_)
            | Schema::Record(_)
            | Schema::Either(..)
            | Schema::Map(..)
            | Schema::HolderOrDirect { .. }
            | Schema::Dispatch { .. }
            | Schema::Custom(_) => self.decode(v, ctx).map_err(|failure| failure.message),
            Schema::Text => mc_text::Component::from_json(&mc_text::json::element_from_value(v))
                .map_err(|e| e.to_string())?
                .to_nbt()
                .map_err(|e| e.to_string()),
            Schema::Style => mc_text::Style::from_json(&mc_text::json::element_from_value(v))
                .map_err(|e| e.to_string())?
                .to_nbt()
                .map_err(|e| e.to_string()),
            Schema::Raw => Ok(json_to_nbt(v)),
        }
    }

    /// [`Schema::convert`] that keeps the partial result of a failure (what the codecs of lists, records and `either`
    /// pass on), see [`Failure`].
    pub fn decode(&self, v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, Failure> {
        match self {
            Schema::List(item) => decode_list(item, v, ctx, 0, usize::MAX),
            Schema::SizedList(item, min, max) => decode_list(item, v, ctx, *min, *max),
            Schema::NonEmptyList(item) => validate(
                decode_list(item, v, ctx, 0, usize::MAX),
                |list| match list {
                    Tag::List(list) if list.is_empty() => Err("List must have contents".to_owned()),
                    _ => Ok(()),
                },
            ),
            Schema::Record(fields) => decode_record(fields, v, ctx).map(Tag::Compound),
            // `RegistryFileCodec.decode`: anything that is not a valid identifier string is an inline definition
            Schema::HolderOrDirect { registry, direct } => match v {
                Value::String(s) if Identifier::read(s).is_ok() => {
                    Schema::Holder(registry).decode(v, ctx)
                }
                _ => direct(v, ctx),
            },
            Schema::Map(key, value) => decode_map(key, value, v, ctx),
            // `KeyDispatchCodec.decode`: the failure of the selected codec keeps its partial result
            Schema::Dispatch { key, resolve } => {
                let o = v.as_object().ok_or_else(|| ctx.not_a_map(v))?;
                let type_value = o
                    .get(*key)
                    .filter(|t| !t.is_null())
                    .ok_or_else(|| format!("No key {key} in MapLike[{}]", ctx.input_text(v)))?;
                let type_str = type_value
                    .as_str()
                    .ok_or_else(|| ctx.not_a("string", "string", type_value))?;
                let type_id = Identifier::read(type_str)?;
                let schema = resolve(&type_id.to_string())
                    .ok_or_else(|| format!("Unknown type {type_id}"))?;
                let with_type = |tag: Tag| match tag {
                    Tag::Compound(mut body) => {
                        body.put(key.to_string(), Tag::String(type_id.to_string().into()));
                        Tag::Compound(body)
                    }
                    other => other,
                };
                schema
                    .decode(v, ctx)
                    .map(with_type)
                    .map_err(|failure| Failure {
                        partial: failure.partial.map(with_type),
                        ..failure
                    })
            }
            // `EitherCodec.decode`: a failed first alternative that still has a partial result is reported alone
            Schema::Either(first, second) => {
                let first = match first.decode(v, ctx) {
                    Ok(tag) => return Ok(tag),
                    Err(failure) if failure.is_thrown() => return Err(failure),
                    Err(failure) => failure,
                };
                let second = match second.decode(v, ctx) {
                    Ok(tag) => return Ok(tag),
                    Err(failure) if failure.is_thrown() => return Err(failure),
                    Err(failure) => failure,
                };
                Err(if first.partial.is_some() {
                    first
                } else if second.partial.is_some() {
                    second
                } else {
                    Failure::from(format!(
                        "Failed to parse either. First: {}; Second: {}",
                        first.message, second.message
                    ))
                })
            }
            Schema::Custom(f) => f(v, ctx),
            _ => self.convert(v, ctx).map_err(Failure::from),
        }
    }
}

/// `ListCodec`: every element is decoded, the errors of all failed elements are collected (later elements first) and the
/// elements that decoded or have a partial result are kept.
fn decode_list(
    item: &Schema,
    v: &Value,
    ctx: &RegistryDecodeContext,
    min: usize,
    max: usize,
) -> Result<Tag, Failure> {
    let list = v
        .as_array()
        .ok_or_else(|| Failure::from(ctx.not_a("list", "json array", v)))?;
    let mut kept = ListTag::new();
    let mut errors: Vec<String> = Vec::new();
    for element in list {
        // `elements.size() >= maxSize`: an element that failed does not use up a slot
        if kept.len() >= max {
            continue;
        }
        match item.decode(element, ctx) {
            Ok(tag) => kept.push(tag),
            Err(failure) if failure.is_thrown() => return Err(failure),
            Err(failure) => {
                errors.push(failure.message);
                if let Some(partial) = failure.partial {
                    kept.push(partial);
                }
            }
        }
    }
    if kept.len() < min {
        return Err(Failure::from(format!(
            "List is too short: {}, expected range [{min}-{max}]",
            kept.len()
        )));
    }
    if list.len() > max {
        errors = vec![format!(
            "List is too long: {}, expected range [{min}-{max}]",
            list.len()
        )];
    }
    if errors.is_empty() {
        return Ok(Tag::List(kept));
    }
    // DFU's applicative `ap` lists the errors of later elements first
    errors.reverse();
    Err(Failure {
        message: errors.join("; "),
        partial: Some(Tag::List(kept)),
    })
}

/// `BaseMapCodec.decode` (`Codec.simpleMap` / `unboundedMap`): every entry is read, the entries whose key and value both
/// have a (partial) result are kept, and the failure lists the errors of later entries first and the raw entries that
/// failed (`missed input`).
///
/// shortcut: a duplicate key is reported with the key's encoded string, which is vanilla's text only for keys that print
/// as their name; no registry-keyed map has duplicates today.
fn decode_map(
    key: &Schema,
    value: &Schema,
    v: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<Tag, Failure> {
    let o = v
        .as_object()
        .ok_or_else(|| Failure::from(ctx.not_a_map(v)))?;
    let mut out = CompoundTag::new();
    let mut errors: Vec<String> = Vec::new();
    let mut failed = serde_json::Map::new();
    for (k, item) in o {
        let key_result = key.decode(&Value::String(k.clone()), ctx);
        let value_result = value.decode(item, ctx);
        for result in [&key_result, &value_result] {
            if let Err(failure) = result
                && failure.is_thrown()
            {
                return Err(failure.clone());
            }
        }
        // `key.apply2stable(Pair::of, value)`: the value's error is listed first
        let error = match (&key_result, &value_result) {
            (Err(key), Err(value)) => Some(format!("{}; {}", value.message, key.message)),
            (Err(failure), _) | (_, Err(failure)) => Some(failure.message.clone()),
            _ => None,
        };
        let result = |r: Result<Tag, Failure>| r.map_or_else(|failure| failure.partial, Some);
        if let (Some(Tag::String(name)), Some(entry)) = (result(key_result), result(value_result)) {
            if out.get(name.as_lossy_str()).is_some() {
                errors.push(format!(
                    "Duplicate entry for key: '{}'",
                    name.as_lossy_str()
                ));
                failed.insert(k.clone(), item.clone());
                continue;
            }
            out.put(name, entry);
        }
        if let Some(error) = error {
            errors.push(error);
            failed.insert(k.clone(), item.clone());
        }
    }
    if errors.is_empty() {
        return Ok(Tag::Compound(out));
    }
    errors.reverse();
    Err(Failure {
        message: format!(
            "{} missed input: {}",
            errors.join("; "),
            compact(&Value::Object(failed))
        ),
        partial: Some(Tag::Compound(out)),
    })
}

/// `HolderSetCodec`: `"#tag"`, a single id (compact form) or a list of ids; direct sets re-encode compactly unless `always_list`.
fn convert_holder_set(
    registry: &str,
    always_list: bool,
    direct: Option<DirectCodec>,
    v: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<Tag, String> {
    let holder = |e: &Value| -> Result<Tag, String> {
        if let (Some(direct), false) = (
            direct,
            e.as_str().is_some_and(|s| Identifier::read(s).is_ok()),
        ) {
            return direct(e, ctx).map_err(|failure| failure.message);
        }
        let s = e.as_str().ok_or_else(|| ctx.not_a("string", "string", e))?;
        let id = Identifier::read(s)?;
        ctx.check_holder(registry, &id)?;
        Ok(Tag::String(id.to_string().into()))
    };
    if let Some(s) = v.as_str()
        && let Some(tag) = s.strip_prefix('#')
    {
        let id = Identifier::read(tag)?;
        ctx.check_tag(registry, &id)?;
        return Ok(Tag::String(format!("#{id}").into()));
    }
    let elements: Vec<Tag> = match v {
        Value::Array(list) => list.iter().map(holder).collect::<Result<_, _>>()?,
        single if !always_list => vec![holder(single)?],
        _ => return Err(format!("Not a list: {}", compact(v))),
    };
    if elements.len() == 1 && !always_list {
        return Ok(elements.into_iter().next().unwrap_or(Tag::End));
    }
    let mut out = ListTag::new();
    for e in elements {
        out.push(e);
    }
    Ok(Tag::List(out))
}

/// `RecordCodecBuilder`: the errors of all fields are collected (later fields first). The partial result exists when every
/// field has one: a `fieldOf` field only passes on its own, an `optionalFieldOf` field always has one (its default).
fn decode_record(
    fields: &[Field],
    v: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<CompoundTag, Failure> {
    let o = v
        .as_object()
        .ok_or_else(|| Failure::from(ctx.not_a_map(v)))?;
    let mut out = CompoundTag::new();
    let mut errors: Vec<String> = Vec::new();
    let mut has_partial = true;
    for field in fields {
        // `JsonOps` hands a JSON `null` to `MapLike.get` as absent
        match (
            o.get(field.name).filter(|value| !value.is_null()),
            &field.kind,
        ) {
            (None, FieldKind::Required) => {
                errors.push(format!(
                    "No key {} in MapLike[{}]",
                    field.name,
                    ctx.input_text(v)
                ));
                has_partial = false;
            }
            (None, FieldKind::Optional | FieldKind::Lenient | FieldKind::Default(_)) => {}
            (None, FieldKind::DefaultAlways(d)) => match field.schema.convert(d, ctx) {
                Ok(tag) => {
                    out.put(field.name, tag);
                }
                Err(e) => {
                    errors.push(e);
                    has_partial = false;
                }
            },
            (Some(value), kind) => match field.schema.decode(value, ctx) {
                Ok(tag) => {
                    let is_default = match kind {
                        FieldKind::Default(d) => {
                            field.schema.convert(d, ctx).is_ok_and(|dt| dt == tag)
                        }
                        _ => false,
                    };
                    if !is_default {
                        out.put(field.name, tag);
                    }
                }
                Err(failure) if failure.is_thrown() => return Err(failure),
                Err(_) if matches!(kind, FieldKind::Lenient) => {}
                Err(failure) => {
                    errors.push(failure.message);
                    match (kind, failure.partial) {
                        (_, Some(partial)) => {
                            out.put(field.name, partial);
                        }
                        (FieldKind::Required, None) => has_partial = false,
                        _ => {}
                    }
                }
            },
        }
    }
    if errors.is_empty() {
        return Ok(out);
    }
    // DFU's applicative `ap` lists the errors of later fields first
    errors.reverse();
    Err(Failure {
        message: errors.join("; "),
        partial: has_partial.then_some(Tag::Compound(out)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Lookups;
    impl RegistryLookup for Lookups {
        fn contains(&self, registry: &str, id: &Identifier) -> Lookup {
            match (registry, id.to_string().as_str()) {
                ("minecraft:block", "minecraft:stone") => Lookup::Yes,
                ("minecraft:worldgen/biome", "foo:later") => Lookup::Pending,
                ("minecraft:block", _) => Lookup::No,
                _ => Lookup::NoRegistry,
            }
        }
        fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup {
            if registry == "minecraft:block" && tag.path() == "mineable/pickaxe" {
                Lookup::Yes
            } else {
                Lookup::No
            }
        }
    }

    fn conv(schema: &Schema, v: Value) -> Result<Tag, String> {
        let l = Lookups;
        schema.convert(&v, &RegistryDecodeContext::new(&l))
    }

    #[test]
    fn scalars_are_typed_like_the_codecs() {
        assert_eq!(conv(&Schema::Bool, json!(true)).unwrap(), Tag::Byte(1));
        assert_eq!(conv(&Schema::Int, json!(3)).unwrap(), Tag::Int(3));
        assert_eq!(conv(&Schema::Float, json!(1)).unwrap(), Tag::Float(1.0));
        assert_eq!(
            conv(&Schema::IntRange(0, 15), json!(16)).unwrap_err(),
            "Value 16 outside of range [0:15]"
        );
        assert_eq!(
            conv(&Schema::FloatRange(0.0, 1.0), json!(2.0)).unwrap_err(),
            "Value 2.0 outside of range [0.0:1.0]"
        );
        assert_eq!(json_to_nbt(&json!(1.0)), Tag::Byte(1));
        assert_eq!(json_to_nbt(&json!(0.5)), Tag::Float(0.5));
        assert_eq!(json_to_nbt(&json!(0.1)), Tag::Double(0.1));
        assert_eq!(json_to_nbt(&json!(70000)), Tag::Int(70000));
    }

    #[test]
    fn records_defaults_and_errors() {
        let s = Schema::Record(vec![
            req("a", Schema::Int),
            default("b", Schema::Bool, json!(false)),
            opt("c", Schema::String),
        ]);
        let Tag::Compound(c) = conv(&s, json!({"a": 1, "b": false})).unwrap() else {
            panic!()
        };
        assert_eq!(c.len(), 1);
        let Tag::Compound(c) = conv(&s, json!({"a": 1, "b": true, "c": "x"})).unwrap() else {
            panic!()
        };
        assert_eq!(c.len(), 3);
        assert!(
            conv(&s, json!({"b": true}))
                .unwrap_err()
                .starts_with("No key a in MapLike[")
        );
    }

    #[test]
    fn holder_sets() {
        let s = Schema::HolderSet {
            registry: "minecraft:block",
            always_list: false,
        };
        assert_eq!(
            conv(&s, json!("#minecraft:mineable/pickaxe")).unwrap(),
            Tag::String("#minecraft:mineable/pickaxe".into())
        );
        assert_eq!(
            conv(&s, json!(["stone"])).unwrap(),
            Tag::String("minecraft:stone".into())
        );
        assert!(
            matches!(conv(&s, json!(["stone", "minecraft:stone"])).unwrap(), Tag::List(l) if l.len() == 2)
        );
        assert_eq!(
            conv(&s, json!("#minecraft:nope")).unwrap_err(),
            "Missing tag: 'minecraft:nope' in 'minecraft:block'"
        );
        assert_eq!(
            conv(&s, json!("dirt")).unwrap_err(),
            "Failed to get element minecraft:dirt from registry minecraft:block"
        );
    }

    #[test]
    fn pending_holders_are_recorded() {
        let l = Lookups;
        let ctx = RegistryDecodeContext::new(&l);
        Schema::Holder("minecraft:worldgen/biome")
            .convert(&json!("foo:later"), &ctx)
            .unwrap();
        assert_eq!(ctx.unbound_values.borrow().len(), 1);
    }

    fn decode(schema: &Schema, v: Value) -> Result<Tag, Failure> {
        let l = Lookups;
        schema.decode(&v, &RegistryDecodeContext::new(&l))
    }

    #[test]
    fn lists_collect_every_error_later_elements_first() {
        let failure = decode(&Schema::list_of(Schema::Int), json!([1, "a", 2, "b"])).unwrap_err();
        assert_eq!(failure.message, "Not a number: \"b\"; Not a number: \"a\"");
        // the elements that decoded are the partial result
        assert_eq!(
            failure.partial,
            Some(Tag::List(vec![Tag::Int(1), Tag::Int(2)].into()))
        );
        assert_eq!(
            decode(&Schema::list_of(Schema::Int), json!(5))
                .unwrap_err()
                .message,
            "Not a json array: 5"
        );
    }

    #[test]
    fn list_sizes_count_the_elements_that_survived() {
        let list = Schema::sized_list(Schema::Int, 1, 2);
        // no element decoded: only the size error remains
        assert_eq!(
            decode(&list, json!(["a"])).unwrap_err().message,
            "List is too short: 0, expected range [1-2]"
        );
        // too long: the length replaces the element errors; a failed element does not use up one of the `max` slots
        let failure = decode(&list, json!([1, "a", 3, 4])).unwrap_err();
        assert_eq!(failure.message, "List is too long: 4, expected range [1-2]");
        assert_eq!(
            failure.partial,
            Some(Tag::List(vec![Tag::Int(1), Tag::Int(3)].into()))
        );
        // an element with a partial result still counts
        let list = Schema::sized_list(
            Schema::Record(vec![default("flags", Schema::Bool, json!(false))]),
            1,
            2,
        );
        assert_eq!(
            decode(&list, json!([{"flags": 3}])).unwrap_err().message,
            "Not a boolean: 3"
        );
    }

    #[test]
    fn validate_runs_on_the_partial_result_and_appends() {
        let non_empty = Schema::NonEmptyList(Box::new(Schema::Int));
        assert_eq!(
            decode(&non_empty, json!([])).unwrap_err(),
            Failure::from("List must have contents".to_owned())
        );
        assert_eq!(
            decode(&non_empty, json!(["a"])).unwrap_err().message,
            "Not a number: \"a\"; List must have contents"
        );
        // a surviving element satisfies the check: the element error stays alone
        assert_eq!(
            decode(&non_empty, json!(["a", 1])).unwrap_err().message,
            "Not a number: \"a\""
        );
    }

    #[test]
    fn either_reports_a_failure_with_a_partial_result_alone() {
        let either = Schema::either(Schema::Bool, Schema::list_of(Schema::Int));
        assert_eq!(
            decode(&either, json!("x")).unwrap_err().message,
            "Failed to parse either. First: Not a boolean: \"x\"; Second: Not a json array: \"x\""
        );
        assert_eq!(
            decode(&either, json!([1, "x"])).unwrap_err().message,
            "Not a number: \"x\""
        );
    }

    #[test]
    fn a_thrown_exception_aborts_everything() {
        let thrown = Schema::custom(|_, _| Err(illegal_argument("boom")));
        let record = Schema::Record(vec![req("a", thrown), req("b", Schema::Int)]);
        let failure = decode(&Schema::list_of(record), json!([{"a": 1}, {"b": "x"}])).unwrap_err();
        assert_eq!(
            split_thrown(&failure.message),
            Some(("java.lang.IllegalArgumentException", "boom"))
        );
    }

    #[test]
    fn a_thrown_exception_is_found_inside_joined_errors() {
        let joined = format!("first problem; {}; last problem", illegal_argument("boom"));
        assert_eq!(
            split_thrown(&joined),
            Some(("java.lang.IllegalArgumentException", "boom"))
        );
        assert_eq!(split_thrown("plain"), None);
    }

    #[test]
    fn xor_keeps_the_partial_result_only_when_both_alternatives_have_one() {
        let ok = |n| Ok(Tag::Int(n));
        let failed = |message: &str, partial: Option<i32>| {
            Err(Failure {
                message: message.to_owned(),
                partial: partial.map(Tag::Int),
            })
        };
        let both = |a: &Tag, b: &Tag| format!("{a:?} {b:?}");
        assert_eq!(xor(ok(1), failed("b", None), both), ok(1));
        assert_eq!(xor(failed("a", None), ok(2), both), ok(2));
        let error = xor(ok(1), ok(2), both).unwrap_err();
        assert_eq!(error.message, "Int(1) Int(2)");
        assert_eq!(error.partial, Some(Tag::Int(1)));
        let error = xor(failed("a", Some(1)), failed("b", Some(2)), both).unwrap_err();
        assert_eq!(
            (error.message.as_str(), error.partial),
            ("b; a", Some(Tag::Int(2)))
        );
        assert_eq!(
            xor(failed("a", Some(1)), failed("b", None), both)
                .unwrap_err()
                .partial,
            None
        );
        assert_eq!(
            xor(failed("a", None), failed("b", Some(2)), both)
                .unwrap_err()
                .partial,
            None
        );
    }

    #[test]
    fn inline_holders_and_dispatch_pass_their_partial_result_on() {
        fn types(name: &str) -> Option<Schema> {
            (name == "minecraft:t").then(|| {
                Schema::Record(vec![
                    req("a", Schema::Int),
                    default("b", Schema::Bool, json!(false)),
                ])
            })
        }
        let holder = Schema::HolderOrDirect {
            registry: "minecraft:block",
            direct: |v, ctx| Schema::dispatch("t", types).decode(v, ctx),
        };
        // the failed optional field leaves a partial record, so the size check of the list around it passes
        let list = Schema::sized_list(holder.clone(), 1, 2);
        let bad = json!({"type": "t", "a": 1, "b": 5});
        let failure = decode(&list, json!([bad.clone()])).unwrap_err();
        assert_eq!(failure.message, "Not a boolean: 5");
        let Some(Tag::List(partial)) = failure.partial else {
            panic!("no partial result")
        };
        assert_eq!(partial.len(), 1);
        // a string that is no identifier is an inline definition, which fails like one
        assert_eq!(
            decode(&holder, json!("Not Valid")).unwrap_err().message,
            "Not a JSON object: \"Not Valid\""
        );
        assert_eq!(
            decode(&holder, json!({"type": "nope"}))
                .unwrap_err()
                .message,
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:t]: minecraft:nope"
        );
    }

    #[test]
    fn maps_collect_errors_and_the_entries_that_failed() {
        let map = Schema::map_of(Schema::Identifier, Schema::Int);
        let failure =
            decode(&map, json!({"a": 1, "b": "x", "Not Valid": 2, "c": "y"})).unwrap_err();
        assert!(
            failure
                .message
                .starts_with("Not a number: \"y\"; Not a valid resource location: Not Valid")
        );
        assert!(
            failure
                .message
                .contains("; Not a number: \"x\" missed input: {")
        );
        assert!(
            failure
                .message
                .ends_with(r#"{"b":"x","Not Valid":2,"c":"y"}"#)
        );
        let Some(Tag::Compound(partial)) = failure.partial else {
            panic!("no partial result")
        };
        assert_eq!(partial.get("minecraft:a"), Some(&Tag::Int(1)));
        assert_eq!(
            decode(&map, json!(5)).unwrap_err().message,
            "Not a JSON object: 5"
        );
    }

    #[test]
    fn intervals_keep_the_first_two_elements_of_a_longer_list() {
        let interval = interval_codec(Schema::Int, "min", "max", |min, max| match (min, max) {
            (Tag::Int(a), Tag::Int(b)) if a > b => Err(format!("inverted {a} {b}")),
            _ => Ok(Tag::List(vec![min.clone(), max.clone()].into())),
        });
        let failure = decode(&interval, json!([1, 2, 3])).unwrap_err();
        assert_eq!(failure.message, "Input is not a list of 2 elements");
        assert_eq!(
            failure.partial,
            Some(Tag::List(vec![Tag::Int(1), Tag::Int(2)].into()))
        );
        // the interval of the kept elements is checked too; without a partial result the `either`s wrap the errors
        assert_eq!(
            decode(&interval, json!([3, 2, 1])).unwrap_err().message,
            "Failed to parse either. First: Not a number: [3,2,1]; Second: Failed to parse either. First: Input is not a list of 2 elements; inverted 3 2; Second: Not a JSON object: [3,2,1]"
        );
    }

    #[test]
    fn a_json_null_field_is_absent() {
        let record = Schema::Record(vec![
            req("a", Schema::Int),
            default("b", Schema::Int, json!(5)),
        ]);
        assert!(
            decode(&record, json!({"a": null, "b": 1}))
                .unwrap_err()
                .message
                .starts_with("No key a in MapLike[")
        );
        assert!(decode(&record, json!({"a": 1, "b": null})).is_ok());
    }
}
