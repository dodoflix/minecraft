//! Effect-flavoured components: attribute modifiers, potions, stews, food and consumables, use effects.

use crate::codec::EncodeResult;
use crate::codec::{read_registry_id, write_varint, DecodeResult};
use crate::{Decode, Encode};
use crate::components::common::{read_enum_zero, RegistrySet};
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::{registry_by_name_codec, Holder};
use crate::components::{component_codec, ItemStackTemplate, SoundEvent};
use crate::packets::play::{AttributeModifier, AttributeModifierOperation};
use crate::string_enum;
use mc_core::ident::Identifier;
use mc_data::{Attribute, ConsumeEffectType, DataComponentType, MobEffect, Potion};
use mc_dfu::extra::{IDENTIFIER, NON_NEGATIVE_FLOAT, NON_NEGATIVE_INT, POSITIVE_FLOAT};
use mc_dfu::{float_range, java_double_cmp, java_float_cmp, string_enum_codec, Codec, MapCodec, BOOL, DOUBLE, FLOAT, INT, STRING};
use mc_text::Component;
use std::cmp::Ordering;

pub const ATTRIBUTE_MODIFIERS: ComponentKey<ItemAttributeModifiers> = ComponentKey::new(DataComponentType::ATTRIBUTE_MODIFIERS);
pub const POTION_CONTENTS: ComponentKey<PotionContents> = ComponentKey::new(DataComponentType::POTION_CONTENTS);
pub const SUSPICIOUS_STEW_EFFECTS: ComponentKey<SuspiciousStewEffects> = ComponentKey::new(DataComponentType::SUSPICIOUS_STEW_EFFECTS);
pub const CONSUMABLE: ComponentKey<Consumable> = ComponentKey::new(DataComponentType::CONSUMABLE);
pub const DEATH_PROTECTION: ComponentKey<DeathProtection> = ComponentKey::new(DataComponentType::DEATH_PROTECTION);
pub const FOOD: ComponentKey<FoodProperties> = ComponentKey::new(DataComponentType::FOOD);
pub const USE_EFFECTS: ComponentKey<UseEffects> = ComponentKey::new(DataComponentType::USE_EFFECTS);
pub const USE_COOLDOWN: ComponentKey<UseCooldown> = ComponentKey::new(DataComponentType::USE_COOLDOWN);
pub const USE_REMAINDER: ComponentKey<UseRemainder> = ComponentKey::new(DataComponentType::USE_REMAINDER);

// ---- helpers shared with combat.rs -------------------------------------------------------------------------------

/// `Float.equals` (`Float.compare == 0`).
pub(crate) fn feq(a: f32, b: f32) -> bool {
    java_float_cmp(a, b) == Ordering::Equal
}
fn deq(a: f64, b: f64) -> bool {
    java_double_cmp(a, b) == Ordering::Equal
}

/// A float whose `==` is `Float.equals`, so default omission of `optionalFieldOf(name, default)` matches Java.
#[derive(Clone, Copy)]
struct JF32(f32);
impl PartialEq for JF32 {
    fn eq(&self, o: &JF32) -> bool {
        feq(self.0, o.0)
    }
}

/// `codec.optionalFieldOf(name, default)` for floats, comparing with `Float.equals`.
pub(crate) fn float_field_default(codec: &Codec<f32>, name: &str, default: f32) -> MapCodec<f32> {
    codec.xmap(JF32, |j: &JF32| j.0).optional_field_of_default(name, JF32(default)).xmap(|j| j.0, |f: &f32| JF32(*f))
}

pub(crate) fn ident(s: &str) -> Identifier {
    Identifier::parse(s).expect("valid identifier")
}

pub(crate) fn enc_opt<T>(out: &mut Vec<u8>, v: &Option<T>, f: impl FnOnce(&T, &mut Vec<u8>) -> EncodeResult<()>) -> EncodeResult<()> {
    match v {
        Some(v) => {
            out.push(1);
            f(v, out)?;
        }
        None => out.push(0),
    }
    Ok(())
}
pub(crate) fn dec_opt<T>(input: &mut &[u8], f: impl FnOnce(&mut &[u8]) -> DecodeResult<T>) -> DecodeResult<Option<T>> {
    if bool::decode(input)? { f(input).map(Some) } else { Ok(None) }
}
pub(crate) fn enc_sound_opt(v: &Option<Holder<SoundEvent>>, out: &mut Vec<u8>) -> EncodeResult<()> {
    enc_opt(out, v, |h, o| SoundEvent::encode_holder(h, o))
}
pub(crate) fn dec_sound_opt(input: &mut &[u8]) -> DecodeResult<Option<Holder<SoundEvent>>> {
    dec_opt(input, SoundEvent::decode_holder)
}

fn sound_ref(name: &str) -> Holder<SoundEvent> {
    Holder::Reference(ident(name))
}

// ---- EquipmentSlotGroup / AttributeModifier ------------------------------------------------------------------------

string_enum! {
    /// `EquipmentSlotGroup` (ordinal == id).
    pub enum EquipmentSlotGroup {
        Any = "any", MainHand = "mainhand", OffHand = "offhand", Hand = "hand", Feet = "feet", Legs = "legs",
        Chest = "chest", Head = "head", Armor = "armor", Body = "body", Saddle = "saddle",
    }
}
impl Encode for EquipmentSlotGroup {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.id());
        Ok(())
    }
}
impl Decode for EquipmentSlotGroup {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_enum_zero(input, EquipmentSlotGroup::ALL)
    }
}

static OPERATIONS: [(AttributeModifierOperation, &str); 3] = [
    (AttributeModifierOperation::AddValue, "add_value"),
    (AttributeModifierOperation::AddMultipliedBase, "add_multiplied_base"),
    (AttributeModifierOperation::AddMultipliedTotal, "add_multiplied_total"),
];

/// `AttributeModifier.Operation.CODEC`.
pub fn attribute_operation_codec() -> Codec<AttributeModifierOperation> {
    string_enum_codec(&OPERATIONS)
}

/// `AttributeModifier.MAP_CODEC`.
pub fn attribute_modifier_map_codec() -> MapCodec<AttributeModifier> {
    mc_dfu::record!(AttributeModifier {
        id: IDENTIFIER.field_of("id"),
        amount: DOUBLE.field_of("amount"),
        operation: string_enum_codec(&OPERATIONS).field_of("operation"),
    })
}
/// `AttributeModifier.CODEC`.
pub fn attribute_modifier_codec() -> Codec<AttributeModifier> {
    attribute_modifier_map_codec().codec()
}

fn modifier_eq(a: &AttributeModifier, b: &AttributeModifier) -> bool {
    a.id == b.id && deq(a.amount, b.amount) && a.operation == b.operation
}

// ---- ItemAttributeModifiers ---------------------------------------------------------------------------------------

string_enum! {
    /// `ItemAttributeModifiers.Display.Type` (ordinal == id).
    pub enum ItemAttributeModifiersDisplayType { Default = "default", Hidden = "hidden", Override = "override" }
}

/// `ItemAttributeModifiers.Display`.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemAttributeModifiersDisplay {
    Default,
    Hidden,
    OverrideText(Component),
}

impl ItemAttributeModifiersDisplay {
    fn kind(&self) -> ItemAttributeModifiersDisplayType {
        match self {
            Self::Default => ItemAttributeModifiersDisplayType::Default,
            Self::Hidden => ItemAttributeModifiersDisplayType::Hidden,
            Self::OverrideText(_) => ItemAttributeModifiersDisplayType::Override,
        }
    }
    /// `Display.CODEC`.
    pub fn codec() -> Codec<ItemAttributeModifiersDisplay> {
        ItemAttributeModifiersDisplayType::codec().dispatch(
            "type",
            |d: &ItemAttributeModifiersDisplay| d.kind(),
            |t| match t {
                ItemAttributeModifiersDisplayType::Default => MapCodec::unit(Self::Default),
                ItemAttributeModifiersDisplayType::Hidden => MapCodec::unit(Self::Hidden),
                ItemAttributeModifiersDisplayType::Override => component_codec().field_of("value").xmap(Self::OverrideText, |d: &Self| match d {
                    Self::OverrideText(c) => c.clone(),
                    _ => unreachable!("dispatched by type"),
                }),
            },
        )
    }
}
impl Encode for ItemAttributeModifiersDisplay {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.kind().id());
        if let Self::OverrideText(c) = self {
            c.encode(out)?;
        };
        Ok(())
    }
}
impl Decode for ItemAttributeModifiersDisplay {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(match read_enum_zero(input, ItemAttributeModifiersDisplayType::ALL)? {
            ItemAttributeModifiersDisplayType::Default => Self::Default,
            ItemAttributeModifiersDisplayType::Hidden => Self::Hidden,
            ItemAttributeModifiersDisplayType::Override => Self::OverrideText(Component::decode(input)?),
        })
    }
}

/// `ItemAttributeModifiers.Entry`.
#[derive(Clone, Debug)]
pub struct ItemAttributeModifiersEntry {
    pub attribute: Attribute,
    pub modifier: AttributeModifier,
    pub slot: EquipmentSlotGroup,
    pub display: ItemAttributeModifiersDisplay,
}
impl PartialEq for ItemAttributeModifiersEntry {
    fn eq(&self, o: &Self) -> bool {
        self.attribute == o.attribute && modifier_eq(&self.modifier, &o.modifier) && self.slot == o.slot && self.display == o.display
    }
}
impl ItemAttributeModifiersEntry {
    pub fn codec() -> Codec<ItemAttributeModifiersEntry> {
        mc_dfu::record!(ItemAttributeModifiersEntry {
            attribute: registry_by_name_codec::<Attribute>().field_of("type"),
            modifier: attribute_modifier_map_codec(),
            slot: EquipmentSlotGroup::codec().optional_field_of_default("slot", EquipmentSlotGroup::Any),
            display: ItemAttributeModifiersDisplay::codec().optional_field_of_default("display", ItemAttributeModifiersDisplay::Default),
        })
        .codec()
    }
}
impl Encode for ItemAttributeModifiersEntry {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.attribute.id() as i32);
        self.modifier.encode(out)?;
        self.slot.encode(out)?;
        self.display.encode(out)?;
        Ok(())
    }
}
impl Decode for ItemAttributeModifiersEntry {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(ItemAttributeModifiersEntry {
            attribute: read_registry_id::<Attribute>(input)?,
            modifier: AttributeModifier::decode(input)?,
            slot: EquipmentSlotGroup::decode(input)?,
            display: ItemAttributeModifiersDisplay::decode(input)?,
        })
    }
}

/// `ItemAttributeModifiers`.
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct ItemAttributeModifiers {
    pub modifiers: Vec<ItemAttributeModifiersEntry>,
}
impl ItemAttributeModifiers {
    pub fn codec() -> Codec<ItemAttributeModifiers> {
        ItemAttributeModifiersEntry::codec().list_of().xmap(|modifiers| ItemAttributeModifiers { modifiers }, |v: &ItemAttributeModifiers| v.modifiers.clone())
    }
}

// ---- MobEffectInstance ----------------------------------------------------------------------------------------------

/// `MobEffectInstance.Details` (the effect-independent part; `hidden_effect` recurses).
#[derive(Clone, Debug, PartialEq)]
struct Details {
    amplifier: i32,
    duration: i32,
    ambient: bool,
    show_particles: bool,
    show_icon: bool,
    hidden_effect: Option<Box<Details>>,
}

/// The record body of `Details.MAP_CODEC`, nested effects go through `this`.
struct DetailsFields {
    amplifier: i32,
    duration: i32,
    ambient: bool,
    show_particles: bool,
    show_icon: Option<bool>,
    hidden_effect: Option<Box<Details>>,
}

impl Details {
    fn map_codec_with(this: Codec<Details>) -> MapCodec<Details> {
        let boxed = this.xmap(Box::new, |b: &Box<Details>| (**b).clone());
        mc_dfu::record!(DetailsFields {
            amplifier: mc_dfu::extra::UNSIGNED_BYTE.optional_field_of_default("amplifier", 0),
            duration: INT.optional_field_of_default("duration", 0),
            ambient: BOOL.optional_field_of_default("ambient", false),
            show_particles: BOOL.optional_field_of_default("show_particles", true),
            show_icon: BOOL.optional_field_of("show_icon"),
            hidden_effect: boxed.optional_field_of("hidden_effect"),
        })
        .xmap(
            |r| Details {
                amplifier: r.amplifier,
                duration: r.duration,
                ambient: r.ambient,
                show_particles: r.show_particles,
                show_icon: r.show_icon.unwrap_or(r.show_particles),
                hidden_effect: r.hidden_effect,
            },
            |d: &Details| DetailsFields {
                amplifier: d.amplifier,
                duration: d.duration,
                ambient: d.ambient,
                show_particles: d.show_particles,
                show_icon: Some(d.show_icon),
                hidden_effect: d.hidden_effect.clone(),
            },
        )
    }
    fn codec() -> Codec<Details> {
        Codec::recursive(|this| Details::map_codec_with(this).codec())
    }
    /// `Details.MAP_CODEC`.
    fn map_codec() -> MapCodec<Details> {
        Details::map_codec_with(Details::codec())
    }
}
impl Encode for Details {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.amplifier);
        write_varint(out, self.duration);
        self.ambient.encode(out)?;
        self.show_particles.encode(out)?;
        self.show_icon.encode(out)?;
        enc_opt(out, &self.hidden_effect, |d, o| d.encode(o))?;
        Ok(())
    }
}
impl Decode for Details {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Details {
            amplifier: crate::codec::read_varint(input)?,
            duration: crate::codec::read_varint(input)?,
            ambient: bool::decode(input)?,
            show_particles: bool::decode(input)?,
            show_icon: bool::decode(input)?,
            hidden_effect: dec_opt(input, |i| Details::decode(i).map(Box::new))?,
        })
    }
}

/// `MobEffectInstance` (duration/amplifier/flags; `equals` ignores the hidden effect).
#[derive(Clone, Debug)]
pub struct MobEffectInstance {
    pub effect: MobEffect,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
    pub hidden_effect: Option<Box<MobEffectInstance>>,
}

impl PartialEq for MobEffectInstance {
    fn eq(&self, o: &Self) -> bool {
        self.duration == o.duration
            && self.amplifier == o.amplifier
            && self.ambient == o.ambient
            && self.visible == o.visible
            && self.show_icon == o.show_icon
            && self.effect == o.effect
    }
}

impl MobEffectInstance {
    /// `new MobEffectInstance(effect, duration, amplifier)`.
    pub fn new(effect: MobEffect, duration: i32, amplifier: i32) -> MobEffectInstance {
        MobEffectInstance { effect, duration, amplifier: amplifier.clamp(0, 255), ambient: false, visible: true, show_icon: true, hidden_effect: None }
    }
    fn from_details(effect: MobEffect, d: Details) -> MobEffectInstance {
        MobEffectInstance {
            effect,
            duration: d.duration,
            amplifier: d.amplifier.clamp(0, 255),
            ambient: d.ambient,
            visible: d.show_particles,
            show_icon: d.show_icon,
            hidden_effect: d.hidden_effect.map(|h| Box::new(MobEffectInstance::from_details(effect, *h))),
        }
    }
    fn as_details(&self) -> Details {
        Details {
            amplifier: self.amplifier,
            duration: self.duration,
            ambient: self.ambient,
            show_particles: self.visible,
            show_icon: self.show_icon,
            hidden_effect: self.hidden_effect.as_ref().map(|h| Box::new(h.as_details())),
        }
    }

    /// `MobEffectInstance.CODEC`.
    pub fn codec() -> Codec<MobEffectInstance> {
        struct Parts {
            effect: MobEffect,
            details: Details,
        }
        mc_dfu::record!(Parts {
            effect: registry_by_name_codec::<MobEffect>().field_of("id"),
            details: Details::map_codec(),
        })
        .xmap(|p| MobEffectInstance::from_details(p.effect, p.details), |m: &MobEffectInstance| Parts { effect: m.effect, details: m.as_details() })
        .codec()
    }
}
impl Encode for MobEffectInstance {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.effect.id() as i32);
        self.as_details().encode(out)?;
        Ok(())
    }
}
impl Decode for MobEffectInstance {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let effect = read_registry_id::<MobEffect>(input)?;
        Ok(MobEffectInstance::from_details(effect, Details::decode(input)?))
    }
}

// ---- PotionContents -----------------------------------------------------------------------------------------------

/// `PotionContents`.
#[derive(Clone, Debug, PartialEq)]
pub struct PotionContents {
    pub potion: Option<Potion>,
    pub custom_color: Option<i32>,
    pub custom_effects: Vec<MobEffectInstance>,
    pub custom_name: Option<String>,
}

impl PotionContents {
    pub fn codec() -> Codec<PotionContents> {
        let potion = registry_by_name_codec::<Potion>();
        let full = mc_dfu::record!(PotionContents {
            potion: potion.optional_field_of("potion"),
            custom_color: INT.optional_field_of("custom_color"),
            custom_effects: MobEffectInstance::codec()
                .list_of()
                .optional_field_of_default("custom_effects", Vec::new())
                .xmap(|v| v, |v: &Vec<MobEffectInstance>| without_hidden(v)),
            custom_name: STRING.optional_field_of("custom_name"),
        })
        .codec();
        full.with_alternative_conv(&potion, |p| PotionContents { potion: Some(p), custom_color: None, custom_effects: Vec::new(), custom_name: None })
    }
}
/// `PotionContents.customEffects()` hands out `new MobEffectInstance(copy)` copies, which drop the hidden effect.
fn without_hidden(v: &[MobEffectInstance]) -> Vec<MobEffectInstance> {
    v.iter().map(|e| MobEffectInstance { hidden_effect: None, ..e.clone() }).collect()
}

impl Encode for PotionContents {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        enc_opt(out, &self.potion, |p, o| {
            write_varint(o, p.id() as i32);
            Ok(())
        })?;
        self.custom_color.encode(out)?;
        without_hidden(&self.custom_effects).encode(out)?;
        self.custom_name.encode(out)?;
        Ok(())
    }
}
impl Decode for PotionContents {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(PotionContents {
            potion: dec_opt(input, read_registry_id::<Potion>)?,
            custom_color: Option::<i32>::decode(input)?,
            custom_effects: Vec::<MobEffectInstance>::decode(input)?,
            custom_name: Option::<String>::decode(input)?,
        })
    }
}

// ---- SuspiciousStewEffects ------------------------------------------------------------------------------------------

/// `SuspiciousStewEffects.Entry`.
#[derive(Clone, Debug, PartialEq)]
pub struct SuspiciousStewEffectsEntry {
    pub effect: MobEffect,
    pub duration: i32,
}
impl SuspiciousStewEffectsEntry {
    pub fn codec() -> Codec<SuspiciousStewEffectsEntry> {
        mc_dfu::record!(SuspiciousStewEffectsEntry {
            effect: registry_by_name_codec::<MobEffect>().field_of("id"),
            duration: INT.lenient_optional_field_of_default("duration", 160),
        })
        .codec()
    }
}
impl Encode for SuspiciousStewEffectsEntry {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.effect.id() as i32);
        write_varint(out, self.duration);
        Ok(())
    }
}
impl Decode for SuspiciousStewEffectsEntry {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(SuspiciousStewEffectsEntry { effect: read_registry_id::<MobEffect>(input)?, duration: crate::codec::read_varint(input)? })
    }
}

/// `SuspiciousStewEffects`.
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct SuspiciousStewEffects {
    pub effects: Vec<SuspiciousStewEffectsEntry>,
}
impl SuspiciousStewEffects {
    pub fn codec() -> Codec<SuspiciousStewEffects> {
        SuspiciousStewEffectsEntry::codec().list_of().xmap(|effects| SuspiciousStewEffects { effects }, |v: &SuspiciousStewEffects| v.effects.clone())
    }
}

// ---- ConsumeEffect ---------------------------------------------------------------------------------------------------

/// `ApplyStatusEffectsConsumeEffect`.
#[derive(Clone, Debug)]
pub struct ApplyStatusEffectsConsumeEffect {
    pub effects: Vec<MobEffectInstance>,
    pub probability: f32,
}
impl PartialEq for ApplyStatusEffectsConsumeEffect {
    fn eq(&self, o: &Self) -> bool {
        self.effects == o.effects && feq(self.probability, o.probability)
    }
}
/// `RemoveStatusEffectsConsumeEffect`.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveStatusEffectsConsumeEffect {
    pub effects: RegistrySet<MobEffect>,
}
/// `TeleportRandomlyConsumeEffect`.
#[derive(Clone, Debug)]
pub struct TeleportRandomlyConsumeEffect {
    pub diameter: f32,
    pub directional_particles: bool,
}
impl PartialEq for TeleportRandomlyConsumeEffect {
    fn eq(&self, o: &Self) -> bool {
        feq(self.diameter, o.diameter) && self.directional_particles == o.directional_particles
    }
}
/// `PlaySoundConsumeEffect`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaySoundConsumeEffect {
    pub sound: Holder<SoundEvent>,
}

/// `ConsumeEffect`.
#[derive(Clone, Debug, PartialEq)]
pub enum ConsumeEffect {
    ApplyStatusEffects(ApplyStatusEffectsConsumeEffect),
    RemoveStatusEffects(RemoveStatusEffectsConsumeEffect),
    ClearAllStatusEffects,
    TeleportRandomly(TeleportRandomlyConsumeEffect),
    PlaySound(PlaySoundConsumeEffect),
}

impl ConsumeEffect {
    fn kind(&self) -> ConsumeEffectType {
        match self {
            Self::ApplyStatusEffects(_) => ConsumeEffectType::APPLY_EFFECTS,
            Self::RemoveStatusEffects(_) => ConsumeEffectType::REMOVE_EFFECTS,
            Self::ClearAllStatusEffects => ConsumeEffectType::CLEAR_ALL_EFFECTS,
            Self::TeleportRandomly(_) => ConsumeEffectType::TELEPORT_RANDOMLY,
            Self::PlaySound(_) => ConsumeEffectType::PLAY_SOUND,
        }
    }

    /// `ConsumeEffect.CODEC`.
    pub fn codec() -> Codec<ConsumeEffect> {
        registry_by_name_codec::<ConsumeEffectType>().dispatch("type", |e: &ConsumeEffect| e.kind(), |t| {
            if *t == ConsumeEffectType::APPLY_EFFECTS {
                mc_dfu::record!(ApplyStatusEffectsConsumeEffect {
                    effects: MobEffectInstance::codec().list_of().field_of("effects"),
                    probability: float_field_default(&float_range(0.0, 1.0), "probability", 1.0),
                })
                .xmap(Self::ApplyStatusEffects, |e: &Self| match e {
                    Self::ApplyStatusEffects(x) => x.clone(),
                    _ => unreachable!("dispatched by type"),
                })
            } else if *t == ConsumeEffectType::REMOVE_EFFECTS {
                mc_dfu::record!(RemoveStatusEffectsConsumeEffect { effects: RegistrySet::<MobEffect>::codec().field_of("effects") }).xmap(
                    Self::RemoveStatusEffects,
                    |e: &Self| match e {
                        Self::RemoveStatusEffects(x) => x.clone(),
                        _ => unreachable!("dispatched by type"),
                    },
                )
            } else if *t == ConsumeEffectType::TELEPORT_RANDOMLY {
                mc_dfu::record!(TeleportRandomlyConsumeEffect {
                    diameter: float_field_default(&POSITIVE_FLOAT, "diameter", 16.0),
                    directional_particles: BOOL.optional_field_of_default("directional_particles", true),
                })
                .xmap(Self::TeleportRandomly, |e: &Self| match e {
                    Self::TeleportRandomly(x) => x.clone(),
                    _ => unreachable!("dispatched by type"),
                })
            } else if *t == ConsumeEffectType::PLAY_SOUND {
                mc_dfu::record!(PlaySoundConsumeEffect { sound: SoundEvent::holder_codec().field_of("sound") }).xmap(
                    Self::PlaySound,
                    |e: &Self| match e {
                        Self::PlaySound(x) => x.clone(),
                        _ => unreachable!("dispatched by type"),
                    },
                )
            } else {
                MapCodec::unit(Self::ClearAllStatusEffects)
            }
        })
    }
}
impl Encode for ConsumeEffect {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.kind().id() as i32);
        match self {
            Self::ApplyStatusEffects(e) => {
                e.effects.encode(out)?;
                e.probability.encode(out)?;
            }
            Self::RemoveStatusEffects(e) => e.effects.encode_stream(out)?,
            Self::ClearAllStatusEffects => {}
            Self::TeleportRandomly(e) => {
                e.diameter.encode(out)?;
                e.directional_particles.encode(out)?;
            }
            Self::PlaySound(e) => SoundEvent::encode_holder(&e.sound, out)?,
        };
        Ok(())
    }
}
impl Decode for ConsumeEffect {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let t = read_registry_id::<ConsumeEffectType>(input)?;
        Ok(if t == ConsumeEffectType::APPLY_EFFECTS {
            Self::ApplyStatusEffects(ApplyStatusEffectsConsumeEffect { effects: Vec::decode(input)?, probability: f32::decode(input)? })
        } else if t == ConsumeEffectType::REMOVE_EFFECTS {
            Self::RemoveStatusEffects(RemoveStatusEffectsConsumeEffect { effects: RegistrySet::decode_stream(input)? })
        } else if t == ConsumeEffectType::TELEPORT_RANDOMLY {
            Self::TeleportRandomly(TeleportRandomlyConsumeEffect { diameter: f32::decode(input)?, directional_particles: bool::decode(input)? })
        } else if t == ConsumeEffectType::PLAY_SOUND {
            Self::PlaySound(PlaySoundConsumeEffect { sound: SoundEvent::decode_holder(input)? })
        } else {
            Self::ClearAllStatusEffects
        })
    }
}

// ---- Consumable / DeathProtection / FoodProperties ------------------------------------------------------------------------

string_enum! {
    /// `ItemUseAnimation` (ordinal == id).
    pub enum ItemUseAnimation {
        None = "none", Eat = "eat", Drink = "drink", Block = "block", Bow = "bow", Trident = "trident", Crossbow = "crossbow",
        Spyglass = "spyglass", TootHorn = "toot_horn", Brush = "brush", Bundle = "bundle", Spear = "spear",
    }
}
impl Encode for ItemUseAnimation {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.id());
        Ok(())
    }
}
impl Decode for ItemUseAnimation {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_enum_zero(input, ItemUseAnimation::ALL)
    }
}

/// `Consumable`.
#[derive(Clone, Debug)]
pub struct Consumable {
    pub consume_seconds: f32,
    pub animation: ItemUseAnimation,
    pub sound: Holder<SoundEvent>,
    pub has_consume_particles: bool,
    pub on_consume_effects: Vec<ConsumeEffect>,
}
impl PartialEq for Consumable {
    fn eq(&self, o: &Self) -> bool {
        feq(self.consume_seconds, o.consume_seconds)
            && self.animation == o.animation
            && self.sound == o.sound
            && self.has_consume_particles == o.has_consume_particles
            && self.on_consume_effects == o.on_consume_effects
    }
}
impl Consumable {
    pub fn codec() -> Codec<Consumable> {
        mc_dfu::record!(Consumable {
            consume_seconds: float_field_default(&NON_NEGATIVE_FLOAT, "consume_seconds", 1.6),
            animation: ItemUseAnimation::codec().optional_field_of_default("animation", ItemUseAnimation::Eat),
            sound: SoundEvent::holder_codec().optional_field_of_default("sound", sound_ref("minecraft:entity.generic.eat")),
            has_consume_particles: BOOL.optional_field_of_default("has_consume_particles", true),
            on_consume_effects: ConsumeEffect::codec().list_of().optional_field_of_default("on_consume_effects", Vec::new()),
        })
        .codec()
    }
}
impl Encode for Consumable {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.consume_seconds.encode(out)?;
        self.animation.encode(out)?;
        SoundEvent::encode_holder(&self.sound, out)?;
        self.has_consume_particles.encode(out)?;
        self.on_consume_effects.encode(out)?;
        Ok(())
    }
}
impl Decode for Consumable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Consumable {
            consume_seconds: f32::decode(input)?,
            animation: ItemUseAnimation::decode(input)?,
            sound: SoundEvent::decode_holder(input)?,
            has_consume_particles: bool::decode(input)?,
            on_consume_effects: Vec::decode(input)?,
        })
    }
}

/// `DeathProtection`.
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct DeathProtection {
    pub death_effects: Vec<ConsumeEffect>,
}
impl DeathProtection {
    pub fn codec() -> Codec<DeathProtection> {
        mc_dfu::record!(DeathProtection { death_effects: ConsumeEffect::codec().list_of().optional_field_of_default("death_effects", Vec::new()) }).codec()
    }
}

/// `FoodProperties`.
#[derive(Clone, Debug)]
pub struct FoodProperties {
    pub nutrition: i32,
    pub saturation: f32,
    pub can_always_eat: bool,
}
impl PartialEq for FoodProperties {
    fn eq(&self, o: &Self) -> bool {
        self.nutrition == o.nutrition && feq(self.saturation, o.saturation) && self.can_always_eat == o.can_always_eat
    }
}
impl FoodProperties {
    /// `FoodProperties.DIRECT_CODEC`.
    pub fn codec() -> Codec<FoodProperties> {
        mc_dfu::record!(FoodProperties {
            nutrition: NON_NEGATIVE_INT.field_of("nutrition"),
            saturation: FLOAT.field_of("saturation"),
            can_always_eat: BOOL.optional_field_of_default("can_always_eat", false),
        })
        .codec()
    }
}
impl Encode for FoodProperties {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.nutrition);
        self.saturation.encode(out)?;
        self.can_always_eat.encode(out)?;
        Ok(())
    }
}
impl Decode for FoodProperties {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(FoodProperties { nutrition: crate::codec::read_varint(input)?, saturation: f32::decode(input)?, can_always_eat: bool::decode(input)? })
    }
}

// ---- UseEffects / UseCooldown / UseRemainder ---------------------------------------------------------------------------------

/// `UseEffects`.
#[derive(Clone, Debug, Encode, Decode)]
pub struct UseEffects {
    pub can_sprint: bool,
    pub interact_vibrations: bool,
    pub speed_multiplier: f32,
}
impl PartialEq for UseEffects {
    fn eq(&self, o: &Self) -> bool {
        self.can_sprint == o.can_sprint && self.interact_vibrations == o.interact_vibrations && feq(self.speed_multiplier, o.speed_multiplier)
    }
}
impl UseEffects {
    pub fn codec() -> Codec<UseEffects> {
        mc_dfu::record!(UseEffects {
            can_sprint: BOOL.optional_field_of_default("can_sprint", false),
            interact_vibrations: BOOL.optional_field_of_default("interact_vibrations", true),
            speed_multiplier: float_field_default(&float_range(0.0, 1.0), "speed_multiplier", 0.2),
        })
        .codec()
    }
}

/// `UseCooldown`.
#[derive(Clone, Debug, Encode, Decode)]
pub struct UseCooldown {
    pub seconds: f32,
    pub cooldown_group: Option<Identifier>,
}
impl PartialEq for UseCooldown {
    fn eq(&self, o: &Self) -> bool {
        feq(self.seconds, o.seconds) && self.cooldown_group == o.cooldown_group
    }
}
impl UseCooldown {
    pub fn codec() -> Codec<UseCooldown> {
        mc_dfu::record!(UseCooldown {
            seconds: POSITIVE_FLOAT.field_of("seconds"),
            cooldown_group: IDENTIFIER.optional_field_of("cooldown_group"),
        })
        .codec()
    }
}

/// `UseRemainder`.
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct UseRemainder {
    pub convert_into: ItemStackTemplate,
}
impl UseRemainder {
    pub fn codec() -> Codec<UseRemainder> {
        // lazy: `ItemStackTemplate.CODEC` reaches the component registry, which is still being built when `defs` runs
        Codec::lazy(|| {
            ItemStackTemplate::codec().xmap(|convert_into| UseRemainder { convert_into }, |r: &UseRemainder| r.convert_into.clone())
        })
    }
}

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(ComponentDef::builder(ATTRIBUTE_MODIFIERS).persistent(ItemAttributeModifiers::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(POTION_CONTENTS).persistent(PotionContents::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(SUSPICIOUS_STEW_EFFECTS).persistent(SuspiciousStewEffects::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(CONSUMABLE).persistent(Consumable::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(DEATH_PROTECTION).persistent(DeathProtection::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(FOOD).persistent(FoodProperties::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(USE_EFFECTS).persistent(UseEffects::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(USE_COOLDOWN).persistent(UseCooldown::codec()).network_synchronized().build());
    out.push(ComponentDef::builder(USE_REMAINDER).persistent(UseRemainder::codec()).network_synchronized().build());
}
