//! `DataComponentMap`, `DataComponentPatch`, `PatchedDataComponentMap`, `TypedDataComponent`.

use super::component::{ComponentKey, Value};
use super::registry::{def, try_def};
use crate::codec::{read_varint, write_varint, Decode, DecodeError, DecodeResult, Encode};
use mc_core::ident::Identifier;
use mc_data::DataComponentType;
use mc_dfu::{Codec, DataResult};
use std::sync::{Arc, LazyLock};

/// `TypedDataComponent<T>` (type-erased).
#[derive(Clone, Debug, PartialEq)]
pub struct TypedDataComponent {
    pub id: DataComponentType,
    pub value: Value,
}

/// `DataComponentGetter`: anything that can look up a component value.
pub trait DataComponentGetter {
    fn get_value(&self, id: DataComponentType) -> Option<&Value>;

    fn get<T: 'static>(&self, key: &ComponentKey<T>) -> Option<&T> {
        self.get_value(key.id()).map(|v| v.downcast_ref::<T>().unwrap_or_else(|| panic!("wrong value type for {}", key.id().name())))
    }
    fn get_or_default<'a, T: 'static>(&'a self, key: &ComponentKey<T>, default: &'a T) -> &'a T {
        self.get(key).unwrap_or(default)
    }
    fn has(&self, id: DataComponentType) -> bool {
        self.get_value(id).is_some()
    }
}

// ---- DataComponentMap ----------------------------------------------------------------------------------------

/// Immutable map of components (`DataComponentMap`). Iteration is in insertion order.
#[derive(Clone, Debug)]
pub struct DataComponentMap {
    entries: Arc<Vec<(DataComponentType, Value)>>,
}

impl Default for DataComponentMap {
    fn default() -> Self {
        static EMPTY: LazyLock<DataComponentMap> = LazyLock::new(|| DataComponentMap { entries: Arc::new(Vec::new()) });
        EMPTY.clone()
    }
}

impl PartialEq for DataComponentMap {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|(k, v)| other.get_value(*k).is_some_and(|o| o == v))
    }
}

impl DataComponentGetter for DataComponentMap {
    fn get_value(&self, id: DataComponentType) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| *k == id).map(|(_, v)| v)
    }
}

impl DataComponentMap {
    pub fn empty() -> DataComponentMap {
        Self::default()
    }
    pub fn builder() -> DataComponentMapBuilder {
        DataComponentMapBuilder::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (DataComponentType, &Value)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }
    pub fn key_set(&self) -> impl Iterator<Item = DataComponentType> + '_ {
        self.entries.iter().map(|(k, _)| *k)
    }
    /// `DataComponentMap.CODEC`: persistent components only, as `{"minecraft:x": value}`.
    pub fn codec() -> Codec<DataComponentMap> {
        static C: LazyLock<Codec<DataComponentMap>> = LazyLock::new(|| {
            mc_dfu::extra::dispatched_map(&persistent_type_codec(), |t| DataResult::success(def(*t).codec_or_throw().clone())).flat_comap_map(
                |entries| DataComponentMap { entries: Arc::new(entries) },
                |m: &DataComponentMap| {
                    DataResult::success(m.entries.iter().filter(|(k, _)| !def(*k).is_transient()).cloned().collect())
                },
            )
        });
        C.clone()
    }
}

/// `DataComponentMap.Builder`.
#[derive(Default)]
pub struct DataComponentMapBuilder {
    entries: Vec<(DataComponentType, Value)>,
}

impl DataComponentMapBuilder {
    pub fn set<T: super::ComponentValue>(mut self, key: ComponentKey<T>, value: T) -> Self {
        self.set_value(key.id(), Some(Value::new(value)));
        self
    }
    /// `setUnchecked`: `None` removes.
    pub fn set_value(&mut self, id: DataComponentType, value: Option<Value>) -> &mut Self {
        match (self.entries.iter().position(|(k, _)| *k == id), value) {
            (Some(i), Some(v)) => self.entries[i].1 = v,
            (None, Some(v)) => self.entries.push((id, v)),
            (Some(i), None) => {
                self.entries.remove(i);
            }
            (None, None) => {}
        }
        self
    }
    pub fn add_all(mut self, map: &DataComponentMap) -> Self {
        for (k, v) in map.iter() {
            self.set_value(k, Some(v.clone()));
        }
        self
    }
    pub fn build(self) -> DataComponentMap {
        DataComponentMap { entries: Arc::new(self.entries) }
    }
}

// ---- DataComponentType codecs --------------------------------------------------------------------------------

/// `DataComponentType.CODEC` (by registry name).
pub fn type_codec() -> Codec<DataComponentType> {
    super::holder::registry_by_name_codec::<DataComponentType>()
}

/// `DataComponentType.PERSISTENT_CODEC`.
pub fn persistent_type_codec() -> Codec<DataComponentType> {
    type_codec().validate(|t| {
        if def(t).is_transient() {
            DataResult::error(move || format!("Encountered transient component {}", t.identifier()))
        } else {
            DataResult::success(t)
        }
    })
}

// ---- DataComponentPatch --------------------------------------------------------------------------------------

/// `DataComponentPatch`: per-type overrides in insertion order; `None` = removed (`Removed.INSTANCE`).
#[derive(Clone, Debug)]
pub struct DataComponentPatch {
    entries: Arc<Vec<(DataComponentType, Option<Value>)>>,
}

impl Default for DataComponentPatch {
    fn default() -> Self {
        static EMPTY: LazyLock<DataComponentPatch> = LazyLock::new(|| DataComponentPatch { entries: Arc::new(Vec::new()) });
        EMPTY.clone()
    }
}

impl PartialEq for DataComponentPatch {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|(k, v)| other.entries.iter().any(|(k2, v2)| k == k2 && v == v2))
    }
}

fn put(entries: &mut Vec<(DataComponentType, Option<Value>)>, id: DataComponentType, v: Option<Value>) {
    match entries.iter_mut().find(|(k, _)| *k == id) {
        Some(e) => e.1 = v,
        None => entries.push((id, v)),
    }
}

impl DataComponentPatch {
    pub fn empty() -> DataComponentPatch {
        Self::default()
    }
    pub fn builder() -> DataComponentPatchBuilder {
        DataComponentPatchBuilder::default()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Entries in insertion order; `None` marks a removal.
    pub fn iter(&self) -> impl Iterator<Item = (DataComponentType, Option<&Value>)> {
        self.entries.iter().map(|(k, v)| (*k, v.as_ref()))
    }
    pub fn entries(&self) -> &[(DataComponentType, Option<Value>)] {
        &self.entries
    }
    /// `get(prototype, type)`: patch value, else prototype; a removal reads as absent.
    pub fn get_value<'a>(&'a self, prototype: &'a impl DataComponentGetter, id: DataComponentType) -> Option<&'a Value> {
        match self.entries.iter().find(|(k, _)| *k == id) {
            Some((_, v)) => v.as_ref(),
            None => prototype.get_value(id),
        }
    }
    pub fn get<'a, T: 'static>(&'a self, prototype: &'a impl DataComponentGetter, key: &ComponentKey<T>) -> Option<&'a T> {
        self.get_value(prototype, key.id()).map(|v| v.downcast_ref::<T>().expect("component value type"))
    }
    /// `forget(test)`: drop entries whose type matches.
    pub fn forget(&self, test: impl Fn(DataComponentType) -> bool) -> DataComponentPatch {
        let v: Vec<_> = self.entries.iter().filter(|(k, _)| !test(*k)).cloned().collect();
        DataComponentPatch { entries: Arc::new(v) }
    }
    /// `split()`: (added components, removed types).
    pub fn split(&self) -> (DataComponentMap, Vec<DataComponentType>) {
        let mut added = DataComponentMap::builder();
        let mut removed = Vec::new();
        for (k, v) in self.entries.iter() {
            match v {
                Some(v) => {
                    added.set_value(*k, Some(v.clone()));
                }
                None => removed.push(*k),
            }
        }
        (added.build(), removed)
    }

    /// `DataComponentPatch.CODEC`.
    pub fn codec() -> Codec<DataComponentPatch> {
        static C: LazyLock<Codec<DataComponentPatch>> = LazyLock::new(|| {
            let key_codec = patch_key_codec();
            mc_dfu::extra::dispatched_map(&key_codec, |k: &PatchKey| {
                if k.removed {
                    DataResult::success(removed_codec())
                } else {
                    match try_def(k.ty).and_then(|d| d.opt_codec.clone()) {
                        Some(c) => DataResult::success(c),
                        None => {
                            let name = k.ty.name();
                            DataResult::error(move || format!("data component {name} is not implemented"))
                        }
                    }
                }
            })
            .xmap(
                |data: Vec<(PatchKey, Option<Value>)>| {
                    let mut map = Vec::with_capacity(data.len());
                    for (k, v) in data {
                        put(&mut map, k.ty, v);
                    }
                    DataComponentPatch { entries: Arc::new(map) }
                },
                |patch: &DataComponentPatch| {
                    patch
                        .entries
                        .iter()
                        
                        .filter(|(k, _)| !try_def(*k).is_some_and(|d| d.is_transient()))
                        .map(|(k, v)| (PatchKey { ty: *k, removed: v.is_none() }, v.clone()))
                        .collect()
                },
            )
        });
        C.clone()
    }

    fn write_stream(&self, out: &mut Vec<u8>, delimited: bool) {
        if self.is_empty() {
            write_varint(out, 0);
            write_varint(out, 0);
            return;
        }
        let positive = self.entries.iter().filter(|(_, v)| v.is_some()).count();
        write_varint(out, positive as i32);
        write_varint(out, (self.entries.len() - positive) as i32);
        for (k, v) in self.entries.iter() {
            if let Some(v) = v {
                k.encode(out);
                if delimited {
                    let mut scratch = Vec::new();
                    def(*k).encode_network(v, &mut scratch);
                    write_varint(out, scratch.len() as i32);
                    out.extend_from_slice(&scratch);
                } else {
                    def(*k).encode_network(v, out);
                }
            }
        }
        for (k, v) in self.entries.iter() {
            if v.is_none() {
                k.encode(out);
            }
        }
    }

    fn read_stream(input: &mut &[u8], delimited: bool) -> DecodeResult<DataComponentPatch> {
        let positive = read_varint(input)?;
        let negative = read_varint(input)?;
        if positive == 0 && negative == 0 {
            return Ok(DataComponentPatch::empty());
        }
        let expected = (positive as i64 + negative as i64).clamp(0, 65536) as usize;
        let mut map: Vec<(DataComponentType, Option<Value>)> = Vec::with_capacity(expected);
        for _ in 0..positive {
            let ty = DataComponentType::decode(input)?;
            let d = try_def(ty).ok_or_else(|| DecodeError::Message(format!("data component {} is not implemented", ty.name())))?;
            let value = if delimited {
                let size = read_varint(input)?;
                // `ByteBufCodecs.lengthPrefixed(Integer.MAX_VALUE)`
                let size_u = usize::try_from(size).map_err(|_| DecodeError::Message(format!("Illegal Capacity: {size}")))?;
                let mut slice = crate::codec::take(input, size_u)?;
                d.decode_network(&mut slice)?
            } else {
                d.decode_network(input)?
            };
            put(&mut map, ty, Some(value));
        }
        for _ in 0..negative {
            let ty = DataComponentType::decode(input)?;
            put(&mut map, ty, None);
        }
        Ok(DataComponentPatch { entries: Arc::new(map) })
    }

    /// `DataComponentPatch.DELIMITED_STREAM_CODEC` encode: every value is length-prefixed.
    pub fn encode_delimited(&self, out: &mut Vec<u8>) {
        self.write_stream(out, true)
    }
    /// `DataComponentPatch.DELIMITED_STREAM_CODEC` decode (untrusted sources: client packets).
    pub fn decode_delimited(input: &mut &[u8]) -> DecodeResult<DataComponentPatch> {
        Self::read_stream(input, true)
    }
}

/// `DataComponentPatch.STREAM_CODEC`.
impl Encode for DataComponentPatch {
    fn encode(&self, out: &mut Vec<u8>) {
        self.write_stream(out, false)
    }
}
impl Decode for DataComponentPatch {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Self::read_stream(input, false)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PatchKey {
    ty: DataComponentType,
    removed: bool,
}

fn patch_key_codec() -> Codec<PatchKey> {
    mc_dfu::STRING.flat_xmap(
        |string| {
            let (removed, name) = match string.strip_prefix('!') {
                Some(r) => (true, r.to_string()),
                None => (false, string),
            };
            let id = Identifier::try_parse(&name);
            let ty = id.as_ref().and_then(|id| DataComponentType::by_name(&id.to_string()));
            match ty {
                None => {
                    let shown = id.map(|i| i.to_string()).unwrap_or_else(|| "null".into());
                    DataResult::error(move || format!("No component with type: '{shown}'"))
                }
                Some(t) if try_def(t).is_some_and(|d| d.is_transient()) => {
                    DataResult::error(move || format!("'{}' is not a persistent component", t.name()))
                }
                Some(t) => DataResult::success(PatchKey { ty: t, removed }),
            }
        },
        |k: &PatchKey| DataResult::success(if k.removed { format!("!{}", k.ty.name()) } else { k.ty.name().to_string() }),
    )
}

/// `Removed.CODEC` (`Codec.unit(Removed.INSTANCE)`).
fn removed_codec() -> Codec<Option<Value>> {
    mc_dfu::UNIT.xmap(|()| None, |_: &Option<Value>| ())
}

/// `DataComponentPatch.Builder`.
#[derive(Default)]
pub struct DataComponentPatchBuilder {
    entries: Vec<(DataComponentType, Option<Value>)>,
}

impl DataComponentPatchBuilder {
    pub fn set<T: super::ComponentValue>(mut self, key: ComponentKey<T>, value: T) -> Self {
        put(&mut self.entries, key.id(), Some(Value::new(value)));
        self
    }
    pub fn set_value(mut self, id: DataComponentType, value: Value) -> Self {
        put(&mut self.entries, id, Some(value));
        self
    }
    pub fn remove(mut self, id: DataComponentType) -> Self {
        put(&mut self.entries, id, None);
        self
    }
    pub fn build(self) -> DataComponentPatch {
        DataComponentPatch { entries: Arc::new(self.entries) }
    }
}

// ---- PatchedDataComponentMap ---------------------------------------------------------------------------------

/// `PatchedDataComponentMap`: an item's prototype plus a patch (values copy-on-write via `Arc`).
#[derive(Clone, Debug, Default)]
pub struct PatchedDataComponentMap {
    prototype: DataComponentMap,
    patch: Vec<(DataComponentType, Option<Value>)>,
}

impl PartialEq for PatchedDataComponentMap {
    fn eq(&self, other: &Self) -> bool {
        self.prototype == other.prototype
            && self.patch.len() == other.patch.len()
            && self.patch.iter().all(|(k, v)| other.patch.iter().any(|(k2, v2)| k == k2 && v == v2))
    }
}

impl DataComponentGetter for PatchedDataComponentMap {
    fn get_value(&self, id: DataComponentType) -> Option<&Value> {
        match self.patch.iter().find(|(k, _)| *k == id) {
            Some((_, v)) => v.as_ref(),
            None => self.prototype.get_value(id),
        }
    }
}

impl PatchedDataComponentMap {
    pub fn new(prototype: DataComponentMap) -> Self {
        PatchedDataComponentMap { prototype, patch: Vec::new() }
    }

    /// `fromPatch(prototype, patch)`: entries equal to the prototype (or removing a missing component) are dropped.
    pub fn from_patch(prototype: DataComponentMap, patch: &DataComponentPatch) -> Self {
        let mut m = Self::new(prototype);
        m.apply_patch(patch);
        m
    }

    pub fn prototype(&self) -> &DataComponentMap {
        &self.prototype
    }

    /// `hasNonDefault`.
    pub fn has_non_default(&self, id: DataComponentType) -> bool {
        self.patch.iter().any(|(k, _)| *k == id)
    }

    /// `set(type, value)` (`None` = null): returns the previous value.
    pub fn set_value(&mut self, id: DataComponentType, value: Option<Value>) -> Option<Value> {
        let default = self.prototype.get_value(id).cloned();
        let idx = self.patch.iter().position(|(k, _)| *k == id);
        let last = if value == default {
            idx.map(|i| self.patch.remove(i).1)
        } else {
            match idx {
                Some(i) => Some(std::mem::replace(&mut self.patch[i].1, value.clone())),
                None => {
                    self.patch.push((id, value.clone()));
                    None
                }
            }
        };
        // lastValue != null ? removedToNull(lastValue) ?: default : default
        match last {
            Some(Some(v)) => Some(v),
            Some(None) | None => default,
        }
    }

    pub fn set<T: super::ComponentValue>(&mut self, key: ComponentKey<T>, value: T) -> Option<Value> {
        self.set_value(key.id(), Some(Value::new(value)))
    }

    /// `remove(type)`.
    pub fn remove(&mut self, id: DataComponentType) -> Option<Value> {
        let default = self.prototype.get_value(id).cloned();
        let idx = self.patch.iter().position(|(k, _)| *k == id);
        let last = if default.is_some() {
            match idx {
                Some(i) => Some(std::mem::replace(&mut self.patch[i].1, None)),
                None => {
                    self.patch.push((id, None));
                    None
                }
            }
        } else {
            idx.map(|i| self.patch.remove(i).1)
        };
        match last {
            Some(v) => v.or(default),
            None => default,
        }
    }

    /// `applyPatch(patch)`.
    pub fn apply_patch(&mut self, patch: &DataComponentPatch) {
        for (id, v) in patch.entries.iter() {
            let default = self.prototype.get_value(*id);
            match v {
                Some(v) => {
                    if default == Some(v) {
                        self.patch.retain(|(k, _)| k != id);
                    } else {
                        put(&mut self.patch, *id, Some(v.clone()));
                    }
                }
                None => {
                    if default.is_some() {
                        put(&mut self.patch, *id, None);
                    } else {
                        self.patch.retain(|(k, _)| k != id);
                    }
                }
            }
        }
    }

    pub fn restore_patch(&mut self, patch: &DataComponentPatch) {
        self.patch = patch.entries.to_vec();
    }
    pub fn clear_patch(&mut self) {
        self.patch.clear();
    }
    /// `setAll(components)`.
    pub fn set_all(&mut self, components: &DataComponentMap) {
        for (k, v) in components.iter() {
            self.set_value(k, Some(v.clone()));
        }
    }

    /// `keySet()`.
    pub fn key_set(&self) -> Vec<DataComponentType> {
        let mut keys: Vec<DataComponentType> = self.prototype.key_set().collect();
        for (k, v) in &self.patch {
            match v {
                Some(_) => {
                    if !keys.contains(k) {
                        keys.push(*k)
                    }
                }
                None => keys.retain(|x| x != k),
            }
        }
        keys
    }

    /// Iteration like vanilla: patched (non-removed) entries first, then untouched prototype entries.
    pub fn iter(&self) -> impl Iterator<Item = (DataComponentType, &Value)> {
        let patched = self.patch.iter().filter_map(|(k, v)| v.as_ref().map(|v| (*k, v)));
        let proto = self.prototype.iter().filter(|(k, _)| !self.patch.iter().any(|(p, _)| p == k));
        patched.chain(proto)
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `asPatch()`.
    pub fn as_patch(&self) -> DataComponentPatch {
        DataComponentPatch { entries: Arc::new(self.patch.clone()) }
    }

    /// `toImmutableMap()`.
    pub fn to_immutable_map(&self) -> DataComponentMap {
        if self.patch.is_empty() {
            return self.prototype.clone();
        }
        let mut b = DataComponentMap::builder();
        for (k, v) in self.iter() {
            b.set_value(k, Some(v.clone()));
        }
        b.build()
    }
}

/// `TypedDataComponent.STREAM_CODEC`: component type id + its network value.
impl Encode for TypedDataComponent {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        def(self.id).encode_network(&self.value, out);
    }
}
impl Decode for TypedDataComponent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = DataComponentType::decode(input)?;
        let d = try_def(id).ok_or_else(|| DecodeError::Message(format!("data component {} is not implemented", id.name())))?;
        Ok(TypedDataComponent { id, value: d.decode_network(input)? })
    }
}

// ---- DataComponentExactPredicate ---------------------------------------------------------------------------------

/// `DataComponentExactPredicate`: components that must be present with exactly these values.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DataComponentExactPredicate {
    expected: Vec<TypedDataComponent>,
}

impl DataComponentExactPredicate {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn expect<T: super::ComponentValue>(key: ComponentKey<T>, value: T) -> Self {
        DataComponentExactPredicate { expected: vec![TypedDataComponent { id: key.id(), value: Value::new(value) }] }
    }
    pub fn from_components(expected: Vec<TypedDataComponent>) -> Self {
        DataComponentExactPredicate { expected }
    }
    /// `allOf(components)`.
    pub fn all_of(components: &DataComponentMap) -> Self {
        DataComponentExactPredicate {
            expected: components.iter().map(|(id, v)| TypedDataComponent { id, value: v.clone() }).collect(),
        }
    }
    /// `someOf(components, types...)`.
    pub fn some_of(components: &DataComponentMap, types: &[DataComponentType]) -> Self {
        DataComponentExactPredicate {
            expected: types
                .iter()
                .filter_map(|t| components.get_value(*t).map(|v| TypedDataComponent { id: *t, value: v.clone() }))
                .collect(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.expected.is_empty()
    }
    pub fn expected_components(&self) -> &[TypedDataComponent] {
        &self.expected
    }
    /// `asPatch()`: the expected components as additions to a patch (a later entry of the same type wins).
    pub fn as_patch(&self) -> DataComponentPatch {
        let mut builder = DataComponentPatch::builder();
        for e in &self.expected {
            builder = builder.set_value(e.id, e.value.clone());
        }
        builder.build()
    }
    pub fn test(&self, actual: &impl DataComponentGetter) -> bool {
        self.expected.iter().all(|e| actual.get_value(e.id) == Some(&e.value))
    }

    /// `DataComponentExactPredicate.CODEC`: `{"minecraft:x": value}` (persistent components only).
    pub fn codec() -> Codec<DataComponentExactPredicate> {
        static C: LazyLock<Codec<DataComponentExactPredicate>> = LazyLock::new(|| {
            mc_dfu::extra::dispatched_map(&persistent_type_codec(), |t| DataResult::success(def(*t).codec_or_throw().clone())).flat_comap_map(
                |entries: Vec<(DataComponentType, Value)>| DataComponentExactPredicate {
                    expected: entries.into_iter().map(|(id, value)| TypedDataComponent { id, value }).collect(),
                },
                |p: &DataComponentExactPredicate| {
                    // `Collectors.toMap` throws on a repeated component type (a list decoded from the wire may repeat one)
                    let entries: Vec<(DataComponentType, Value)> =
                        p.expected.iter().filter(|e| !def(e.id).is_transient()).map(|e| (e.id, e.value.clone())).collect();
                    match entries.iter().enumerate().find(|(i, (k, _))| entries[..*i].iter().any(|(k2, _)| k2 == k)) {
                        Some((_, (k, _))) => {
                            let m = format!("Duplicate key {}", k.name());
                            DataResult::error(move || m.clone())
                        }
                        None => DataResult::success(entries),
                    }
                },
            )
        });
        C.clone()
    }
}

/// `DataComponentExactPredicate.STREAM_CODEC`: a list of typed components.
impl Encode for DataComponentExactPredicate {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.expected.len() as i32);
        for e in &self.expected {
            e.encode(out);
        }
    }
}
impl Decode for DataComponentExactPredicate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(DataComponentExactPredicate { expected: <TypedDataComponent as Decode>::decode_vec(input, i32::MAX as usize)? })
    }
}
