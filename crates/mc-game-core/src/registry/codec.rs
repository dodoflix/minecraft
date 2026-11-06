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
            unbound_values: RefCell::new(Vec::new()),
            unbound_tags: RefCell::new(Vec::new()),
        }
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
                "Failed to get element {id} from registry {registry}"
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
            Lookup::No => Err(format!("Missing tag: '{tag}' in '{registry}'")),
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

/// `parse` + `encodeStart` of a hand-written codec.
pub type CustomCodec =
    std::sync::Arc<dyn Fn(&Value, &RegistryDecodeContext) -> Result<Tag, String> + Send + Sync>;

/// The element codec of an inline holder (`DIRECT_CODEC`): `parse` + `encodeStart`.
pub type DirectCodec = fn(&Value, &RegistryDecodeContext) -> Result<Tag, String>;

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
    /// A list with at least one element (`ExtraCodecs.nonEmptyList`).
    NonEmptyList(Box<Schema>),
    Record(Vec<Field>),
    /// `Codec.simpleMap` / `unboundedMap`: object with codec-validated keys.
    Map(Box<Schema>, Box<Schema>),
    /// First alternative that decodes wins (`Codec.either`, `withAlternative`).
    Either(Vec<Schema>),
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

fn number(v: &Value) -> Result<&serde_json::Number, String> {
    match v {
        Value::Number(n) => Ok(n),
        _ => Err(format!("Not a number: {}", compact(v))),
    }
}

/// `Number.intValue()`.
fn int_value(n: &serde_json::Number) -> i32 {
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
        Value::String(s) => Tag::String(s.clone()),
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
        Schema::Custom(std::sync::Arc::new(f))
    }

    pub fn list_of(item: Schema) -> Schema {
        Schema::List(Box::new(item))
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
            Schema::Byte => Ok(Tag::Byte(int_value(number(v)?) as i8)),
            Schema::Short => Ok(Tag::Short(int_value(number(v)?) as i16)),
            Schema::Int => Ok(Tag::Int(int_value(number(v)?))),
            Schema::Long => Ok(Tag::Long(long_value(number(v)?))),
            Schema::Float => Ok(Tag::Float(double_value(number(v)?) as f32)),
            Schema::Double => Ok(Tag::Double(double_value(number(v)?))),
            Schema::IntRange(min, max) => {
                let n = int_value(number(v)?);
                if n >= *min && n <= *max {
                    Ok(Tag::Int(n))
                } else {
                    Err(range_error(n, *min, *max))
                }
            }
            Schema::FloatRange(min, max) => {
                let f = double_value(number(v)?) as f32;
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
                let d = double_value(number(v)?);
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
                .map(|s| Tag::String(s.to_owned()))
                .ok_or_else(|| format!("Not a string: {}", compact(v))),
            Schema::Identifier => {
                let s = v
                    .as_str()
                    .ok_or_else(|| format!("Not a string: {}", compact(v)))?;
                Ok(Tag::String(Identifier::read(s)?.to_string()))
            }
            Schema::Enum(names) => {
                let s = v
                    .as_str()
                    .ok_or_else(|| format!("Not a string: {}", compact(v)))?;
                if names.contains(&s) {
                    Ok(Tag::String(s.to_owned()))
                } else {
                    Err(format!("Unknown element name:{s}"))
                }
            }
            Schema::StaticRegistry(registry) => {
                let s = v
                    .as_str()
                    .ok_or_else(|| format!("Not a string: {}", compact(v)))?;
                let id = Identifier::read(s)?;
                match ctx.lookup.contains(registry, &id) {
                    Lookup::Yes | Lookup::Pending => Ok(Tag::String(id.to_string())),
                    _ => Err(format!(
                        "Unknown registry key in ResourceKey[minecraft:root / {registry}]: ResourceKey[{registry} / {id}]"
                    )),
                }
            }
            Schema::Holder(registry) => match v {
                Value::String(s) => {
                    let id = Identifier::read(s)?;
                    ctx.check_holder(registry, &id)?;
                    Ok(Tag::String(id.to_string()))
                }
                _ => Err("Inline definitions not allowed here".to_owned()),
            },
            Schema::HolderOrDirect { registry, direct } => match v {
                Value::String(_) => Schema::Holder(registry).convert(v, ctx),
                _ => direct(v, ctx),
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
            Schema::List(item) => convert_list(item, v, ctx, false),
            Schema::NonEmptyList(item) => convert_list(item, v, ctx, true),
            Schema::Record(fields) => convert_record(fields, v, ctx).map(Tag::Compound),
            Schema::Map(key, value) => {
                let o = v
                    .as_object()
                    .ok_or_else(|| format!("Not a map: {}", compact(v)))?;
                let mut out = CompoundTag::new();
                for (k, item) in o {
                    let key_tag = key.convert(&Value::String(k.clone()), ctx)?;
                    let Tag::String(k) = key_tag else {
                        return Err("Map keys must encode to strings".to_owned());
                    };
                    out.put(k, value.convert(item, ctx)?);
                }
                Ok(Tag::Compound(out))
            }
            Schema::Either(alternatives) => {
                let mut errors = Vec::new();
                for alt in alternatives {
                    match alt.convert(v, ctx) {
                        Ok(t) => return Ok(t),
                        Err(e) => errors.push(e),
                    }
                }
                Err(format!(
                    "Failed to parse either. First: {}; Second: {}",
                    errors.first().cloned().unwrap_or_default(),
                    errors.last().cloned().unwrap_or_default()
                ))
            }
            Schema::Dispatch { key, resolve } => {
                let o = v
                    .as_object()
                    .ok_or_else(|| format!("Not a map: {}", compact(v)))?;
                let type_value = o
                    .get(*key)
                    .ok_or_else(|| format!("No key {key} in MapLike[{}]", compact(v)))?;
                let type_str = type_value
                    .as_str()
                    .ok_or_else(|| format!("Not a string: {}", compact(type_value)))?;
                let type_id = Identifier::read(type_str)?;
                let schema = resolve(&type_id.to_string())
                    .ok_or_else(|| format!("Unknown type {type_id}"))?;
                let Tag::Compound(mut body) = schema.convert(v, ctx)? else {
                    return Err("Dispatched codec is not a map".to_owned());
                };
                body.put(key.to_string(), Tag::String(type_id.to_string()));
                Ok(Tag::Compound(body))
            }
            Schema::Text => mc_text::Component::from_json(v)
                .map_err(|e| e.to_string())?
                .to_nbt()
                .map_err(|e| e.to_string()),
            Schema::Style => mc_text::Style::from_json(v)
                .map_err(|e| e.to_string())?
                .to_nbt()
                .map_err(|e| e.to_string()),
            Schema::Raw => Ok(json_to_nbt(v)),
            Schema::Custom(f) => f(v, ctx),
        }
    }
}

fn convert_list(
    item: &Schema,
    v: &Value,
    ctx: &RegistryDecodeContext,
    non_empty: bool,
) -> Result<Tag, String> {
    let list = v
        .as_array()
        .ok_or_else(|| format!("Not a list: {}", compact(v)))?;
    if non_empty && list.is_empty() {
        return Err("List must have contents".to_owned());
    }
    let mut out = ListTag::new();
    for element in list {
        out.push(item.convert(element, ctx)?);
    }
    Ok(Tag::List(out))
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
        if let (Some(direct), false) = (direct, e.is_string()) {
            return direct(e, ctx);
        }
        let s = e
            .as_str()
            .ok_or_else(|| format!("Not a string: {}", compact(e)))?;
        let id = Identifier::read(s)?;
        ctx.check_holder(registry, &id)?;
        Ok(Tag::String(id.to_string()))
    };
    if let Some(s) = v.as_str() {
        if let Some(tag) = s.strip_prefix('#') {
            let id = Identifier::read(tag)?;
            ctx.check_tag(registry, &id)?;
            return Ok(Tag::String(format!("#{id}")));
        }
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

fn convert_record(
    fields: &[Field],
    v: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<CompoundTag, String> {
    let o = v
        .as_object()
        .ok_or_else(|| format!("Not a JSON object: {}", compact(v)))?;
    let mut out = CompoundTag::new();
    let mut errors: Vec<String> = Vec::new();
    for field in fields {
        match (o.get(field.name), &field.kind) {
            (None, FieldKind::Required) => {
                errors.push(format!("No key {} in MapLike[{}]", field.name, compact(v)))
            }
            (None, FieldKind::Optional | FieldKind::Lenient | FieldKind::Default(_)) => {}
            (None, FieldKind::DefaultAlways(d)) => match field.schema.convert(d, ctx) {
                Ok(tag) => {
                    out.put(field.name, tag);
                }
                Err(e) => errors.push(e),
            },
            (Some(value), kind) => match field.schema.convert(value, ctx) {
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
                Err(_) if matches!(kind, FieldKind::Lenient) => {}
                Err(e) => errors.push(format!("{e}")),
            },
        }
    }
    // DFU's applicative `ap` lists the errors of later fields first
    errors.reverse();
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors.join("; "))
    }
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
}
