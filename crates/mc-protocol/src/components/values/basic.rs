//! Basic components: custom data, names/lore, simple scalars, colours, enchantments, tooltip display and the small
//! item records (enchantable, repairable, fuels, ...).

use crate::codec::EncodeResult;
use crate::codec::{
    Decode, DecodeError, DecodeResult, Encode, NbtLimit, decode_tag_with, encode_tag, read_count,
    read_registry_id, read_varint, write_varint,
};
use crate::components::common::{
    DyeColor, Rarity, RegistrySet, SoundEvent, component_codec, compound_tag_codec, read_enum_zero,
};
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::{self, Holder, NoDirect};
use mc_core::ident::Identifier;
use mc_data::{DataComponentType, EntityType, Item};
use mc_dfu::extra::{
    IDENTIFIER, NON_NEGATIVE_FLOAT, NON_NEGATIVE_INT, POSITIVE_INT, RGB_COLOR_CODEC,
    extra_float_range, extra_int_range,
};
use mc_dfu::{BOOL, Codec, DataResult, Either, FLOAT, INT, STRING, UNIT, record};
use mc_nbt::CompoundTag;
use mc_text::{ChatFormatting, Component, Style};

// ---- keys ------------------------------------------------------------------------------------------------------

pub const CUSTOM_DATA: ComponentKey<CustomData> = ComponentKey::new(DataComponentType::CUSTOM_DATA);
pub const MAX_STACK_SIZE: ComponentKey<i32> = ComponentKey::new(DataComponentType::MAX_STACK_SIZE);
pub const MAX_DAMAGE: ComponentKey<i32> = ComponentKey::new(DataComponentType::MAX_DAMAGE);
pub const DAMAGE: ComponentKey<i32> = ComponentKey::new(DataComponentType::DAMAGE);
pub const UNBREAKABLE: ComponentKey<()> = ComponentKey::new(DataComponentType::UNBREAKABLE);
pub const CUSTOM_NAME: ComponentKey<Component> = ComponentKey::new(DataComponentType::CUSTOM_NAME);
pub const MINIMUM_ATTACK_CHARGE: ComponentKey<f32> =
    ComponentKey::new(DataComponentType::MINIMUM_ATTACK_CHARGE);
pub const DAMAGE_TYPE: ComponentKey<Holder<NoDirect>> =
    ComponentKey::new(DataComponentType::DAMAGE_TYPE);
pub const ITEM_NAME: ComponentKey<Component> = ComponentKey::new(DataComponentType::ITEM_NAME);
pub const ITEM_MODEL: ComponentKey<Identifier> = ComponentKey::new(DataComponentType::ITEM_MODEL);
pub const LORE: ComponentKey<ItemLore> = ComponentKey::new(DataComponentType::LORE);
pub const RARITY: ComponentKey<Rarity> = ComponentKey::new(DataComponentType::RARITY);
pub const ENCHANTMENTS: ComponentKey<ItemEnchantments> =
    ComponentKey::new(DataComponentType::ENCHANTMENTS);
pub const CUSTOM_MODEL_DATA: ComponentKey<CustomModelData> =
    ComponentKey::new(DataComponentType::CUSTOM_MODEL_DATA);
pub const TOOLTIP_DISPLAY: ComponentKey<TooltipDisplay> =
    ComponentKey::new(DataComponentType::TOOLTIP_DISPLAY);
pub const REPAIR_COST: ComponentKey<i32> = ComponentKey::new(DataComponentType::REPAIR_COST);
pub const CREATIVE_SLOT_LOCK: ComponentKey<()> =
    ComponentKey::new(DataComponentType::CREATIVE_SLOT_LOCK);
pub const ENCHANTMENT_GLINT_OVERRIDE: ComponentKey<bool> =
    ComponentKey::new(DataComponentType::ENCHANTMENT_GLINT_OVERRIDE);
pub const INTANGIBLE_PROJECTILE: ComponentKey<()> =
    ComponentKey::new(DataComponentType::INTANGIBLE_PROJECTILE);
pub const ENCHANTABLE: ComponentKey<Enchantable> =
    ComponentKey::new(DataComponentType::ENCHANTABLE);
pub const REPAIRABLE: ComponentKey<Repairable> = ComponentKey::new(DataComponentType::REPAIRABLE);
pub const GLIDER: ComponentKey<()> = ComponentKey::new(DataComponentType::GLIDER);
pub const TOOLTIP_STYLE: ComponentKey<Identifier> =
    ComponentKey::new(DataComponentType::TOOLTIP_STYLE);
pub const ADDITIONAL_TRADE_COST: ComponentKey<i32> =
    ComponentKey::new(DataComponentType::ADDITIONAL_TRADE_COST);
pub const BLOCK_TRANSFORMER: ComponentKey<Holder<NoDirect>> =
    ComponentKey::new(DataComponentType::BLOCK_TRANSFORMER);
pub const VILLAGER_FOOD: ComponentKey<VillagerFood> =
    ComponentKey::new(DataComponentType::VILLAGER_FOOD);
pub const STORED_ENCHANTMENTS: ComponentKey<ItemEnchantments> =
    ComponentKey::new(DataComponentType::STORED_ENCHANTMENTS);
pub const DYE: ComponentKey<DyeColor> = ComponentKey::new(DataComponentType::DYE);
pub const DYED_COLOR: ComponentKey<DyedItemColor> =
    ComponentKey::new(DataComponentType::DYED_COLOR);
pub const MAP_ID: ComponentKey<MapId> = ComponentKey::new(DataComponentType::MAP_ID);
pub const MAP_POST_PROCESSING: ComponentKey<MapPostProcessing> =
    ComponentKey::new(DataComponentType::MAP_POST_PROCESSING);
pub const POTION_DURATION_SCALE: ComponentKey<f32> =
    ComponentKey::new(DataComponentType::POTION_DURATION_SCALE);
pub const BUCKET_ENTITY_DATA: ComponentKey<CustomData> =
    ComponentKey::new(DataComponentType::BUCKET_ENTITY_DATA);
pub const OMINOUS_BOTTLE_AMPLIFIER: ComponentKey<OminousBottleAmplifier> =
    ComponentKey::new(DataComponentType::OMINOUS_BOTTLE_AMPLIFIER);
pub const NOTE_BLOCK_SOUND: ComponentKey<Identifier> =
    ComponentKey::new(DataComponentType::NOTE_BLOCK_SOUND);
pub const BASE_COLOR: ComponentKey<DyeColor> = ComponentKey::new(DataComponentType::BASE_COLOR);
pub const BREAK_SOUND: ComponentKey<Holder<SoundEvent>> =
    ComponentKey::new(DataComponentType::BREAK_SOUND);
pub const COMPOSTABLE: ComponentKey<Compostable> =
    ComponentKey::new(DataComponentType::COMPOSTABLE);
pub const COOKING_FUEL: ComponentKey<CookingFuel> =
    ComponentKey::new(DataComponentType::COOKING_FUEL);
pub const BREWING_FUEL: ComponentKey<BrewingFuel> =
    ComponentKey::new(DataComponentType::BREWING_FUEL);
pub const MOB_VISIBILITY: ComponentKey<MobVisibility> =
    ComponentKey::new(DataComponentType::MOB_VISIBILITY);
pub const WOLF_COLLAR: ComponentKey<DyeColor> = ComponentKey::new(DataComponentType::WOLF_COLLAR);
pub const TROPICAL_FISH_BASE_COLOR: ComponentKey<DyeColor> =
    ComponentKey::new(DataComponentType::TROPICAL_FISH_BASE_COLOR);
pub const TROPICAL_FISH_PATTERN_COLOR: ComponentKey<DyeColor> =
    ComponentKey::new(DataComponentType::TROPICAL_FISH_PATTERN_COLOR);
pub const CAT_COLLAR: ComponentKey<DyeColor> = ComponentKey::new(DataComponentType::CAT_COLLAR);
pub const SHEEP_COLOR: ComponentKey<DyeColor> = ComponentKey::new(DataComponentType::SHEEP_COLOR);
pub const SHULKER_COLOR: ComponentKey<DyeColor> =
    ComponentKey::new(DataComponentType::SHULKER_COLOR);
pub const WAXED: ComponentKey<()> = ComponentKey::new(DataComponentType::WAXED);
pub const CUSHION_COLOR: ComponentKey<DyeColor> =
    ComponentKey::new(DataComponentType::CUSHION_COLOR);

// ---- CustomData ------------------------------------------------------------------------------------------------

/// `CustomData`: an arbitrary compound.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomData(CompoundTag);

impl CustomData {
    /// `CustomData.of(tag)` (copies).
    pub fn of(tag: &CompoundTag) -> CustomData {
        CustomData(tag.clone())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn tag(&self) -> &CompoundTag {
        &self.0
    }
    /// `copyTag()`.
    pub fn copy_tag(&self) -> CompoundTag {
        self.0.clone()
    }
    /// `CustomData.COMPOUND_TAG_CODEC` = `Codec.withAlternative(CompoundTag.CODEC, TagParser.FLATTENED_CODEC)`.
    pub fn compound_tag_codec() -> Codec<CompoundTag> {
        compound_tag_codec()
            .with_alternative(&crate::components::common::flattened_compound_codec())
    }
    /// `CustomData.CODEC`.
    pub fn codec() -> Codec<CustomData> {
        Self::compound_tag_codec().xmap(CustomData, |d: &CustomData| d.0.clone())
    }
}

/// `CustomData.STREAM_CODEC` (`ByteBufCodecs.COMPOUND_TAG`).
impl Encode for CustomData {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.0.encode(out)?;
        Ok(())
    }
}
impl Decode for CustomData {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        CompoundTag::decode(input).map(CustomData)
    }
}

// ---- text components on the wire (`ComponentSerialization.STREAM_CODEC`) -----------------------------------------

/// `ByteBufCodecs.fromCodecWithRegistries(ComponentSerialization.CODEC)` encode.
fn encode_component(c: &Component, out: &mut Vec<u8>) -> EncodeResult<()> {
    let tag = mc_dfu::encode_nbt(&component_codec(), c)
        .into_result()
        .map_err(|e| crate::codec::EncodeError(format!("Failed to encode: {e} {c:?}")))?;
    encode_tag(&tag, out)
}

/// ... decode (default NBT quota).
fn decode_component(input: &mut &[u8]) -> DecodeResult<Component> {
    let tag = decode_tag_with(input, NbtLimit::DEFAULT)?
        .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
    match mc_dfu::parse_nbt(&component_codec(), &tag) {
        DataResult::Success(c) => Ok(c),
        e => Err(DecodeError::Message(format!(
            "Failed to decode: {} {tag}",
            e.error_message().unwrap_or_default()
        ))),
    }
}

// ---- ItemLore --------------------------------------------------------------------------------------------------

/// `ItemLore`: at most 256 lines; the styled lines are derived.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ItemLore {
    lines: Vec<Component>,
}

impl ItemLore {
    pub const MAX_LINES: usize = 256;

    /// # Panics
    /// With more than [`ItemLore::MAX_LINES`] lines (vanilla throws `IllegalArgumentException`).
    pub fn new(lines: Vec<Component>) -> ItemLore {
        assert!(
            lines.len() <= Self::MAX_LINES,
            "Got {} lines, but maximum is 256",
            lines.len()
        );
        ItemLore { lines }
    }
    pub fn lines(&self) -> &[Component] {
        &self.lines
    }
    /// `styledLines`: every line merged with dark purple italics.
    pub fn styled_lines(&self) -> Vec<Component> {
        let style = Style::EMPTY
            .with_color_formatting(Some(ChatFormatting::DarkPurple))
            .with_italic(Some(true));
        self.lines
            .iter()
            .map(|c| mc_text::util::merge_styles(c.clone(), &style))
            .collect()
    }
    pub fn with_line_added(&self, line: Component) -> ItemLore {
        let mut lines = self.lines.clone();
        lines.push(line);
        ItemLore::new(lines)
    }
    pub fn codec() -> Codec<ItemLore> {
        component_codec()
            .size_limited_list_of(Self::MAX_LINES)
            .xmap(ItemLore::new, |l: &ItemLore| l.lines.clone())
    }
}

impl Encode for ItemLore {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        crate::codec::write_count(out, self.lines.len(), Self::MAX_LINES)?;
        for l in &self.lines {
            encode_component(l, out)?;
        }
        Ok(())
    }
}
impl Decode for ItemLore {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, Self::MAX_LINES)?;
        let mut lines = Vec::with_capacity(count);
        for _ in 0..count {
            lines.push(decode_component(input)?);
        }
        Ok(ItemLore { lines })
    }
}

// ---- MapId / MapPostProcessing / OminousBottleAmplifier -----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MapId(pub i32);

impl MapId {
    pub fn id(self) -> i32 {
        self.0
    }
    pub fn key(self) -> String {
        format!("maps/{}", self.0)
    }
    pub fn codec() -> Codec<MapId> {
        INT.xmap(MapId, |m: &MapId| m.0)
    }
}
impl Encode for MapId {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.0);
        Ok(())
    }
}
impl Decode for MapId {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input).map(MapId)
    }
}

/// `MapPostProcessing` (network only, `ByIdMap.OutOfBoundsStrategy.ZERO`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapPostProcessing {
    Lock,
    Scale,
}

impl MapPostProcessing {
    pub const ALL: &'static [MapPostProcessing] =
        &[MapPostProcessing::Lock, MapPostProcessing::Scale];
    pub fn id(self) -> i32 {
        self as i32
    }
}
impl Encode for MapPostProcessing {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.id());
        Ok(())
    }
}
impl Decode for MapPostProcessing {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_enum_zero(input, Self::ALL)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OminousBottleAmplifier {
    pub value: i32,
}

impl OminousBottleAmplifier {
    pub const MIN_AMPLIFIER: i32 = 0;
    pub const MAX_AMPLIFIER: i32 = 4;
    pub fn codec() -> Codec<OminousBottleAmplifier> {
        extra_int_range(0, 4).xmap(
            |value| OminousBottleAmplifier { value },
            |a: &OminousBottleAmplifier| a.value,
        )
    }
}
impl Encode for OminousBottleAmplifier {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.value);
        Ok(())
    }
}
impl Decode for OminousBottleAmplifier {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input).map(|value| OminousBottleAmplifier { value })
    }
}

// ---- CustomModelData / DyedItemColor -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomModelData {
    pub floats: Vec<f32>,
    pub flags: Vec<bool>,
    pub strings: Vec<String>,
    pub colors: Vec<i32>,
}

impl CustomModelData {
    pub fn codec() -> Codec<CustomModelData> {
        record!(CustomModelData {
            floats: FLOAT
                .list_of()
                .optional_field_of_default("floats", Vec::new()),
            flags: BOOL
                .list_of()
                .optional_field_of_default("flags", Vec::new()),
            strings: STRING
                .list_of()
                .optional_field_of_default("strings", Vec::new()),
            colors: RGB_COLOR_CODEC
                .list_of()
                .optional_field_of_default("colors", Vec::new()),
        })
        .codec()
    }
    pub fn get_float(&self, index: i32) -> Option<f32> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.floats.get(i).copied())
    }
    pub fn get_boolean(&self, index: i32) -> Option<bool> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.flags.get(i).copied())
    }
    pub fn get_string(&self, index: i32) -> Option<&str> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.strings.get(i))
            .map(String::as_str)
    }
    pub fn get_color(&self, index: i32) -> Option<i32> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.colors.get(i).copied())
    }
}
impl Encode for CustomModelData {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.floats.encode(out)?;
        self.flags.encode(out)?;
        self.strings.encode(out)?;
        self.colors.encode(out)?;
        Ok(())
    }
}
impl Decode for CustomModelData {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(CustomModelData {
            floats: Vec::decode(input)?,
            flags: Vec::decode(input)?,
            strings: Vec::decode(input)?,
            colors: Vec::decode(input)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DyedItemColor {
    pub rgb: i32,
}

impl DyedItemColor {
    pub const LEATHER_COLOR: i32 = -6265536;
    pub fn codec() -> Codec<DyedItemColor> {
        RGB_COLOR_CODEC.xmap(|rgb| DyedItemColor { rgb }, |d: &DyedItemColor| d.rgb)
    }
}
impl Encode for DyedItemColor {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.rgb.encode(out)?;
        Ok(())
    }
}
impl Decode for DyedItemColor {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        i32::decode(input).map(|rgb| DyedItemColor { rgb })
    }
}

// ---- ItemEnchantments ------------------------------------------------------------------------------------------

pub const ENCHANTMENT_REGISTRY: &str = "minecraft:enchantment";

/// `Holder<Enchantment>` (a reference into the data-driven `enchantment` registry).
pub type EnchantmentHolder = Holder<NoDirect>;

/// `ItemEnchantments`: enchantment -> level (an `Object2IntOpenHashMap`; equality ignores order).
#[derive(Clone, Debug, Default)]
pub struct ItemEnchantments {
    entries: Vec<(EnchantmentHolder, i32)>,
}

impl PartialEq for ItemEnchantments {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|e| other.entries.contains(e))
    }
}

impl ItemEnchantments {
    /// # Panics
    /// On a level outside `0..=255` (vanilla throws `IllegalArgumentException`).
    pub fn new(entries: Vec<(EnchantmentHolder, i32)>) -> ItemEnchantments {
        Self::checked(entries).unwrap_or_else(|e| panic!("{e}"))
    }
    fn checked(entries: Vec<(EnchantmentHolder, i32)>) -> Result<ItemEnchantments, String> {
        for (k, level) in &entries {
            if !(0..=255).contains(level) {
                return Err(format!("Enchantment {k:?} has invalid level {level}"));
            }
        }
        Ok(ItemEnchantments { entries })
    }
    pub fn entries(&self) -> &[(EnchantmentHolder, i32)] {
        &self.entries
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// `getLevel(enchantment)`: 0 when absent.
    pub fn get_level(&self, enchantment: &EnchantmentHolder) -> i32 {
        self.entries
            .iter()
            .find(|(k, _)| k == enchantment)
            .map_or(0, |(_, l)| *l)
    }
    pub fn codec() -> Codec<ItemEnchantments> {
        let level = mc_dfu::int_range(1, 255);
        holder::holder_fixed_codec::<NoDirect>(ENCHANTMENT_REGISTRY)
            .unbounded_map(&level)
            .xmap(
                |entries| ItemEnchantments { entries },
                |e: &ItemEnchantments| e.entries.clone(),
            )
    }
}

impl Encode for ItemEnchantments {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.entries.len() as i32);
        for (k, level) in &self.entries {
            holder::encode_holder_registry(ENCHANTMENT_REGISTRY, k, out)?;
            write_varint(out, *level);
        }
        Ok(())
    }
}
impl Decode for ItemEnchantments {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut entries: Vec<(EnchantmentHolder, i32)> = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            let key = holder::decode_holder_registry(ENCHANTMENT_REGISTRY, input)?;
            let level = read_varint(input)?;
            match entries.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 = level,
                None => entries.push((key, level)),
            }
        }
        Self::checked(entries).map_err(DecodeError::Message)
    }
}

// ---- TooltipDisplay --------------------------------------------------------------------------------------------

/// `TooltipDisplay`: `hidden_components` is a `ReferenceLinkedOpenHashSet` (insertion ordered, no duplicates).
#[derive(Clone, Debug, Default)]
pub struct TooltipDisplay {
    pub hide_tooltip: bool,
    pub hidden_components: Vec<DataComponentType>,
}

impl PartialEq for TooltipDisplay {
    fn eq(&self, other: &Self) -> bool {
        self.hide_tooltip == other.hide_tooltip
            && self.hidden_components.len() == other.hidden_components.len()
            && self
                .hidden_components
                .iter()
                .all(|c| other.hidden_components.contains(c))
    }
}

impl TooltipDisplay {
    pub const DEFAULT: TooltipDisplay = TooltipDisplay {
        hide_tooltip: false,
        hidden_components: Vec::new(),
    };

    pub fn shows(&self, component: DataComponentType) -> bool {
        !self.hide_tooltip && !self.hidden_components.contains(&component)
    }
    pub fn with_hidden(&self, component: DataComponentType, hidden: bool) -> TooltipDisplay {
        let mut out = self.clone();
        let present = out.hidden_components.contains(&component);
        if hidden && !present {
            out.hidden_components.push(component);
        } else if !hidden && present {
            out.hidden_components.retain(|c| *c != component);
        }
        out
    }
    fn dedup(list: Vec<DataComponentType>) -> Vec<DataComponentType> {
        let mut set: Vec<DataComponentType> = Vec::with_capacity(list.len());
        for c in list {
            if !set.contains(&c) {
                set.push(c);
            }
        }
        set
    }
    pub fn codec() -> Codec<TooltipDisplay> {
        let component_set = holder::registry_by_name_codec::<DataComponentType>()
            .list_of()
            .xmap(Self::dedup, |s: &Vec<DataComponentType>| s.clone());
        record!(TooltipDisplay {
            hide_tooltip: BOOL.optional_field_of_default("hide_tooltip", false),
            hidden_components: component_set
                .optional_field_of_default("hidden_components", Vec::new()),
        })
        .codec()
    }
}

impl Encode for TooltipDisplay {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.hide_tooltip.encode(out)?;
        write_varint(out, self.hidden_components.len() as i32);
        for c in &self.hidden_components {
            write_varint(out, c.id() as i32);
        }
        Ok(())
    }
}
impl Decode for TooltipDisplay {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let hide_tooltip = bool::decode(input)?;
        let count = read_count(input, i32::MAX as usize)?;
        let mut set = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            let c = read_registry_id::<DataComponentType>(input)?;
            if !set.contains(&c) {
                set.push(c);
            }
        }
        Ok(TooltipDisplay {
            hide_tooltip,
            hidden_components: set,
        })
    }
}

// ---- small records -----------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Enchantable {
    pub value: i32,
}
impl Enchantable {
    /// # Panics
    /// When `value <= 0`.
    pub fn new(value: i32) -> Enchantable {
        assert!(
            value > 0,
            "Enchantment value must be positive, but was {value}"
        );
        Enchantable { value }
    }
    pub fn codec() -> Codec<Enchantable> {
        record!(Enchantable {
            value: POSITIVE_INT.field_of("value")
        })
        .codec()
    }
}
impl Encode for Enchantable {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.value);
        Ok(())
    }
}
impl Decode for Enchantable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let value = read_varint(input)?;
        if value <= 0 {
            return Err(DecodeError::Message(format!(
                "Enchantment value must be positive, but was {value}"
            )));
        }
        Ok(Enchantable { value })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Repairable {
    pub items: RegistrySet<Item>,
}
impl Repairable {
    pub fn codec() -> Codec<Repairable> {
        record!(Repairable {
            items: RegistrySet::<Item>::codec().field_of("items")
        })
        .codec()
    }
    pub fn is_valid_repair_item(&self, item: Item) -> bool {
        self.items.contains(item)
    }
}
impl Encode for Repairable {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.items.encode_stream(out)?;
        Ok(())
    }
}
impl Decode for Repairable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        RegistrySet::<Item>::decode_stream(input).map(|items| Repairable { items })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VillagerFood {
    pub nutrition: i32,
}
impl VillagerFood {
    pub fn codec() -> Codec<VillagerFood> {
        record!(VillagerFood {
            nutrition: POSITIVE_INT.field_of("nutrition")
        })
        .codec()
    }
}
impl Encode for VillagerFood {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.nutrition);
        Ok(())
    }
}
impl Decode for VillagerFood {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_varint(input).map(|nutrition| VillagerFood { nutrition })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MobVisibility {
    pub targeting_entity_types: RegistrySet<EntityType>,
    pub visibility: f32,
}
impl MobVisibility {
    pub const MIN_VISIBILITY: f32 = 0.0;
    pub const MAX_VISIBILITY: f32 = 10.0;
    pub fn codec() -> Codec<MobVisibility> {
        record!(MobVisibility {
            targeting_entity_types: RegistrySet::<EntityType>::codec()
                .field_of("targeting_entity_types"),
            visibility: extra_float_range(0.0, 10.0).field_of("visibility"),
        })
        .codec()
    }
}
impl Encode for MobVisibility {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.targeting_entity_types.encode_stream(out)?;
        self.visibility.encode(out)?;
        Ok(())
    }
}
impl Decode for MobVisibility {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(MobVisibility {
            targeting_entity_types: RegistrySet::decode_stream(input)?,
            visibility: f32::decode(input)?,
        })
    }
}

// ---- ResolvableInt / ResolvableFloat (loot number providers by reference) -----------------------------------------

/// `ResolvableInt`: a constant, or the key of a `context_int_provider` (not validated, like `ResourceKey.codec`).
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvableInt {
    Constant(i32),
    Reference(Identifier),
}
impl ResolvableInt {
    pub fn codec() -> Codec<ResolvableInt> {
        INT.either(&IDENTIFIER).xmap(
            |e| e.map(ResolvableInt::Constant, ResolvableInt::Reference),
            |r: &ResolvableInt| match r {
                ResolvableInt::Constant(v) => Either::Left(*v),
                ResolvableInt::Reference(k) => Either::Right(k.clone()),
            },
        )
    }
}
impl Encode for ResolvableInt {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            ResolvableInt::Constant(v) => {
                true.encode(out)?;
                v.encode(out)?;
            }
            ResolvableInt::Reference(k) => {
                false.encode(out)?;
                k.encode(out)?;
            }
        };
        Ok(())
    }
}
impl Decode for ResolvableInt {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? {
            i32::decode(input).map(ResolvableInt::Constant)
        } else {
            Identifier::decode(input).map(ResolvableInt::Reference)
        }
    }
}

/// `ResolvableFloat`.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvableFloat {
    Constant(f32),
    Reference(Identifier),
}
impl ResolvableFloat {
    pub fn codec() -> Codec<ResolvableFloat> {
        FLOAT.either(&IDENTIFIER).xmap(
            |e| e.map(ResolvableFloat::Constant, ResolvableFloat::Reference),
            |r: &ResolvableFloat| match r {
                ResolvableFloat::Constant(v) => Either::Left(*v),
                ResolvableFloat::Reference(k) => Either::Right(k.clone()),
            },
        )
    }
}
impl Encode for ResolvableFloat {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            ResolvableFloat::Constant(v) => {
                true.encode(out)?;
                v.encode(out)?;
            }
            ResolvableFloat::Reference(k) => {
                false.encode(out)?;
                k.encode(out)?;
            }
        };
        Ok(())
    }
}
impl Decode for ResolvableFloat {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? {
            f32::decode(input).map(ResolvableFloat::Constant)
        } else {
            Identifier::decode(input).map(ResolvableFloat::Reference)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Compostable {
    pub layers: ResolvableInt,
}
impl Compostable {
    pub fn codec() -> Codec<Compostable> {
        record!(Compostable {
            layers: ResolvableInt::codec().field_of("layers")
        })
        .codec()
    }
}
impl Encode for Compostable {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.layers.encode(out)?;
        Ok(())
    }
}
impl Decode for Compostable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        ResolvableInt::decode(input).map(|layers| Compostable { layers })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CookingFuel {
    pub burn_time: ResolvableInt,
    pub speed_multiplier: ResolvableFloat,
}
impl CookingFuel {
    pub fn codec() -> Codec<CookingFuel> {
        record!(CookingFuel {
            burn_time: ResolvableInt::codec().field_of("burn_time"),
            speed_multiplier: ResolvableFloat::codec().field_of("speed_multiplier"),
        })
        .codec()
    }
}
impl Encode for CookingFuel {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.burn_time.encode(out)?;
        self.speed_multiplier.encode(out)?;
        Ok(())
    }
}
impl Decode for CookingFuel {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(CookingFuel {
            burn_time: ResolvableInt::decode(input)?,
            speed_multiplier: ResolvableFloat::decode(input)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrewingFuel {
    pub uses: ResolvableInt,
    pub speed_multiplier: ResolvableFloat,
}
impl BrewingFuel {
    pub fn codec() -> Codec<BrewingFuel> {
        record!(BrewingFuel {
            uses: ResolvableInt::codec().field_of("uses"),
            speed_multiplier: ResolvableFloat::codec().field_of("speed_multiplier"),
        })
        .codec()
    }
}
impl Encode for BrewingFuel {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.uses.encode(out)?;
        self.speed_multiplier.encode(out)?;
        Ok(())
    }
}
impl Decode for BrewingFuel {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(BrewingFuel {
            uses: ResolvableInt::decode(input)?,
            speed_multiplier: ResolvableFloat::decode(input)?,
        })
    }
}

// ---- definitions -------------------------------------------------------------------------------------------------

fn var_int(key: ComponentKey<i32>, codec: Codec<i32>) -> ComponentDef {
    ComponentDef::builder(key)
        .persistent(codec)
        .network_with(
            |v, out| {
                write_varint(out, *v);
                Ok(())
            },
            |i| read_varint(i),
        )
        .build()
}

fn float(key: ComponentKey<f32>, codec: Codec<f32>) -> ComponentDef {
    ComponentDef::builder(key)
        .persistent(codec)
        .network_with(|v, out| v.encode(out), |i| f32::decode(i))
        .build()
}

fn unit(key: ComponentKey<()>, persistent: bool) -> ComponentDef {
    let b = ComponentDef::builder(key);
    let b = if persistent {
        b.persistent(UNIT.clone())
    } else {
        b
    };
    b.network_with(|_, _| Ok(()), |_| Ok(())).build()
}

fn dye(key: ComponentKey<DyeColor>) -> ComponentDef {
    ComponentDef::builder(key)
        .persistent(DyeColor::codec())
        .network_synchronized()
        .build()
}

fn identifier(key: ComponentKey<Identifier>) -> ComponentDef {
    ComponentDef::builder(key)
        .persistent(IDENTIFIER.clone())
        .network_synchronized()
        .build()
}

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(CUSTOM_DATA)
            .persistent(CustomData::codec())
            .build(),
    );
    out.push(var_int(MAX_STACK_SIZE, extra_int_range(1, 99)));
    out.push(var_int(MAX_DAMAGE, POSITIVE_INT.clone()));
    out.push(var_int(DAMAGE, NON_NEGATIVE_INT.clone()));
    out.push(unit(UNBREAKABLE, true));
    out.push(
        ComponentDef::builder(CUSTOM_NAME)
            .persistent(component_codec())
            .build(),
    );
    out.push(float(MINIMUM_ATTACK_CHARGE, extra_float_range(0.0, 1.0)));
    out.push(
        ComponentDef::builder(DAMAGE_TYPE)
            .persistent(holder::holder_fixed_codec::<NoDirect>(
                "minecraft:damage_type",
            ))
            .network_with(
                |h, out| holder::encode_holder_registry("minecraft:damage_type", h, out),
                |i| holder::decode_holder_registry("minecraft:damage_type", i),
            )
            .build(),
    );
    out.push(
        ComponentDef::builder(ITEM_NAME)
            .persistent(component_codec())
            .build(),
    );
    out.push(identifier(ITEM_MODEL));
    out.push(
        ComponentDef::builder(LORE)
            .persistent(ItemLore::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(RARITY)
            .persistent(Rarity::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(ENCHANTMENTS)
            .persistent(ItemEnchantments::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(CUSTOM_MODEL_DATA)
            .persistent(CustomModelData::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(TOOLTIP_DISPLAY)
            .persistent(TooltipDisplay::codec())
            .network_synchronized()
            .build(),
    );
    out.push(var_int(REPAIR_COST, NON_NEGATIVE_INT.clone()));
    out.push(unit(CREATIVE_SLOT_LOCK, false));
    out.push(
        ComponentDef::builder(ENCHANTMENT_GLINT_OVERRIDE)
            .persistent(BOOL.clone())
            .network_with(|v, out| v.encode(out), |i| bool::decode(i))
            .build(),
    );
    out.push(
        ComponentDef::builder(INTANGIBLE_PROJECTILE)
            .persistent(UNIT.clone())
            .build(),
    );
    out.push(
        ComponentDef::builder(ENCHANTABLE)
            .persistent(Enchantable::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(REPAIRABLE)
            .persistent(Repairable::codec())
            .network_synchronized()
            .build(),
    );
    out.push(unit(GLIDER, true));
    out.push(identifier(TOOLTIP_STYLE));
    out.push(
        ComponentDef::builder(ADDITIONAL_TRADE_COST)
            .network_with(
                |v, out| {
                    write_varint(out, *v);
                    Ok(())
                },
                |i| read_varint(i),
            )
            .build(),
    );
    out.push(
        ComponentDef::builder(BLOCK_TRANSFORMER)
            .persistent(holder::holder_fixed_codec::<NoDirect>(
                "minecraft:block_transformer",
            ))
            .network_with(
                |h, out| holder::encode_holder_registry("minecraft:block_transformer", h, out),
                |i| holder::decode_holder_registry("minecraft:block_transformer", i),
            )
            .build(),
    );
    out.push(
        ComponentDef::builder(VILLAGER_FOOD)
            .persistent(VillagerFood::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(STORED_ENCHANTMENTS)
            .persistent(ItemEnchantments::codec())
            .network_synchronized()
            .build(),
    );
    out.push(dye(DYE));
    out.push(
        ComponentDef::builder(DYED_COLOR)
            .persistent(DyedItemColor::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(MAP_ID)
            .persistent(MapId::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(MAP_POST_PROCESSING)
            .network_synchronized()
            .build(),
    );
    out.push(float(POTION_DURATION_SCALE, NON_NEGATIVE_FLOAT.clone()));
    out.push(
        ComponentDef::builder(BUCKET_ENTITY_DATA)
            .persistent(CustomData::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(OMINOUS_BOTTLE_AMPLIFIER)
            .persistent(OminousBottleAmplifier::codec())
            .network_synchronized()
            .build(),
    );
    out.push(identifier(NOTE_BLOCK_SOUND));
    out.push(dye(BASE_COLOR));
    out.push(
        ComponentDef::builder(BREAK_SOUND)
            .persistent(SoundEvent::holder_codec())
            .network_with(
                |h, out| SoundEvent::encode_holder(h, out),
                |i| SoundEvent::decode_holder(i),
            )
            .build(),
    );
    out.push(
        ComponentDef::builder(COMPOSTABLE)
            .persistent(Compostable::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(COOKING_FUEL)
            .persistent(CookingFuel::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BREWING_FUEL)
            .persistent(BrewingFuel::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(MOB_VISIBILITY)
            .persistent(MobVisibility::codec())
            .network_synchronized()
            .build(),
    );
    for key in [
        WOLF_COLLAR,
        TROPICAL_FISH_BASE_COLOR,
        TROPICAL_FISH_PATTERN_COLOR,
        CAT_COLLAR,
        SHEEP_COLOR,
        SHULKER_COLOR,
        CUSHION_COLOR,
    ] {
        out.push(dye(key));
    }
    out.push(unit(WAXED, true));
}
