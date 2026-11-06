//! `DynamicOps` for NBT (`NbtOps`), JSON (`JsonOps`) and hashing (`HashOps`), all over the shared [`Dyn`] tree.

use crate::dynv::{Dyn, JNum};
use crate::result::DataResult;
use mc_core::math::java::{java_double_to_string, java_float_to_string};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpsKind {
    Nbt,
    Json,
    Hash,
}

/// A `DynamicOps` instance (they are stateless singletons in vanilla).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ops {
    kind: OpsKind,
}

/// `NbtOps.INSTANCE`.
pub const NBT: Ops = Ops { kind: OpsKind::Nbt };
/// `JsonOps.INSTANCE`.
pub const JSON: Ops = Ops {
    kind: OpsKind::Json,
};
/// `HashOps.CRC32C_INSTANCE`.
pub const HASH: Ops = Ops {
    kind: OpsKind::Hash,
};

fn unsupported<A>() -> DataResult<A> {
    DataResult::error_str("Unsupported operation")
}

/// `MapLike`: read access to the entries of a map value.
#[derive(Clone, Copy)]
pub struct MapLike<'a> {
    entries: &'a [(Dyn, Dyn)],
    kind: OpsKind,
}

impl<'a> MapLike<'a> {
    /// `MapLike.get(String)`; JSON `null` values read as absent.
    pub fn get(&self, key: &str) -> Option<&'a Dyn> {
        let v = self
            .entries
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)?;
        if self.kind == OpsKind::Json && v.is_empty_value() {
            None
        } else {
            Some(v)
        }
    }
    pub fn entries(&self) -> &'a [(Dyn, Dyn)] {
        self.entries
    }
    pub fn kind(&self) -> OpsKind {
        self.kind
    }
}

impl std::fmt::Display for MapLike<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `NbtOps`' map is a `CompoundTag`: its `toString()` is SNBT
        if self.kind == OpsKind::Nbt
            && let Ok(tag) = Dyn::Map(self.entries.to_vec()).to_tag()
        {
            return write!(f, "MapLike[{tag}]");
        }
        write!(f, "MapLike[{}]", Dyn::Map(self.entries.to_vec()))
    }
}

impl Ops {
    pub fn kind(&self) -> OpsKind {
        self.kind
    }
    pub fn is_nbt(&self) -> bool {
        self.kind == OpsKind::Nbt
    }
    pub fn is_json(&self) -> bool {
        self.kind == OpsKind::Json
    }
    pub fn is_hash(&self) -> bool {
        self.kind == OpsKind::Hash
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            OpsKind::Nbt => "NBT",
            OpsKind::Json => "JSON",
            OpsKind::Hash => "Hash",
        }
    }

    // ---- creation ------------------------------------------------------------------------------------------
    pub fn empty(&self) -> Dyn {
        Dyn::Empty
    }
    pub fn empty_map(&self) -> Dyn {
        Dyn::Map(Vec::new())
    }
    pub fn empty_list(&self) -> Dyn {
        Dyn::List(Vec::new())
    }
    pub fn create_byte(&self, v: i8) -> Dyn {
        Dyn::Byte(v)
    }
    pub fn create_short(&self, v: i16) -> Dyn {
        Dyn::Short(v)
    }
    pub fn create_int(&self, v: i32) -> Dyn {
        Dyn::Int(v)
    }
    pub fn create_long(&self, v: i64) -> Dyn {
        Dyn::Long(v)
    }
    /// NBT collapses `-0.0` to `0.0` (`FloatTag.valueOf`).
    pub fn create_float(&self, v: f32) -> Dyn {
        Dyn::Float(if self.is_nbt() && v == 0.0 { 0.0 } else { v })
    }
    pub fn create_double(&self, v: f64) -> Dyn {
        Dyn::Double(if self.is_nbt() && v == 0.0 { 0.0 } else { v })
    }
    /// `createNumeric(Number)`: NBT makes a double tag, JSON keeps the number, hash dispatches on its type.
    pub fn create_numeric(&self, n: &JNum) -> Dyn {
        match self.kind {
            OpsKind::Nbt => self.create_double(n.double_value()),
            OpsKind::Json => n.to_dyn(),
            OpsKind::Hash => match n {
                JNum::Lazy(_) => Dyn::Double(n.double_value()),
                other => other.to_dyn(),
            },
        }
    }
    pub fn create_boolean(&self, v: bool) -> Dyn {
        if self.is_nbt() {
            Dyn::Byte(v as i8)
        } else {
            Dyn::Bool(v)
        }
    }
    pub fn create_string(&self, s: impl Into<String>) -> Dyn {
        Dyn::Str(s.into())
    }
    pub fn create_list(&self, items: Vec<Dyn>) -> Dyn {
        Dyn::List(items)
    }
    /// `createMap(Stream<Pair>)`: NBT requires string keys (the Java version throws otherwise).
    pub fn create_map(&self, entries: Vec<(Dyn, Dyn)>) -> Dyn {
        if self.is_hash() {
            return Dyn::Map(entries);
        }
        for (k, _) in &entries {
            assert!(
                matches!(k, Dyn::Str(_)),
                "Cannot create map with non-string key: {k}"
            );
        }
        if self.is_nbt() {
            return Dyn::Map(crate::nbt_order::organic(entries));
        }
        let mut out: Vec<(Dyn, Dyn)> = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            Dyn::map_put(&mut out, k, v);
        }
        Dyn::Map(out)
    }
    /// `createByteList`: NBT byte array, otherwise a list of bytes.
    pub fn create_byte_list(&self, v: Vec<i8>) -> Dyn {
        if self.is_json() {
            Dyn::List(v.into_iter().map(Dyn::Byte).collect())
        } else {
            Dyn::Bytes(v)
        }
    }
    pub fn create_int_list(&self, v: Vec<i32>) -> Dyn {
        if self.is_json() {
            Dyn::List(v.into_iter().map(Dyn::Int).collect())
        } else {
            Dyn::Ints(v)
        }
    }
    pub fn create_long_list(&self, v: Vec<i64>) -> Dyn {
        if self.is_json() {
            Dyn::List(v.into_iter().map(Dyn::Long).collect())
        } else {
            Dyn::Longs(v)
        }
    }

    // ---- reading -------------------------------------------------------------------------------------------
    pub fn get_number_value(&self, input: &Dyn) -> DataResult<JNum> {
        if self.is_hash() {
            return unsupported();
        }
        let n = match input {
            Dyn::Byte(v) => JNum::Byte(*v),
            Dyn::Short(v) => JNum::Short(*v),
            Dyn::Int(v) => JNum::Int(*v),
            Dyn::Long(v) => JNum::Long(*v),
            Dyn::Float(v) => JNum::Float(*v),
            Dyn::Double(v) => JNum::Double(*v),
            Dyn::JsonNum(t) if self.is_json() => JNum::Lazy(t.clone()),
            _ => {
                let msg = if self.is_nbt() {
                    "Not a number".to_string()
                } else {
                    format!("Not a number: {}", self.show(input))
                };
                return DataResult::error(move || msg.clone());
            }
        };
        DataResult::success(n)
    }
    pub fn get_boolean_value(&self, input: &Dyn) -> DataResult<bool> {
        match self.kind {
            OpsKind::Nbt => self
                .get_number_value(input)
                .map(|n| n.double_value() != 0.0),
            OpsKind::Json => match input {
                Dyn::Bool(b) => DataResult::success(*b),
                _ => {
                    let i = self.show(input);
                    DataResult::error(move || format!("Not a boolean: {i}"))
                }
            },
            OpsKind::Hash => unsupported(),
        }
    }
    pub fn get_string_value(&self, input: &Dyn) -> DataResult<String> {
        if self.is_hash() {
            return unsupported();
        }
        match input {
            Dyn::Str(s) => DataResult::success(s.clone()),
            _ => {
                if self.is_nbt() {
                    DataResult::error_str("Not a string")
                } else {
                    let i = self.show(input);
                    DataResult::error(move || format!("Not a string: {i}"))
                }
            }
        }
    }

    /// `getStream` / `getList`: the elements of a list (NBT: also typed arrays).
    pub fn get_list<'a>(&self, input: &'a Dyn) -> DataResult<std::borrow::Cow<'a, [Dyn]>> {
        use std::borrow::Cow;
        if self.is_hash() {
            return unsupported();
        }
        match input {
            Dyn::List(l) => DataResult::success(Cow::Borrowed(l.as_slice())),
            Dyn::Bytes(v) if self.is_nbt() => {
                DataResult::success(Cow::Owned(v.iter().map(|x| Dyn::Byte(*x)).collect()))
            }
            Dyn::Ints(v) if self.is_nbt() => {
                DataResult::success(Cow::Owned(v.iter().map(|x| Dyn::Int(*x)).collect()))
            }
            Dyn::Longs(v) if self.is_nbt() => {
                DataResult::success(Cow::Owned(v.iter().map(|x| Dyn::Long(*x)).collect()))
            }
            _ => {
                let i = self.show(input);
                if self.is_nbt() {
                    DataResult::error(move || format!("Not a list: {i}"))
                } else {
                    DataResult::error(move || format!("Not a json array: {i}"))
                }
            }
        }
    }

    /// `getStream`: as [`Self::get_list`] but NBT's error message is the bare `Not a list`.
    pub fn get_stream<'a>(&self, input: &'a Dyn) -> DataResult<std::borrow::Cow<'a, [Dyn]>> {
        match self.get_list(input) {
            DataResult::Error { .. } if self.is_nbt() => DataResult::error_str("Not a list"),
            r => r,
        }
    }

    pub fn get_map<'a>(&self, input: &'a Dyn) -> DataResult<MapLike<'a>> {
        if self.is_hash() {
            return unsupported();
        }
        match input {
            Dyn::Map(m) => DataResult::success(MapLike {
                entries: m,
                kind: self.kind,
            }),
            _ => {
                let i = self.show(input);
                if self.is_nbt() {
                    DataResult::error(move || format!("Not a map: {i}"))
                } else {
                    DataResult::error(move || format!("Not a JSON object: {i}"))
                }
            }
        }
    }

    /// `getMapValues`/`getMapEntries`: entries with JSON `null` values kept as `Empty`.
    pub fn get_map_values<'a>(&self, input: &'a Dyn) -> DataResult<&'a [(Dyn, Dyn)]> {
        self.get_map(input).map(|m| m.entries)
    }

    /// `getByteBuffer`.
    pub fn get_byte_buffer(&self, input: &Dyn) -> DataResult<Vec<i8>> {
        if let (true, Dyn::Bytes(b)) = (self.is_nbt(), input) {
            return DataResult::success(b.clone());
        }
        self.get_stream(input).flat_map(|list| {
            let nums: Vec<DataResult<JNum>> =
                list.iter().map(|e| self.get_number_value(e)).collect();
            if nums.iter().all(|n| n.is_success()) {
                DataResult::success(
                    nums.into_iter()
                        .map(|n| n.result().unwrap().byte_value())
                        .collect(),
                )
            } else {
                let i = self.show(input);
                DataResult::error(move || format!("Some elements are not bytes: {i}"))
            }
        })
    }
    pub fn get_int_stream(&self, input: &Dyn) -> DataResult<Vec<i32>> {
        if let (true, Dyn::Ints(b)) = (self.is_nbt(), input) {
            return DataResult::success(b.clone());
        }
        self.get_stream(input).flat_map(|list| {
            let nums: Vec<DataResult<JNum>> =
                list.iter().map(|e| self.get_number_value(e)).collect();
            if nums.iter().all(|n| n.is_success()) {
                DataResult::success(
                    nums.into_iter()
                        .map(|n| n.result().unwrap().int_value())
                        .collect(),
                )
            } else {
                let i = self.show(input);
                DataResult::error(move || format!("Some elements are not ints: {i}"))
            }
        })
    }
    pub fn get_long_stream(&self, input: &Dyn) -> DataResult<Vec<i64>> {
        if let (true, Dyn::Longs(b)) = (self.is_nbt(), input) {
            return DataResult::success(b.clone());
        }
        self.get_stream(input).flat_map(|list| {
            let nums: Vec<DataResult<JNum>> =
                list.iter().map(|e| self.get_number_value(e)).collect();
            if nums.iter().all(|n| n.is_success()) {
                DataResult::success(
                    nums.into_iter()
                        .map(|n| n.result().unwrap().long_value())
                        .collect(),
                )
            } else {
                let i = self.show(input);
                DataResult::error(move || format!("Some elements are not longs: {i}"))
            }
        })
    }

    // ---- merging -------------------------------------------------------------------------------------------
    /// `mergeToList(prefix, values)`.
    pub fn merge_to_list(&self, prefix: &Dyn, values: Vec<Dyn>) -> DataResult<Dyn> {
        match self.kind {
            OpsKind::Hash => {
                if prefix.is_empty_value() {
                    DataResult::success(Dyn::List(values))
                } else {
                    unsupported()
                }
            }
            OpsKind::Json => match prefix {
                Dyn::List(_) | Dyn::Empty => {
                    if values.is_empty() {
                        return DataResult::success(if prefix.is_empty_value() {
                            Dyn::List(Vec::new())
                        } else {
                            prefix.clone()
                        });
                    }
                    let mut out = match prefix {
                        Dyn::List(l) => l.clone(),
                        _ => Vec::new(),
                    };
                    out.extend(values);
                    DataResult::success(Dyn::List(out))
                }
                _ => {
                    let p = self.show(prefix);
                    DataResult::error_partial(
                        move || format!("mergeToList called with not a list: {p}"),
                        prefix.clone(),
                    )
                }
            },
            OpsKind::Nbt => {
                // createCollector: End / empty collection -> generic list; typed arrays stay typed while elements match
                let mut collector = match prefix {
                    Dyn::Empty => Collector::Generic(Vec::new()),
                    Dyn::List(l) => Collector::Generic(l.clone()),
                    Dyn::Bytes(b) if b.is_empty() => Collector::Generic(Vec::new()),
                    Dyn::Ints(b) if b.is_empty() => Collector::Generic(Vec::new()),
                    Dyn::Longs(b) if b.is_empty() => Collector::Generic(Vec::new()),
                    Dyn::Bytes(b) => Collector::Bytes(b.clone()),
                    Dyn::Ints(b) => Collector::Ints(b.clone()),
                    Dyn::Longs(b) => Collector::Longs(b.clone()),
                    _ => {
                        let p = self.show(prefix);
                        return DataResult::error_partial(
                            move || format!("mergeToList called with not a list: {p}"),
                            prefix.clone(),
                        );
                    }
                };
                for v in values {
                    collector = collector.accept(v);
                }
                DataResult::success(collector.result())
            }
        }
    }

    pub fn merge_to_list_one(&self, prefix: &Dyn, value: Dyn) -> DataResult<Dyn> {
        self.merge_to_list(prefix, vec![value])
    }

    /// `mergeToMap(map, key, value)`.
    pub fn merge_to_map_one(&self, map: &Dyn, key: Dyn, value: Dyn) -> DataResult<Dyn> {
        match self.kind {
            OpsKind::Hash => {
                if map.is_empty_value() {
                    DataResult::success(Dyn::Map(vec![(key, value)]))
                } else {
                    unsupported()
                }
            }
            _ => {
                if !matches!(map, Dyn::Map(_) | Dyn::Empty) {
                    let m = self.show(map);
                    return DataResult::error_partial(
                        move || format!("mergeToMap called with not a map: {m}"),
                        map.clone(),
                    );
                }
                if !matches!(key, Dyn::Str(_)) {
                    let k = self.show(&key);
                    return DataResult::error_partial(
                        move || format!("key is not a string: {k}"),
                        map.clone(),
                    );
                }
                DataResult::success(Dyn::Map(self.put_all(map, [(key, value)])))
            }
        }
    }

    /// `shallowCopy()` + `put`s of `items` (NBT, in `HashMap` iteration order) or a copy + `JsonObject.add`s.
    fn put_all(&self, base: &Dyn, items: impl IntoIterator<Item = (Dyn, Dyn)>) -> Vec<(Dyn, Dyn)> {
        match (self.kind, base) {
            (OpsKind::Nbt, Dyn::Map(m)) => crate::nbt_order::copy_put(m, items),
            (OpsKind::Nbt, _) => crate::nbt_order::organic(items),
            (_, base) => {
                let mut out = match base {
                    Dyn::Map(m) => m.clone(),
                    _ => Vec::new(),
                };
                for (k, v) in items {
                    Dyn::map_put(&mut out, k, v);
                }
                out
            }
        }
    }

    /// `mergeToMap(map, MapLike)`.
    pub fn merge_to_map(&self, map: &Dyn, entries: &[(Dyn, Dyn)]) -> DataResult<Dyn> {
        match self.kind {
            OpsKind::Hash => {
                if map.is_empty_value() {
                    DataResult::success(Dyn::Map(entries.to_vec()))
                } else {
                    unsupported()
                }
            }
            _ => {
                if !matches!(map, Dyn::Map(_) | Dyn::Empty) {
                    let m = self.show(map);
                    return DataResult::error_partial(
                        move || format!("mergeToMap called with not a map: {m}"),
                        map.clone(),
                    );
                }
                if entries.is_empty() {
                    return DataResult::success(if map.is_empty_value() {
                        Dyn::Map(Vec::new())
                    } else {
                        map.clone()
                    });
                }
                let mut missed: Vec<Dyn> = Vec::new();
                let out = self.put_all(
                    map,
                    entries.iter().filter_map(|(k, v)| {
                        if matches!(k, Dyn::Str(_)) {
                            Some((k.clone(), v.clone()))
                        } else {
                            missed.push(k.clone());
                            None
                        }
                    }),
                );
                if missed.is_empty() {
                    DataResult::success(Dyn::Map(out))
                } else {
                    DataResult::error_partial(
                        move || format!("some keys are not strings: {missed:?}"),
                        Dyn::Map(out),
                    )
                }
            }
        }
    }

    /// `mergeToPrimitive`.
    pub fn merge_to_primitive(&self, prefix: &Dyn, value: Dyn) -> DataResult<Dyn> {
        if !prefix.is_empty_value() {
            let (p, v) = (self.show(prefix), self.show(&value));
            DataResult::error_partial(
                move || format!("Do not know how to append a primitive value {v} to {p}"),
                value,
            )
        } else {
            DataResult::success(value)
        }
    }

    /// `remove(input, key)`.
    pub fn remove(&self, input: &Dyn, key: &str) -> Dyn {
        match (self.kind, input) {
            (OpsKind::Hash, _) => input.clone(),
            (OpsKind::Nbt, Dyn::Map(m)) => Dyn::Map(crate::nbt_order::copy_remove(m, key)),
            (_, Dyn::Map(m)) => Dyn::Map(
                m.iter()
                    .filter(|(k, _)| k.as_str() != Some(key))
                    .cloned()
                    .collect(),
            ),
            _ => input.clone(),
        }
    }

    /// `DynamicOps.getGeneric(input, key)`: panics on non-string keys like `NbtOps`' `MapLike.get`.
    pub fn get_generic(&self, input: &Dyn, key: &Dyn) -> DataResult<Dyn> {
        self.get_map(input).flat_map(|map| {
            let Dyn::Str(k) = key else {
                panic!("Cannot get map entry with non-string key: {key}")
            };
            match map.get(k) {
                Some(v) => DataResult::success(v.clone()),
                None => {
                    let (k, i) = (self.show(key), self.show(input));
                    DataResult::error(move || format!("No element {k} in the map {i}"))
                }
            }
        })
    }

    /// `DynamicOps.get(input, key)`.
    pub fn get(&self, input: &Dyn, key: &str) -> DataResult<Dyn> {
        self.get_generic(input, &Dyn::Str(key.to_string()))
    }

    /// `DynamicOps.set(input, key, value)`: `mergeToMap(..).result().orElse(input)`.
    pub fn set(&self, input: &Dyn, key: &str, value: Dyn) -> Dyn {
        self.merge_to_map_one(input, Dyn::Str(key.to_string()), value)
            .result()
            .unwrap_or_else(|| input.clone())
    }

    /// `DynamicOps.update(input, key, function)`.
    pub fn update(&self, input: &Dyn, key: &str, function: impl FnOnce(Dyn) -> Dyn) -> Dyn {
        match self.get(input, key).result() {
            Some(v) => self.set(input, key, function(v)),
            None => input.clone(),
        }
    }

    /// `DynamicOps.updateGeneric(input, key, function)`.
    pub fn update_generic(&self, input: &Dyn, key: &Dyn, function: impl FnOnce(Dyn) -> Dyn) -> Dyn {
        self.get_generic(input, key)
            .flat_map(|v| self.merge_to_map_one(input, key.clone(), function(v)))
            .result()
            .unwrap_or_else(|| input.clone())
    }

    /// `toString()` of a value of this ops' type (`Tag` SNBT, `JsonElement` text).
    pub fn show(&self, value: &Dyn) -> String {
        if self.is_nbt()
            && let Ok(tag) = value.to_tag()
        {
            return tag.to_string();
        }
        value.to_json_string()
    }

    /// `Dynamic.convert`: re-express `input` (built by `self`) in `out`'s ops.
    pub fn convert_to(&self, out: &Ops, input: &Dyn) -> Dyn {
        assert!(!self.is_hash(), "Can't convert from this type");
        match self.kind {
            OpsKind::Nbt => match input {
                Dyn::Empty => out.empty(),
                Dyn::Byte(v) => out.create_byte(*v),
                Dyn::Short(v) => out.create_short(*v),
                Dyn::Int(v) => out.create_int(*v),
                Dyn::Long(v) => out.create_long(*v),
                Dyn::Float(v) => out.create_float(*v),
                Dyn::Double(v) => out.create_double(*v),
                Dyn::Bytes(v) => out.create_byte_list(v.clone()),
                Dyn::Ints(v) => out.create_int_list(v.clone()),
                Dyn::Longs(v) => out.create_long_list(v.clone()),
                Dyn::Str(s) => out.create_string(s.clone()),
                Dyn::List(l) => {
                    out.create_list(l.iter().map(|e| self.convert_to(out, e)).collect())
                }
                Dyn::Map(m) => out.create_map(
                    m.iter()
                        .map(|(k, v)| (self.convert_to(out, k), self.convert_to(out, v)))
                        .collect(),
                ),
                Dyn::Bool(b) => out.create_boolean(*b),
                Dyn::JsonNum(_) => self.convert_json_number(out, input),
            },
            _ => match input {
                Dyn::Empty => out.empty(),
                Dyn::Str(s) => out.create_string(s.clone()),
                Dyn::Bool(b) => out.create_boolean(*b),
                // `JsonOps.getStream/getMapValues` hand nested `JsonNull`s out as Java `null`, which `convertTo` dereferences
                Dyn::List(l) => {
                    out.create_list(l.iter().map(|e| self.convert_nested(out, e)).collect())
                }
                Dyn::Map(m) => out.create_map(
                    m.iter()
                        .map(|(k, v)| (self.convert_to(out, k), self.convert_nested(out, v)))
                        .collect(),
                ),
                Dyn::Bytes(v) => out.create_byte_list(v.clone()),
                Dyn::Ints(v) => out.create_int_list(v.clone()),
                Dyn::Longs(v) => out.create_long_list(v.clone()),
                n => self.convert_json_number(out, n),
            },
        }
    }

    fn convert_nested(&self, out: &Ops, input: &Dyn) -> Dyn {
        assert!(
            !(self.is_json() && input.is_empty_value()),
            "NullPointerException: JsonNull element in JsonOps.convertTo"
        );
        self.convert_to(out, input)
    }

    /// `JsonOps.convertTo` for numbers: narrowest of byte/short/int/long when the `BigDecimal` is an exact long,
    /// otherwise float when exactly representable, else double.
    fn convert_json_number(&self, out: &Ops, n: &Dyn) -> Dyn {
        let text = match n {
            Dyn::Byte(v) => v.to_string(),
            Dyn::Short(v) => v.to_string(),
            Dyn::Int(v) => v.to_string(),
            Dyn::Long(v) => v.to_string(),
            Dyn::Float(v) => java_float_to_string(*v),
            Dyn::Double(v) => java_double_to_string(*v),
            Dyn::JsonNum(t) => t.clone(),
            other => panic!("not a number: {other}"),
        };
        if let Some(l) = decimal_long_exact(&text) {
            return if l as i8 as i64 == l {
                out.create_byte(l as i8)
            } else if l as i16 as i64 == l {
                out.create_short(l as i16)
            } else if l as i32 as i64 == l {
                out.create_int(l as i32)
            } else {
                out.create_long(l)
            };
        }
        let d: f64 = text.parse().unwrap_or(0.0);
        if (d as f32) as f64 == d {
            out.create_float(d as f32)
        } else {
            out.create_double(d)
        }
    }

    /// `HashOps` final hash of a tree built by this (hash) ops.
    pub fn hash(&self, value: &Dyn) -> u32 {
        crate::hash::hash_dyn(value)
    }

    pub fn map_builder(&self) -> RecordBuilder {
        RecordBuilder {
            ops: *self,
            entries: DataResult::success(Vec::new()),
        }
    }
    pub fn list_builder(&self) -> ListBuilder {
        ListBuilder {
            ops: *self,
            items: DataResult::success(Vec::new()),
        }
    }
}

/// `BigDecimal(text).longValueExact()`: the integer value when `text` is an exact long.
fn decimal_long_exact(text: &str) -> Option<i64> {
    let t = text.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (mant, exp) = match t.find(['e', 'E']) {
        Some(i) => (&t[..i], t[i + 1..].parse::<i64>().ok()?),
        None => (t, 0),
    };
    let (int_part, frac_part) = match mant.split_once('.') {
        Some((a, b)) => (a, b),
        None => (mant, ""),
    };
    let mut digits: String = format!("{int_part}{frac_part}");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let scale = frac_part.len() as i64 - exp; // value = digits * 10^-scale
    if scale > 0 {
        let s = scale as usize;
        let tail_start = digits.len().checked_sub(s);
        match tail_start {
            Some(i) if digits[i..].bytes().all(|b| b == b'0') => digits.truncate(i),
            None if digits.bytes().all(|b| b == b'0') => digits.clear(),
            _ => return None,
        }
    } else if scale < 0 {
        if scale < -40 {
            return if digits.bytes().all(|b| b == b'0') {
                Some(0)
            } else {
                None
            };
        }
        digits.push_str(&"0".repeat((-scale) as usize));
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some(0);
    }
    let v: i128 = digits.parse().ok()?;
    let v = if neg { -v } else { v };
    i64::try_from(v).ok()
}

enum Collector {
    Generic(Vec<Dyn>),
    Bytes(Vec<i8>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
}

impl Collector {
    fn accept(self, tag: Dyn) -> Collector {
        match (self, tag) {
            (Collector::Generic(mut v), t) => {
                v.push(t);
                Collector::Generic(v)
            }
            (Collector::Bytes(mut v), Dyn::Byte(b)) => {
                v.push(b);
                Collector::Bytes(v)
            }
            (Collector::Ints(mut v), Dyn::Int(b)) => {
                v.push(b);
                Collector::Ints(v)
            }
            (Collector::Longs(mut v), Dyn::Long(b)) => {
                v.push(b);
                Collector::Longs(v)
            }
            (Collector::Bytes(v), t) => {
                Collector::Generic(v.into_iter().map(Dyn::Byte).chain([t]).collect())
            }
            (Collector::Ints(v), t) => {
                Collector::Generic(v.into_iter().map(Dyn::Int).chain([t]).collect())
            }
            (Collector::Longs(v), t) => {
                Collector::Generic(v.into_iter().map(Dyn::Long).chain([t]).collect())
            }
        }
    }
    fn result(self) -> Dyn {
        match self {
            Collector::Generic(v) => Dyn::List(v),
            Collector::Bytes(v) => Dyn::Bytes(v),
            Collector::Ints(v) => Dyn::Ints(v),
            Collector::Longs(v) => Dyn::Longs(v),
        }
    }
}

/// `RecordBuilder` (`AbstractStringBuilder` for NBT/JSON, `AbstractUniversalBuilder` for hash).
pub struct RecordBuilder {
    ops: Ops,
    entries: DataResult<Vec<(Dyn, Dyn)>>,
}

impl RecordBuilder {
    pub fn ops(&self) -> Ops {
        self.ops
    }

    fn take_entries(&mut self) -> DataResult<Vec<(Dyn, Dyn)>> {
        std::mem::replace(&mut self.entries, DataResult::success(Vec::new()))
    }

    fn append(entries: &mut Vec<(Dyn, Dyn)>, universal: bool, key: Dyn, value: Dyn) {
        if universal {
            entries.push((key, value))
        } else {
            Dyn::map_put(entries, key, value)
        }
    }

    /// `add(String key, T value)`.
    pub fn add(&mut self, key: &str, value: Dyn) -> &mut Self {
        self.add_key(Dyn::Str(key.to_string()), value)
    }

    /// `add(T key, T value)`: NBT/JSON builders require a string key (error otherwise).
    pub fn add_key(&mut self, key: Dyn, value: Dyn) -> &mut Self {
        let universal = self.ops.is_hash();
        let prev = self.take_entries();
        self.entries = if universal {
            prev.map(|mut b| {
                Self::append(&mut b, true, key, value);
                b
            })
        } else {
            match self.ops.get_string_value(&key) {
                DataResult::Success(k) => prev.map(|mut b| {
                    Self::append(&mut b, false, Dyn::Str(k), value);
                    b
                }),
                e => prev.flat_map(|_| e.cast_error()),
            }
        };
        self
    }

    /// `add(String key, DataResult<T> value)` (`apply2stable`).
    pub fn add_result(&mut self, key: &str, value: DataResult<Dyn>) -> &mut Self {
        let universal = self.ops.is_hash();
        let prev = self.take_entries();
        let key = Dyn::Str(key.to_string());
        self.entries = prev.apply2(value, move |mut b, v| {
            Self::append(&mut b, universal, key, v);
            b
        });
        self
    }

    /// `add(T key, DataResult<T> value)`.
    pub fn add_key_result(&mut self, key: Dyn, value: DataResult<Dyn>) -> &mut Self {
        let universal = self.ops.is_hash();
        if universal {
            let prev = self.take_entries();
            self.entries = prev.apply2(value, move |mut b, v| {
                Self::append(&mut b, true, key, v);
                b
            });
            return self;
        }
        match self.ops.get_string_value(&key) {
            DataResult::Success(k) => self.add_result(&k, value),
            e => {
                let prev = self.take_entries();
                self.entries = prev.flat_map(|_| e.cast_error());
                self
            }
        }
    }

    /// `add(DataResult<T> key, DataResult<T> value)`.
    pub fn add_results(&mut self, key: DataResult<Dyn>, value: DataResult<Dyn>) -> &mut Self {
        let universal = self.ops.is_hash();
        if universal {
            let prev = self.take_entries();
            self.entries = prev.apply2(key.apply2(value, |k, v| (k, v)), move |mut b, (k, v)| {
                Self::append(&mut b, true, k, v);
                b
            });
            return self;
        }
        match key {
            DataResult::Success(k) => self.add_key_result(k, value),
            e => {
                let ops = self.ops;
                let prev = self.take_entries();
                self.entries =
                    prev.flat_map(|_| e.flat_map(|k| ops.get_string_value(&k)).cast_error_keep());
                self
            }
        }
    }

    /// `withErrorsFrom(result)`.
    pub fn with_errors_from<X>(&mut self, result: &DataResult<X>) -> &mut Self {
        if let DataResult::Error { message, .. } = result {
            let prev = self.take_entries();
            self.entries = prev.flat_map(|_| DataResult::error_msg(message.clone(), None));
        }
        self
    }

    /// `mapError`.
    pub fn map_error(&mut self, f: impl Fn(String) -> String + Send + Sync + 'static) -> &mut Self {
        let prev = self.take_entries();
        self.entries = prev.map_error(f);
        self
    }

    /// `build(prefix)`; the builder is reset.
    pub fn build(&mut self, prefix: &Dyn) -> DataResult<Dyn> {
        let ops = self.ops;
        let entries = self.take_entries();
        entries.flat_map(|e| match ops.kind {
            OpsKind::Hash => DataResult::success(Dyn::Map(e)),
            _ => match prefix {
                Dyn::Empty if ops.is_nbt() => {
                    DataResult::success(Dyn::Map(crate::nbt_order::organic(e)))
                }
                Dyn::Empty => DataResult::success(Dyn::Map(e)),
                Dyn::Map(p) if ops.is_nbt() => {
                    // the builder is a `CompoundTag`: its entries are merged in its own iteration order
                    DataResult::success(Dyn::Map(crate::nbt_order::copy_put(
                        p,
                        crate::nbt_order::organic(e),
                    )))
                }
                Dyn::Map(p) => {
                    let mut out = p.clone();
                    for (k, v) in e {
                        Dyn::map_put(&mut out, k, v);
                    }
                    DataResult::success(Dyn::Map(out))
                }
                other => {
                    let o = ops.show(other);
                    DataResult::error_partial(
                        move || format!("mergeToMap called with not a map: {o}"),
                        other.clone(),
                    )
                }
            },
        })
    }

    /// `build(DataResult<T> prefix)`.
    pub fn build_result(&mut self, prefix: DataResult<Dyn>) -> DataResult<Dyn> {
        prefix.flat_map(|p| self.build(&p))
    }
}

trait CastErrorKeep<A> {
    fn cast_error_keep(self) -> DataResult<A>;
}
impl<A, B> CastErrorKeep<A> for DataResult<B> {
    fn cast_error_keep(self) -> DataResult<A> {
        match self {
            DataResult::Error { message, .. } => DataResult::error_msg(message, None),
            DataResult::Success(_) => unreachable!("string key lookups return errors here"),
        }
    }
}

/// `ListBuilder`.
pub struct ListBuilder {
    ops: Ops,
    items: DataResult<Vec<Dyn>>,
}

impl ListBuilder {
    pub fn add(&mut self, value: Dyn) -> &mut Self {
        let prev = std::mem::replace(&mut self.items, DataResult::success(Vec::new()));
        self.items = prev.map(|mut b| {
            b.push(value);
            b
        });
        self
    }
    pub fn add_result(&mut self, value: DataResult<Dyn>) -> &mut Self {
        let prev = std::mem::replace(&mut self.items, DataResult::success(Vec::new()));
        self.items = prev.apply2(value, |mut b, v| {
            b.push(v);
            b
        });
        self
    }
    pub fn build(&mut self, prefix: &Dyn) -> DataResult<Dyn> {
        let items = std::mem::replace(&mut self.items, DataResult::success(Vec::new()));
        let ops = self.ops;
        items.flat_map(|b| ops.merge_to_list(prefix, b))
    }
}
