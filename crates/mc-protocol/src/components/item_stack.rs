//! `ItemStack` (value type: item, count, patched component map) and `ItemStackTemplate`.

use super::map::{DataComponentGetter, DataComponentMap, DataComponentPatch, PatchedDataComponentMap};
use super::registry::try_def;
use super::values::{DAMAGE, MAX_DAMAGE, MAX_STACK_SIZE, UNBREAKABLE};
use super::{ComponentKey, ComponentValue, Value};
use crate::codec::{read_varint, write_varint, Decode, DecodeError, DecodeResult, Encode};
use mc_data::{DataComponentType, Item};
use mc_dfu::{Codec, DataResult, Dyn, JSON};
use std::sync::{LazyLock, OnceLock};

/// Components of items whose default JSON could not be parsed because a component family is missing (see tests).
pub static MISSING_PROTOTYPE_COMPONENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// `Item.components()`: the item's default component map, parsed once from `mc_data`'s extracted defaults.
pub fn item_prototype(item: Item) -> &'static DataComponentMap {
    static PROTOS: OnceLock<Vec<OnceLock<DataComponentMap>>> = OnceLock::new();
    let protos = PROTOS.get_or_init(|| (0..Item::registry().len()).map(|_| OnceLock::new()).collect());
    protos[item.id() as usize].get_or_init(|| {
        let json = Dyn::from_json_str(item.data().components_json).expect("components_json is valid JSON");
        let mut b = DataComponentMap::builder();
        if let Dyn::Map(entries) = json {
            for (k, v) in entries {
                let name = k.as_str().expect("string key").to_string();
                let ty = DataComponentType::by_name(&name).unwrap_or_else(|| panic!("unknown component {name} in default components of {item}"));
                match try_def(ty) {
                    Some(d) => {
                        let value = d
                            .codec_or_throw()
                            .parse(&JSON, &v)
                            .into_result()
                            .unwrap_or_else(|e| panic!("default component {name} of {item} does not parse: {e}"));
                        b.set_value(ty, Some(value));
                    }
                    None => MISSING_PROTOTYPE_COMPONENTS.lock().unwrap().push(format!("{item}: {name}")),
                }
            }
        }
        b.build()
    })
}

#[derive(Clone, Debug)]
pub struct ItemStack {
    item: Item,
    count: i32,
    components: PatchedDataComponentMap,
}

impl PartialEq for ItemStack {
    /// Structural equality (vanilla `ItemStack` has identity equality; use [`ItemStack::matches`] for gameplay).
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.count() == other.count() && self.components == other.components
    }
}

impl Default for ItemStack {
    fn default() -> Self {
        Self::empty()
    }
}

impl ItemStack {
    /// `ItemStack.EMPTY` (a fresh empty stack; cheap: shares the static empty maps).
    pub fn empty() -> ItemStack {
        ItemStack { item: Item::AIR, count: 0, components: PatchedDataComponentMap::default() }
    }
    pub fn new(item: Item) -> ItemStack {
        Self::with_count(item, 1)
    }
    pub fn with_count(item: Item, count: i32) -> ItemStack {
        ItemStack { item, count, components: PatchedDataComponentMap::new(item_prototype(item).clone()) }
    }
    /// `transmuteCopy(newItem)`: same count and components patch on another item (`EMPTY` for an empty stack).
    pub fn transmute_copy(&self, new_item: Item) -> ItemStack {
        self.transmute_copy_with_count(new_item, self.count())
    }
    /// `transmuteCopy(newItem, newCount)`.
    pub fn transmute_copy_with_count(&self, new_item: Item, new_count: i32) -> ItemStack {
        if self.is_empty() { Self::empty() } else { Self::with_patch(new_item, new_count, &self.get_components_patch()) }
    }
    /// `new ItemStack(item, count, patch)`.
    pub fn with_patch(item: Item, count: i32, patch: &DataComponentPatch) -> ItemStack {
        ItemStack { item, count, components: PatchedDataComponentMap::from_patch(item_prototype(item).clone(), patch) }
    }

    pub fn is_empty(&self) -> bool {
        self.item == Item::AIR || self.count <= 0
    }
    /// `getItem()` / `typeHolder()`: `air` for the empty stack.
    pub fn item(&self) -> Item {
        if self.is_empty() { Item::AIR } else { self.item }
    }
    pub fn is(&self, item: Item) -> bool {
        self.item() == item
    }
    /// `getCount()`: 0 for an empty stack.
    pub fn count(&self) -> i32 {
        if self.is_empty() { 0 } else { self.count }
    }
    pub fn set_count(&mut self, count: i32) {
        self.count = count;
    }
    pub fn grow(&mut self, amount: i32) {
        self.set_count(self.count().wrapping_add(amount));
    }
    pub fn shrink(&mut self, amount: i32) {
        self.grow(amount.wrapping_neg());
    }
    pub fn limit_size(&mut self, max: i32) {
        if !self.is_empty() && self.count() > max {
            self.count = max;
        }
    }

    /// `getComponents()`: the (patched) components; the empty stack has none.
    pub fn components(&self) -> Option<&PatchedDataComponentMap> {
        if self.is_empty() { None } else { Some(&self.components) }
    }
    pub fn get<T: 'static>(&self, key: &ComponentKey<T>) -> Option<&T> {
        self.components().and_then(|c| c.get(key))
    }
    pub fn get_value(&self, id: DataComponentType) -> Option<&Value> {
        self.components().and_then(|c| c.get_value(id))
    }
    pub fn get_or_default<'a, T: 'static>(&'a self, key: &ComponentKey<T>, default: &'a T) -> &'a T {
        self.get(key).unwrap_or(default)
    }
    pub fn has(&self, id: DataComponentType) -> bool {
        self.get_value(id).is_some()
    }
    pub fn has_non_default(&self, id: DataComponentType) -> bool {
        !self.is_empty() && self.components.has_non_default(id)
    }
    /// `set(type, value)`; a no-op on the empty stack like vanilla's (which would mutate `EMPTY`; callers never do).
    pub fn set<T: ComponentValue>(&mut self, key: ComponentKey<T>, value: T) -> Option<Value> {
        self.components.set(key, value)
    }
    pub fn set_value(&mut self, id: DataComponentType, value: Option<Value>) -> Option<Value> {
        self.components.set_value(id, value)
    }
    pub fn remove(&mut self, id: DataComponentType) -> Option<Value> {
        self.components.remove(id)
    }
    pub fn apply_components(&mut self, patch: &DataComponentPatch) {
        self.components.apply_patch(patch)
    }
    pub fn get_components_patch(&self) -> DataComponentPatch {
        if self.is_empty() { DataComponentPatch::empty() } else { self.components.as_patch() }
    }
    pub fn get_prototype(&self) -> DataComponentMap {
        if self.is_empty() { DataComponentMap::empty() } else { self.components.prototype().clone() }
    }
    pub fn immutable_components(&self) -> DataComponentMap {
        if self.is_empty() { DataComponentMap::empty() } else { self.components.to_immutable_map() }
    }

    pub fn copy(&self) -> ItemStack {
        if self.is_empty() { Self::empty() } else { self.clone() }
    }
    pub fn copy_with_count(&self, count: i32) -> ItemStack {
        if self.is_empty() {
            return Self::empty();
        }
        let mut c = self.copy();
        c.set_count(count);
        c
    }
    pub fn copy_and_clear(&mut self) -> ItemStack {
        if self.is_empty() {
            return Self::empty();
        }
        let r = self.copy();
        self.set_count(0);
        r
    }
    /// `split(amount)`.
    pub fn split(&mut self, amount: i32) -> ItemStack {
        let real = amount.min(self.count());
        let result = self.copy_with_count(real);
        self.shrink(real);
        result
    }

    /// `getMaxStackSize()`.
    pub fn get_max_stack_size(&self) -> i32 {
        *self.get_or_default(&MAX_STACK_SIZE, &1)
    }
    pub fn is_stackable(&self) -> bool {
        self.get_max_stack_size() > 1 && (!self.is_damageable_item() || !self.is_damaged())
    }
    pub fn is_damageable_item(&self) -> bool {
        self.has(MAX_DAMAGE.id()) && !self.has(UNBREAKABLE.id()) && self.has(DAMAGE.id())
    }
    pub fn is_damaged(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() > 0
    }
    pub fn get_damage_value(&self) -> i32 {
        (*self.get_or_default(&DAMAGE, &0)).clamp(0, self.get_max_damage().max(0))
    }
    pub fn set_damage_value(&mut self, value: i32) {
        let v = value.clamp(0, self.get_max_damage().max(0));
        self.set(DAMAGE, v);
    }
    pub fn get_max_damage(&self) -> i32 {
        *self.get_or_default(&MAX_DAMAGE, &0)
    }

    /// `ItemStack.matches(a, b)`: equal counts and `isSameItemSameComponents`.
    pub fn matches(a: &ItemStack, b: &ItemStack) -> bool {
        a.count() == b.count() && Self::is_same_item_same_components(a, b)
    }
    pub fn is_same_item(a: &ItemStack, b: &ItemStack) -> bool {
        a.item() == b.item()
    }
    pub fn is_same_item_same_components(a: &ItemStack, b: &ItemStack) -> bool {
        if a.item() != b.item() {
            return false;
        }
        a.is_empty() && b.is_empty() || a.components == b.components
    }
    pub fn matches_ignoring_components(a: &ItemStack, b: &ItemStack, ignored: impl Fn(DataComponentType) -> bool) -> bool {
        if a.count() != b.count() || a.item() != b.item() {
            return false;
        }
        if a.is_empty() && b.is_empty() {
            return true;
        }
        let keep = |m: &PatchedDataComponentMap| -> Vec<(DataComponentType, Value)> {
            m.iter().filter(|(k, _)| !ignored(*k)).map(|(k, v)| (k, v.clone())).collect()
        };
        let (ka, kb) = (keep(&a.components), keep(&b.components));
        ka.len() == kb.len() && ka.iter().all(|(k, v)| kb.iter().any(|(k2, v2)| k == k2 && v == v2))
    }

    // ---- codecs ----------------------------------------------------------------------------------------------

    /// `ItemStack.CODEC` (`MAP_CODEC.codec()`): `{id, count, components}`.
    pub fn codec() -> Codec<ItemStack> {
        static C: LazyLock<Codec<ItemStack>> = LazyLock::new(|| Codec::lazy(build_codec));
        C.clone()
    }

    /// `ItemStack.OPTIONAL_CODEC`: `{}` is the empty stack.
    pub fn optional_codec() -> Codec<ItemStack> {
        static C: LazyLock<Codec<ItemStack>> = LazyLock::new(|| {
            mc_dfu::extra::optional_empty_map(&ItemStack::codec()).xmap(
                |o| o.unwrap_or(ItemStack::empty()),
                |s: &ItemStack| if s.is_empty() { None } else { Some(s.clone()) },
            )
        });
        C.clone()
    }

    pub fn from_tag(tag: &mc_nbt::Tag) -> DataResult<ItemStack> {
        mc_dfu::parse_nbt(&Self::codec(), tag)
    }
    pub fn to_tag(&self) -> DataResult<mc_nbt::Tag> {
        mc_dfu::encode_nbt(&Self::codec(), self)
    }
    pub fn optional_from_tag(tag: &mc_nbt::Tag) -> DataResult<ItemStack> {
        mc_dfu::parse_nbt(&Self::optional_codec(), tag)
    }
    pub fn optional_to_tag(&self) -> DataResult<mc_nbt::Tag> {
        mc_dfu::encode_nbt(&Self::optional_codec(), self)
    }

    /// `ItemStack.validateStrict`.
    pub fn validate_strict(stack: ItemStack) -> DataResult<ItemStack> {
        if let Err(e) = super::values::validate_components(&stack) {
            return DataResult::error(move || e.clone());
        }
        if stack.count() > stack.get_max_stack_size() {
            let (c, m) = (stack.count(), stack.get_max_stack_size());
            return DataResult::error(move || format!("Item stack with stack size of {c} was larger than maximum: {m}"));
        }
        DataResult::success(stack)
    }

    // ---- stream codecs ---------------------------------------------------------------------------------------

    fn encode_with(&self, out: &mut Vec<u8>, delimited: bool) {
        if self.is_empty() {
            write_varint(out, 0);
        } else {
            write_varint(out, self.count);
            self.item.encode(out);
            let patch = self.components.as_patch();
            if delimited { patch.encode_delimited(out) } else { patch.encode(out) }
        }
    }

    fn decode_with(input: &mut &[u8], delimited: bool) -> DecodeResult<ItemStack> {
        let count = read_varint(input)?;
        if count <= 0 {
            return Ok(ItemStack::empty());
        }
        let item = crate::codec::read_registry_id::<Item>(input)?;
        let patch = if delimited { DataComponentPatch::decode_delimited(input)? } else { DataComponentPatch::decode(input)? };
        Ok(ItemStack::with_patch(item, count, &patch))
    }

    /// `ItemStack.STREAM_CODEC` encode: rejects the empty stack.
    pub fn encode_non_empty(&self, out: &mut Vec<u8>) {
        assert!(!self.is_empty(), "Empty ItemStack not allowed");
        self.encode_with(out, false)
    }
    /// `ItemStack.STREAM_CODEC` decode: rejects the empty stack.
    pub fn decode_non_empty(input: &mut &[u8]) -> DecodeResult<ItemStack> {
        let s = Self::decode_with(input, false)?;
        if s.is_empty() {
            return Err(DecodeError::msg("Empty ItemStack not allowed"));
        }
        Ok(s)
    }
    /// `ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC` encode (delimited component values).
    pub fn encode_optional_untrusted(&self, out: &mut Vec<u8>) {
        self.encode_with(out, true)
    }
    /// `ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC` decode.
    pub fn decode_optional_untrusted(input: &mut &[u8]) -> DecodeResult<ItemStack> {
        Self::decode_with(input, true)
    }
    /// `ItemStack.validatedStreamCodec(OPTIONAL_UNTRUSTED_STREAM_CODEC)` decode: the stack must re-encode with `CODEC`.
    pub fn decode_validated_untrusted(input: &mut &[u8]) -> DecodeResult<ItemStack> {
        let s = Self::decode_optional_untrusted(input)?;
        if !s.is_empty() {
            Self::codec().encode_start(&mc_dfu::NBT, &s).into_result().map_err(DecodeError::Message)?;
        }
        Ok(s)
    }
}

fn build_codec() -> Codec<ItemStack> {
    #[derive(Clone)]
    struct Raw {
        id: Item,
        count: i32,
        components: DataComponentPatch,
    }
    let item_codec = super::holder::registry_by_name_codec::<Item>().validate(|item| {
        if item == Item::AIR { DataResult::error_str("Item must not be minecraft:air") } else { DataResult::success(item) }
    });
    mc_dfu::record!(Raw {
        id: item_codec.field_of("id"),
        count: mc_dfu::extra::optional_always_present_field_of(&mc_dfu::extra::extra_int_range(1, 99), "count", 1),
        components: DataComponentPatch::codec().optional_field_of_default("components", DataComponentPatch::empty()),
    })
    .codec()
    .xmap(
        |r: Raw| ItemStack::with_patch(r.id, r.count, &r.components),
        |s: &ItemStack| Raw { id: s.item(), count: s.count(), components: s.get_components_patch() },
    )
}

/// `ItemStack.OPTIONAL_STREAM_CODEC`.
impl Encode for ItemStack {
    fn encode(&self, out: &mut Vec<u8>) {
        self.encode_with(out, false)
    }
}
impl Decode for ItemStack {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Self::decode_with(input, false)
    }
}

impl super::map::DataComponentGetter for ItemStack {
    fn get_value(&self, id: DataComponentType) -> Option<&Value> {
        ItemStack::get_value(self, id)
    }
}

/// `ItemStackTemplate`: item + count + patch, a non-empty stack blueprint (wire order: item, count, patch).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackTemplate {
    pub item: Item,
    pub count: i32,
    pub components: DataComponentPatch,
}

impl ItemStackTemplate {
    /// The constructor's check: `count != 0` and not air.
    pub fn new(item: Item, count: i32, components: DataComponentPatch) -> Result<ItemStackTemplate, String> {
        if count != 0 && item != Item::AIR {
            Ok(ItemStackTemplate { item, count, components })
        } else {
            Err("Item must be non-empty".into())
        }
    }
    pub fn of(item: Item) -> ItemStackTemplate {
        ItemStackTemplate { item, count: 1, components: DataComponentPatch::empty() }
    }
    pub fn from_stack(stack: &ItemStack) -> Result<ItemStackTemplate, String> {
        Self::new(stack.item(), stack.count(), stack.get_components_patch())
    }
    pub fn with_count(&self, count: i32) -> Result<ItemStackTemplate, String> {
        Self::new(self.item, count, self.components.clone())
    }
    /// `create()`: the validated stack, or `EMPTY` when strict validation fails.
    pub fn create(&self) -> ItemStack {
        let s = ItemStack::with_patch(self.item, self.count, &self.components);
        match ItemStack::validate_strict(s) {
            DataResult::Success(s) => s,
            _ => ItemStack::empty(),
        }
    }

    /// `ItemStackTemplate.MAP_CODEC`.
    pub fn map_codec() -> mc_dfu::MapCodec<ItemStackTemplate> {
        let item_codec = super::holder::registry_by_name_codec::<Item>().validate(|item| {
            if item == Item::AIR { DataResult::error_str("Item must not be minecraft:air") } else { DataResult::success(item) }
        });
        mc_dfu::record!(ItemStackTemplate {
            item: item_codec.field_of("id"),
            count: mc_dfu::extra::extra_int_range(1, 99).optional_field_of_default("count", 1),
            components: DataComponentPatch::codec().optional_field_of_default("components", DataComponentPatch::empty()),
        })
    }

    /// `ItemStackTemplate.CODEC`: the map form, or a bare item id.
    pub fn codec() -> Codec<ItemStackTemplate> {
        let item_codec = super::holder::registry_by_name_codec::<Item>().validate(|item| {
            if item == Item::AIR { DataResult::error_str("Item must not be minecraft:air") } else { DataResult::success(item) }
        });
        Self::map_codec().codec().with_alternative_conv(&item_codec, ItemStackTemplate::of)
    }
}

impl Encode for ItemStackTemplate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.item.encode(out);
        write_varint(out, self.count);
        self.components.encode(out);
    }
}
impl Decode for ItemStackTemplate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let item = crate::codec::read_registry_id::<Item>(input)?;
        let count = read_varint(input)?;
        let components = DataComponentPatch::decode(input)?;
        ItemStackTemplate::new(item, count, components).map_err(DecodeError::Message)
    }
}
