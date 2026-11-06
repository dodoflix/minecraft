//! Container-like components: `container`, `bundle_contents`, `charged_projectiles`, `container_loot`, `block_state`,
//! `sulfur_cube_content`, `debug_stick_state`, `recipes`, `lodestone_tracker`, `map_decorations`.

use super::entity::TypedEntityData;
use super::predicates::ItemPredicate;
use crate::codec::EncodeResult;
use crate::codec::{
    Decode, DecodeResult, Encode, GlobalPos, MAX_STRING_LENGTH, read_count, read_utf, write_utf,
    write_varint,
};
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::registry_by_name_codec;
use crate::components::item_stack::{ItemStack, ItemStackTemplate, item_prototype};
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_data::{Block, DataComponentType, EntityType, MapDecorationType};
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{
    BOOL, Codec, DOUBLE, DataResult, FLOAT, INT, INT_STREAM, LONG, STRING, int_range, record,
};

pub const CONTAINER: ComponentKey<ItemContainerContents> =
    ComponentKey::new(DataComponentType::CONTAINER);
pub const BUNDLE_CONTENTS: ComponentKey<BundleContents> =
    ComponentKey::new(DataComponentType::BUNDLE_CONTENTS);
pub const CHARGED_PROJECTILES: ComponentKey<ChargedProjectiles> =
    ComponentKey::new(DataComponentType::CHARGED_PROJECTILES);
pub const CONTAINER_LOOT: ComponentKey<SeededContainerLoot> =
    ComponentKey::new(DataComponentType::CONTAINER_LOOT);
pub const BLOCK_STATE: ComponentKey<BlockItemStateProperties> =
    ComponentKey::new(DataComponentType::BLOCK_STATE);
pub const SULFUR_CUBE_CONTENT: ComponentKey<SulfurCubeContent> =
    ComponentKey::new(DataComponentType::SULFUR_CUBE_CONTENT);
pub const DEBUG_STICK_STATE: ComponentKey<DebugStickState> =
    ComponentKey::new(DataComponentType::DEBUG_STICK_STATE);
pub const LOCK: ComponentKey<LockCode> = ComponentKey::new(DataComponentType::LOCK);
pub const RECIPES: ComponentKey<Vec<Identifier>> = ComponentKey::new(DataComponentType::RECIPES);
pub const LODESTONE_TRACKER: ComponentKey<LodestoneTracker> =
    ComponentKey::new(DataComponentType::LODESTONE_TRACKER);
pub const BEES: ComponentKey<Bees> = ComponentKey::new(DataComponentType::BEES);
pub const MAP_DECORATIONS: ComponentKey<MapDecorations> =
    ComponentKey::new(DataComponentType::MAP_DECORATIONS);

// ---- stream helpers (`ByteBufCodecs.list/optional/stringUtf8`) -----------------------------------------------------

/// `ByteBufCodecs.list(max)` / `collection` decode.
pub(crate) fn read_list<T>(
    input: &mut &[u8],
    max: usize,
    mut f: impl FnMut(&mut &[u8]) -> DecodeResult<T>,
) -> DecodeResult<Vec<T>> {
    let count = read_count(input, max)?;
    let mut v = Vec::with_capacity(count.min(65536));
    for _ in 0..count {
        v.push(f(input)?);
    }
    Ok(v)
}

/// `ByteBufCodecs.list(..)` encode.
pub(crate) fn write_list<T>(
    out: &mut Vec<u8>,
    max: usize,
    items: &[T],
    mut f: impl FnMut(&T, &mut Vec<u8>) -> EncodeResult<()>,
) -> EncodeResult<()> {
    crate::codec::write_count(out, items.len(), max)?;
    for i in items {
        f(i, out)?;
    }
    Ok(())
}

/// `ByteBufCodecs.optional(codec)` decode.
pub(crate) fn read_optional<T>(
    input: &mut &[u8],
    f: impl FnOnce(&mut &[u8]) -> DecodeResult<T>,
) -> DecodeResult<Option<T>> {
    if bool::decode(input)? {
        f(input).map(Some)
    } else {
        Ok(None)
    }
}

/// `ByteBufCodecs.optional(codec)` encode.
pub(crate) fn write_optional<T>(
    out: &mut Vec<u8>,
    v: &Option<T>,
    f: impl FnOnce(&T, &mut Vec<u8>) -> EncodeResult<()>,
) -> EncodeResult<()> {
    match v {
        Some(v) => {
            out.push(1);
            f(v, out)?;
        }
        None => out.push(0),
    }
    Ok(())
}

// ---- Fraction (commons-lang3) ------------------------------------------------------------------------------------

/// `org.apache.commons.lang3.math.Fraction` (only what the bundle weight needs); `Err` = `ArithmeticException`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fraction {
    num: i32,
    den: i32,
}

/// `ArithmeticException` as `Err(())`.
pub type Arith<T> = Result<T, ()>;

fn mul_and_check(x: i32, y: i32) -> Arith<i32> {
    i32::try_from(i64::from(x) * i64::from(y)).map_err(|_| ())
}
fn mul_pos_and_check(x: i32, y: i32) -> Arith<i32> {
    // `m > Integer.MAX_VALUE` only
    let m = i64::from(x) * i64::from(y);
    if m > i64::from(i32::MAX) {
        Err(())
    } else {
        Ok(m as i32)
    }
}
fn add_and_check(x: i32, y: i32) -> Arith<i32> {
    i32::try_from(i64::from(x) + i64::from(y)).map_err(|_| ())
}

fn gcd(mut u: i32, mut v: i32) -> Arith<i32> {
    if u == 0 || v == 0 {
        if u == i32::MIN || v == i32::MIN {
            return Err(());
        }
        return Ok(u.abs() + v.abs());
    }
    if u.unsigned_abs() == 1 || v.unsigned_abs() == 1 {
        return Ok(1);
    }
    if u > 0 {
        u = -u;
    }
    if v > 0 {
        v = -v;
    }
    let mut k = 0;
    while (u & 1) == 0 && (v & 1) == 0 && k < 31 {
        u /= 2;
        v /= 2;
        k += 1;
    }
    if k == 31 {
        return Err(());
    }
    let mut t = if (u & 1) == 1 { v } else { -(u / 2) };
    loop {
        while (t & 1) == 0 {
            t /= 2;
        }
        if t > 0 {
            u = -t;
        } else {
            v = t;
        }
        t = (v - u) / 2;
        if t == 0 {
            break;
        }
    }
    Ok(-u * (1 << k))
}

impl Fraction {
    pub const ZERO: Fraction = Fraction { num: 0, den: 1 };
    pub const ONE: Fraction = Fraction { num: 1, den: 1 };

    /// `Fraction.getFraction(n, d)` (not reduced).
    pub fn get(mut num: i32, mut den: i32) -> Arith<Fraction> {
        if den == 0 {
            return Err(());
        }
        if den < 0 {
            if num == i32::MIN || den == i32::MIN {
                return Err(());
            }
            num = -num;
            den = -den;
        }
        Ok(Fraction { num, den })
    }

    pub fn numerator(self) -> i32 {
        self.num
    }
    pub fn denominator(self) -> i32 {
        self.den
    }

    fn reduced(mut num: i32, mut den: i32) -> Arith<Fraction> {
        if den == 0 {
            return Err(());
        }
        if num == 0 {
            return Ok(Fraction::ZERO);
        }
        if den == i32::MIN && (num & 1) == 0 {
            num /= 2;
            den /= 2;
        }
        if den < 0 {
            if num == i32::MIN || den == i32::MIN {
                return Err(());
            }
            num = -num;
            den = -den;
        }
        let g = gcd(num, den)?;
        Ok(Fraction {
            num: num / g,
            den: den / g,
        })
    }

    /// `negate`.
    pub fn negate(self) -> Arith<Fraction> {
        if self.num == i32::MIN {
            return Err(());
        }
        Ok(Fraction {
            num: -self.num,
            den: self.den,
        })
    }

    /// `invert`.
    pub fn invert(self) -> Arith<Fraction> {
        if self.num == 0 || self.num == i32::MIN {
            return Err(());
        }
        Ok(if self.num < 0 {
            Fraction {
                num: -self.den,
                den: -self.num,
            }
        } else {
            Fraction {
                num: self.den,
                den: self.num,
            }
        })
    }

    /// `addSub(other, isAdd)`.
    fn add_sub(self, other: Fraction, is_add: bool) -> Arith<Fraction> {
        if self.num == 0 {
            return if is_add { Ok(other) } else { other.negate() };
        }
        if other.num == 0 {
            return Ok(self);
        }
        let d1 = gcd(self.den, other.den)?;
        if d1 == 1 {
            let uvp = mul_and_check(self.num, other.den)?;
            let upv = mul_and_check(other.num, self.den)?;
            let n = if is_add {
                add_and_check(uvp, upv)?
            } else {
                i32::try_from(i64::from(uvp) - i64::from(upv)).map_err(|_| ())?
            };
            return Ok(Fraction {
                num: n,
                den: mul_pos_and_check(self.den, other.den)?,
            });
        }
        let uvp = i128::from(self.num) * i128::from(other.den / d1);
        let upv = i128::from(other.num) * i128::from(self.den / d1);
        let t = if is_add { uvp + upv } else { uvp - upv };
        let tmodd1 = t.rem_euclid(i128::from(d1)) as i32;
        let d2 = if tmodd1 == 0 { d1 } else { gcd(tmodd1, d1)? };
        let w = t / i128::from(d2);
        let num = i32::try_from(w).map_err(|_| ())?;
        Ok(Fraction {
            num,
            den: mul_pos_and_check(self.den / d1, other.den / d2)?,
        })
    }

    /// `add`.
    pub fn add(self, other: Fraction) -> Arith<Fraction> {
        self.add_sub(other, true)
    }

    /// `subtract`.
    pub fn subtract(self, other: Fraction) -> Arith<Fraction> {
        self.add_sub(other, false)
    }

    /// `multiplyBy`.
    pub fn multiply_by(self, other: Fraction) -> Arith<Fraction> {
        if self.num == 0 || other.num == 0 {
            return Ok(Fraction::ZERO);
        }
        let d1 = gcd(self.num, other.den)?;
        let d2 = gcd(other.num, self.den)?;
        Fraction::reduced(
            mul_and_check(self.num / d1, other.num / d2)?,
            mul_pos_and_check(self.den / d2, other.den / d1)?,
        )
    }

    /// `divideBy`.
    pub fn divide_by(self, other: Fraction) -> Arith<Fraction> {
        if other.num == 0 {
            return Err(());
        }
        self.multiply_by(other.invert()?)
    }

    /// `intValue()` (truncating division).
    pub fn int_value(self) -> i32 {
        self.num / self.den
    }

    /// `compareTo`.
    pub fn compare_to(self, other: Fraction) -> std::cmp::Ordering {
        if self == other {
            return std::cmp::Ordering::Equal;
        }
        (i64::from(self.num) * i64::from(other.den))
            .cmp(&(i64::from(other.num) * i64::from(self.den)))
    }
}

// ---- ItemStackTemplate as an `ItemInstance` -------------------------------------------------------------------------

/// `ItemInstance.getMaxStackSize()` of a template: the patched `max_stack_size` (default 1).
pub(crate) fn template_max_stack_size(t: &ItemStackTemplate) -> i32 {
    t.components
        .get(item_prototype(t.item), &super::basic::MAX_STACK_SIZE)
        .copied()
        .unwrap_or(1)
}

/// `ItemStack.validateContainedItemSizes`.
pub(crate) fn validate_contained_item_sizes<'a>(
    items: impl IntoIterator<Item = &'a ItemStackTemplate>,
) -> Result<(), String> {
    for item in items {
        let max = template_max_stack_size(item);
        if item.count > max {
            return Err(format!(
                "Item stack with count of {} was larger than maximum: {max}",
                item.count
            ));
        }
    }
    Ok(())
}

// ---- ItemContainerContents ---------------------------------------------------------------------------------------

/// `ItemContainerContents`: up to 256 slots, each empty or an item stack template.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ItemContainerContents {
    items: Vec<Option<ItemStackTemplate>>,
}

struct Slot {
    slot: i32,
    item: ItemStackTemplate,
}

impl ItemContainerContents {
    pub const MAX_SIZE: usize = 256;

    /// `ItemContainerContents.fromItems`-shaped constructor from already converted slots (no trailing empties expected).
    pub fn from_items(items: Vec<Option<ItemStackTemplate>>) -> ItemContainerContents {
        ItemContainerContents { items }
    }
    pub fn size(&self) -> usize {
        self.items.len()
    }
    pub fn items(&self) -> &[Option<ItemStackTemplate>] {
        &self.items
    }
    /// `nonEmptyItems()`.
    pub fn non_empty_items(&self) -> impl Iterator<Item = &ItemStackTemplate> {
        self.items.iter().flatten()
    }

    /// `fromItems(itemStacks)`.
    pub fn from_item_stacks(item_stacks: &[ItemStack]) -> ItemContainerContents {
        let Some(last) = item_stacks.iter().rposition(|s| !s.is_empty()) else {
            return ItemContainerContents::default();
        };
        let items = item_stacks[..=last]
            .iter()
            .map(|s| {
                if s.is_empty() {
                    None
                } else {
                    ItemStackTemplate::from_stack(s).ok()
                }
            })
            .collect();
        ItemContainerContents { items }
    }

    /// `copyInto(destination)`.
    pub fn copy_into(&self, destination: &mut [ItemStack]) {
        for (i, d) in destination.iter_mut().enumerate() {
            *d = self
                .items
                .get(i)
                .and_then(Option::as_ref)
                .map_or_else(ItemStack::empty, ItemStackTemplate::create);
        }
    }

    fn from_slots(slots: Vec<Slot>) -> ItemContainerContents {
        let Some(max) = slots.iter().map(|s| s.slot).max() else {
            return ItemContainerContents::default();
        };
        let mut items = vec![None; max as usize + 1];
        for s in slots {
            items[s.slot as usize] = Some(s.item);
        }
        ItemContainerContents { items }
    }

    fn as_slots(&self) -> Vec<Slot> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                t.clone().map(|item| Slot {
                    slot: i as i32,
                    item,
                })
            })
            .collect()
    }

    /// `ItemContainerContents.CODEC`.
    pub fn codec() -> Codec<ItemContainerContents> {
        let slot = record!(Slot {
            slot: int_range(0, 255).field_of("slot"),
            item: ItemStackTemplate::codec().field_of("item"),
        })
        .codec();
        slot.size_limited_list_of(256).xmap(
            ItemContainerContents::from_slots,
            |c: &ItemContainerContents| c.as_slots(),
        )
    }
}

impl Encode for ItemContainerContents {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_list(out, 256, &self.items, |t, o| {
            write_optional(o, t, |t, o| t.encode(o))
        })
    }
}
impl Decode for ItemContainerContents {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, 256, |i| read_optional(i, ItemStackTemplate::decode))
            .map(|items| ItemContainerContents { items })
    }
}

// ---- BundleContents ------------------------------------------------------------------------------------------------

/// `BundleContents` (the persistent/network data only).
#[derive(Clone, Debug)]
pub struct BundleContents {
    items: Vec<ItemStackTemplate>,
    /// Runtime-only selected index (not part of the codecs or `equals`); `-1` = none.
    selected_item: i32,
}

impl Default for BundleContents {
    fn default() -> Self {
        BundleContents::new(Vec::new())
    }
}

/// `equals` compares the items only.
impl PartialEq for BundleContents {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl BundleContents {
    pub const NO_SELECTED_ITEM_INDEX: i32 = -1;

    pub fn new(items: Vec<ItemStackTemplate>) -> BundleContents {
        BundleContents {
            items,
            selected_item: -1,
        }
    }
    /// The private `BundleContents(items, selectedItem)` constructor.
    pub fn with_selected_item(items: Vec<ItemStackTemplate>, selected_item: i32) -> BundleContents {
        BundleContents {
            items,
            selected_item,
        }
    }
    /// `getSelectedItemIndex()`.
    pub fn selected_item_index(&self) -> i32 {
        self.selected_item
    }
    /// `getSelectedItem()`.
    pub fn selected_item(&self) -> Option<&ItemStackTemplate> {
        usize::try_from(self.selected_item)
            .ok()
            .and_then(|i| self.items.get(i))
    }
    /// `weight()` as the `Fraction` (`Err(message)`: nested error or "Excessive total bundle weight").
    pub fn weight_fraction(&self) -> Result<Fraction, String> {
        match self.weight_inner() {
            Err(()) => Err("Excessive total bundle weight".into()),
            Ok(r) => r,
        }
    }
    /// `getWeight(item)` of a single contained item.
    pub fn item_weight_fraction(item: &ItemStackTemplate) -> Result<Fraction, String> {
        match Self::item_weight(item) {
            Err(()) => Err("Excessive total bundle weight".into()),
            Ok(r) => r,
        }
    }
    pub fn items(&self) -> &[ItemStackTemplate] {
        &self.items
    }
    pub fn size(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// `getWeight(item)`: nested bundle, bees (a hive weighs 1) or `1 / maxStackSize`.
    fn item_weight(item: &ItemStackTemplate) -> Result<Result<Fraction, String>, ()> {
        let proto = item_prototype(item.item);
        if let Some(bundle) = item.components.get(proto, &BUNDLE_CONTENTS) {
            return match bundle.weight_inner()? {
                Ok(w) => Ok(Ok(w.add(Fraction::get(1, 16)?)?)),
                Err(e) => Ok(Err(e)),
            };
        }
        if item
            .components
            .get(proto, &BEES)
            .is_some_and(|b| !b.bees.is_empty())
        {
            return Ok(Ok(Fraction::ONE));
        }
        Ok(Ok(Fraction::get(1, template_max_stack_size(item))?))
    }

    /// `computeContentWeight`; the outer `Err` is the `ArithmeticException`.
    fn weight_inner(&self) -> Result<Result<Fraction, String>, ()> {
        let mut weight = Fraction::ZERO;
        for stack in &self.items {
            match Self::item_weight(stack)? {
                Err(e) => return Ok(Err(e)),
                Ok(w) => weight = weight.add(w.multiply_by(Fraction::get(stack.count, 1)?)?)?,
            }
        }
        Ok(Ok(weight))
    }

    /// `weight()`: `Ok(())` when the total weight is computable (the value itself is not needed by codecs).
    pub fn weight(&self) -> Result<(), String> {
        match self.weight_inner() {
            Err(()) => Err("Excessive total bundle weight".into()),
            Ok(r) => r.map(|_| ()),
        }
    }

    /// `BundleContents.CODEC`.
    pub fn codec() -> Codec<BundleContents> {
        ItemStackTemplate::codec()
            .list_of()
            .xmap(BundleContents::new, |c: &BundleContents| c.items.clone())
    }
}

impl Encode for BundleContents {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_list(out, i32::MAX as usize, &self.items, |t, o| t.encode(o))?;
        Ok(())
    }
}
impl Decode for BundleContents {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, i32::MAX as usize, ItemStackTemplate::decode).map(BundleContents::new)
    }
}

// ---- ChargedProjectiles --------------------------------------------------------------------------------------------

/// `ChargedProjectiles`: at most 1024 item stack templates.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChargedProjectiles {
    pub items: Vec<ItemStackTemplate>,
}

impl ChargedProjectiles {
    pub const MAX_SIZE: usize = 1024;

    pub fn codec() -> Codec<ChargedProjectiles> {
        ItemStackTemplate::codec().size_limited_list_of(1024).xmap(
            |items| ChargedProjectiles { items },
            |c: &ChargedProjectiles| c.items.clone(),
        )
    }
}

impl Encode for ChargedProjectiles {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_list(out, 1024, &self.items, |t, o| t.encode(o))?;
        Ok(())
    }
}
impl Decode for ChargedProjectiles {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, 1024, ItemStackTemplate::decode).map(|items| ChargedProjectiles { items })
    }
}

// ---- SeededContainerLoot -------------------------------------------------------------------------------------------

/// `SeededContainerLoot` (no explicit stream codec: NBT fallback).
#[derive(Clone, Debug, PartialEq)]
pub struct SeededContainerLoot {
    pub loot_table: Identifier,
    pub seed: i64,
}

impl SeededContainerLoot {
    pub fn codec() -> Codec<SeededContainerLoot> {
        record!(SeededContainerLoot {
            loot_table: IDENTIFIER.field_of("loot_table"),
            seed: LONG.optional_field_of_default("seed", 0),
        })
        .codec()
    }
}

// ---- BlockItemStateProperties --------------------------------------------------------------------------------------

/// `BlockItemStateProperties`: property name -> value (a `Map`: equality ignores order).
#[derive(Clone, Debug, Default)]
pub struct BlockItemStateProperties {
    pub properties: Vec<(String, String)>,
}

impl PartialEq for BlockItemStateProperties {
    fn eq(&self, other: &Self) -> bool {
        map_eq(&self.properties, &other.properties)
    }
}

/// `Map.equals` over insertion-ordered pairs with unique keys.
fn map_eq<K: PartialEq, V: PartialEq>(a: &[(K, V)], b: &[(K, V)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
}

impl BlockItemStateProperties {
    pub fn codec() -> Codec<BlockItemStateProperties> {
        STRING.unbounded_map(&STRING).xmap(
            |properties| BlockItemStateProperties { properties },
            |b: &BlockItemStateProperties| b.properties.clone(),
        )
    }
}

impl Encode for BlockItemStateProperties {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_list(out, i32::MAX as usize, &self.properties, |(k, v), o| {
            write_utf(o, k, MAX_STRING_LENGTH)?;
            write_utf(o, v, MAX_STRING_LENGTH)
        })
    }
}
impl Decode for BlockItemStateProperties {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, i32::MAX as usize)?;
        // `ByteBufCodecs.map(Object2ObjectOpenHashMap::new, ..)`: the vanilla map iterates in fastutil table order
        let mut map =
            mc_core::java_collections::FastOpenHashMap::new(count.min(65536), |k: &String| {
                mc_core::math::java::java_string_hash(k)
            });
        for _ in 0..count {
            let k = read_utf(input, MAX_STRING_LENGTH)?;
            let v = read_utf(input, MAX_STRING_LENGTH)?;
            map.put(k, v);
        }
        Ok(BlockItemStateProperties {
            properties: map.into_entries(),
        })
    }
}

// ---- SulfurCubeContent ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SulfurCubeContent {
    pub absorbed_block_item_stack: ItemStackTemplate,
}

impl SulfurCubeContent {
    pub fn codec() -> Codec<SulfurCubeContent> {
        ItemStackTemplate::codec().xmap(
            |t| SulfurCubeContent {
                absorbed_block_item_stack: t,
            },
            |c: &SulfurCubeContent| c.absorbed_block_item_stack.clone(),
        )
    }
}

impl Encode for SulfurCubeContent {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.absorbed_block_item_stack.encode(out)?;
        Ok(())
    }
}
impl Decode for SulfurCubeContent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        ItemStackTemplate::decode(input).map(|t| SulfurCubeContent {
            absorbed_block_item_stack: t,
        })
    }
}

// ---- DebugStickState -----------------------------------------------------------------------------------------------

/// `DebugStickState`: block -> the name of the block property last selected (no stream codec: NBT fallback).
#[derive(Clone, Debug, Default)]
pub struct DebugStickState {
    pub properties: Vec<(Block, String)>,
}

impl PartialEq for DebugStickState {
    fn eq(&self, other: &Self) -> bool {
        map_eq(&self.properties, &other.properties)
    }
}

impl DebugStickState {
    pub fn codec() -> Codec<DebugStickState> {
        mc_dfu::extra::dispatched_map(&registry_by_name_codec::<Block>(), |block: &Block| {
            let block = *block;
            DataResult::success(STRING.comap_flat_map(
                move |name| {
                    if block.properties().any(|p| p.name == name) {
                        DataResult::success(name)
                    } else {
                        let m = format!("No property on {} with name: {name}", block.name());
                        DataResult::error(move || m.clone())
                    }
                },
                |name: &String| name.clone(),
            ))
        })
        .xmap(
            |properties| DebugStickState { properties },
            |s: &DebugStickState| s.properties.clone(),
        )
    }
}

// ---- LockCode ------------------------------------------------------------------------------------------------------

/// `LockCode` (no explicit stream codec: NBT fallback).
#[derive(Clone, Debug, PartialEq)]
pub struct LockCode {
    pub predicate: ItemPredicate,
}

impl LockCode {
    pub fn codec() -> Codec<LockCode> {
        ItemPredicate::codec().xmap(
            |predicate| LockCode { predicate },
            |c: &LockCode| c.predicate.clone(),
        )
    }
}

// ---- recipes -------------------------------------------------------------------------------------------------------

/// `Recipe.KEY_CODEC.listOf()`.
pub fn recipes_codec() -> Codec<Vec<Identifier>> {
    IDENTIFIER.list_of()
}

// ---- LodestoneTracker ----------------------------------------------------------------------------------------------

/// `BlockPos.CODEC`: three ints.
pub fn block_pos_codec() -> Codec<BlockPos> {
    INT_STREAM.comap_flat_map(
        |v: Vec<i32>| {
            if v.len() == 3 {
                DataResult::success(BlockPos::new(v[0], v[1], v[2]))
            } else {
                DataResult::error_str("Input is not a list of 3 ints")
            }
        },
        |p: &BlockPos| vec![p.x(), p.y(), p.z()],
    )
}

/// `GlobalPos.CODEC`.
pub fn global_pos_codec() -> Codec<GlobalPos> {
    record!(GlobalPos {
        dimension: IDENTIFIER.field_of("dimension"),
        pos: block_pos_codec().field_of("pos"),
    })
    .codec()
}

#[derive(Clone, Debug, PartialEq)]
pub struct LodestoneTracker {
    pub target: Option<GlobalPos>,
    pub tracked: bool,
}

impl LodestoneTracker {
    pub fn codec() -> Codec<LodestoneTracker> {
        record!(LodestoneTracker {
            target: global_pos_codec().optional_field_of("target"),
            tracked: BOOL.optional_field_of_default("tracked", true),
        })
        .codec()
    }
}

impl Encode for LodestoneTracker {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_optional(out, &self.target, |t, o| t.encode(o))?;
        self.tracked.encode(out)?;
        Ok(())
    }
}
impl Decode for LodestoneTracker {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let target = read_optional(input, GlobalPos::decode)?;
        Ok(LodestoneTracker {
            target,
            tracked: bool::decode(input)?,
        })
    }
}

// ---- MapDecorations ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct MapDecorationEntry {
    pub ty: MapDecorationType,
    pub x: f64,
    pub z: f64,
    pub rotation: f32,
}

#[derive(Clone, Debug, Default)]
pub struct MapDecorations {
    pub decorations: Vec<(String, MapDecorationEntry)>,
}

impl PartialEq for MapDecorations {
    fn eq(&self, other: &Self) -> bool {
        map_eq(&self.decorations, &other.decorations)
    }
}

impl MapDecorations {
    pub fn codec() -> Codec<MapDecorations> {
        let entry = record!(MapDecorationEntry {
            ty: registry_by_name_codec::<MapDecorationType>().field_of("type"),
            x: DOUBLE.field_of("x"),
            z: DOUBLE.field_of("z"),
            rotation: FLOAT.field_of("rotation"),
        })
        .codec();
        STRING.unbounded_map(&entry).xmap(
            |decorations| MapDecorations { decorations },
            |m: &MapDecorations| m.decorations.clone(),
        )
    }
}

// ---- Bees ----------------------------------------------------------------------------------------------------------

/// `BeehiveBlockEntity.Occupant`.
#[derive(Clone, Debug, PartialEq)]
pub struct BeeOccupant {
    pub entity_data: TypedEntityData<EntityType>,
    pub ticks_in_hive: i32,
    pub min_ticks_in_hive: i32,
}

impl BeeOccupant {
    pub fn codec() -> Codec<BeeOccupant> {
        record!(BeeOccupant {
            entity_data: TypedEntityData::codec(&registry_by_name_codec::<EntityType>())
                .field_of("entity_data"),
            ticks_in_hive: INT.field_of("ticks_in_hive"),
            min_ticks_in_hive: INT.field_of("min_ticks_in_hive"),
        })
        .codec()
    }
}

impl Encode for BeeOccupant {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.entity_data.encode(out)?;
        write_varint(out, self.ticks_in_hive);
        write_varint(out, self.min_ticks_in_hive);
        Ok(())
    }
}
impl Decode for BeeOccupant {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(BeeOccupant {
            entity_data: TypedEntityData::decode(input)?,
            ticks_in_hive: crate::codec::read_varint(input)?,
            min_ticks_in_hive: crate::codec::read_varint(input)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Bees {
    pub bees: Vec<BeeOccupant>,
}

impl Bees {
    pub fn codec() -> Codec<Bees> {
        BeeOccupant::codec()
            .list_of()
            .xmap(|bees| Bees { bees }, |b: &Bees| b.bees.clone())
    }
}

impl Encode for Bees {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_list(out, i32::MAX as usize, &self.bees, |b, o| b.encode(o))
    }
}
impl Decode for Bees {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, i32::MAX as usize, BeeOccupant::decode).map(|bees| Bees { bees })
    }
}

// ---- definitions ---------------------------------------------------------------------------------------------------

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(CONTAINER)
            .persistent(ItemContainerContents::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BUNDLE_CONTENTS)
            .persistent(BundleContents::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(CHARGED_PROJECTILES)
            .persistent(ChargedProjectiles::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(CONTAINER_LOOT)
            .persistent(SeededContainerLoot::codec())
            .build(),
    );
    out.push(
        ComponentDef::builder(BLOCK_STATE)
            .persistent(BlockItemStateProperties::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(SULFUR_CUBE_CONTENT)
            .persistent(SulfurCubeContent::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(DEBUG_STICK_STATE)
            .persistent(DebugStickState::codec())
            .build(),
    );
    out.push(
        ComponentDef::builder(LOCK)
            .persistent(LockCode::codec())
            .build(),
    );
    out.push(
        ComponentDef::builder(RECIPES)
            .persistent(recipes_codec())
            .build(),
    );
    out.push(
        ComponentDef::builder(LODESTONE_TRACKER)
            .persistent(LodestoneTracker::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BEES)
            .persistent(Bees::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(MAP_DECORATIONS)
            .persistent(MapDecorations::codec())
            .build(),
    );
}
