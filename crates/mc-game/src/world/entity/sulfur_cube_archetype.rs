//! `net.minecraft.world.entity.SulfurCubeArchetype` (`sulfur_cube_archetype` registry): what a sulfur cube that swallowed
//! an item of `items` turns into (attribute modifiers, buoyancy, explosion, contact damage, knockback, sounds).
//! `SulfurCubeArchetypes` (the vanilla bootstrap) is the data pack: the twelve `data/minecraft/sulfur_cube_archetype/*.json`.

use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::registry::typed::HolderSetRef;
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_core::ident::Identifier;
use mc_data::{Attribute, Item, SoundEvent};
use mc_worldgen::provider::FloatProvider;
use serde_json::Value;

/// `SulfurCubeArchetype.AttributeEntry`.
#[derive(Clone, Debug)]
pub struct AttributeEntry {
    pub attribute: Attribute,
    pub modifier: AttributeModifier,
}

/// `SulfurCubeArchetype.ContactDamage` (`damage_type` is the damage type's registry key).
#[derive(Clone, Debug)]
pub struct ContactDamage {
    pub damage_type: Identifier,
    pub amount: FloatProvider,
    pub attribute_to_source: bool,
}

/// `SulfurCubeArchetype.ExplosionData`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplosionData {
    pub power: i32,
    pub causes_fire: bool,
    pub fuse: i32,
}

/// `SulfurCubeArchetype.KnockbackModifiers`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnockbackModifiers {
    pub horizontal_power: f32,
    pub vertical_power: f32,
}

/// `SulfurCubeArchetype.SoundSettings`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundSettings {
    pub hit_sound: SoundEvent,
    pub push_sound: SoundEvent,
    pub push_sound_impulse_threshold: f32,
    pub push_sound_cooldown: f32,
}

/// `SulfurCubeArchetype.DEFAULT_KNOCKBACK_MODIFIERS`.
pub const DEFAULT_KNOCKBACK_MODIFIERS: KnockbackModifiers = KnockbackModifiers { horizontal_power: 0.33, vertical_power: 0.06 };

/// `SulfurCubeArchetype.DEFAULT_SOUND_SETTINGS`.
pub const DEFAULT_SOUND_SETTINGS: SoundSettings =
    SoundSettings { hit_sound: SoundEvent::ENTITY_SULFUR_CUBE_REGULAR_HIT, push_sound: SoundEvent::ENTITY_SULFUR_CUBE_REGULAR_PUSH, push_sound_impulse_threshold: 0.2, push_sound_cooldown: 0.5 };

/// `SulfurCubeArchetype`.
#[derive(Clone, Debug)]
pub struct SulfurCubeArchetype {
    pub items: HolderSetRef,
    pub attribute_modifiers: Vec<AttributeEntry>,
    pub buoyant: bool,
    pub explosion: Option<ExplosionData>,
    pub contact_damage: Option<ContactDamage>,
    pub knockback_modifiers: KnockbackModifiers,
    pub sound_settings: SoundSettings,
}

impl SulfurCubeArchetype {
    /// `stack.is(archetype.items())`.
    pub fn matches_item(&self, item: Item) -> bool {
        match &self.items {
            HolderSetRef::Tag(tag) => mc_data::tags::get::<Item>(tag.path()).is_some_and(|t| t.contains(item)),
            HolderSetRef::Elements(ids) => ids.iter().any(|id| Item::by_name(&id.to_string()) == Some(item)),
        }
    }

    /// `DIRECT_CODEC` decode.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let items = v.get("items").and_then(HolderSetRef::from_json).ok_or("missing or invalid items")?;
        let attribute_modifiers = v
            .get("attribute_modifiers")
            .and_then(Value::as_array)
            .ok_or("missing attribute_modifiers")?
            .iter()
            .map(attribute_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let buoyant = v.get("buoyant").and_then(Value::as_bool).unwrap_or(false);
        let explosion = v.get("explosion").map(explosion_data).transpose()?;
        let contact_damage = v.get("contact_damage").map(contact_damage).transpose()?;
        let knockback = v.get("knockback_modifiers").ok_or("missing knockback_modifiers")?;
        let knockback_modifiers = KnockbackModifiers { horizontal_power: f32_of(knockback, "horizontal_power")?, vertical_power: f32_of(knockback, "vertical_power")? };
        let sound = v.get("sound_settings").ok_or("missing sound_settings")?;
        let sound_settings = SoundSettings {
            hit_sound: sound_of(sound, "hit_sound")?,
            push_sound: sound_of(sound, "push_sound")?,
            push_sound_impulse_threshold: f32_of(sound, "push_sound_impulse_threshold")?,
            push_sound_cooldown: f32_of(sound, "push_sound_cooldown")?,
        };
        Ok(Self { items, attribute_modifiers, buoyant, explosion, contact_damage, knockback_modifiers, sound_settings })
    }
}

fn f32_of(v: &Value, key: &str) -> Result<f32, String> {
    v.get(key).and_then(Value::as_f64).map(|d| d as f32).ok_or_else(|| format!("missing {key}"))
}

fn sound_of(v: &Value, key: &str) -> Result<SoundEvent, String> {
    let name = v.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing {key}"))?;
    SoundEvent::by_name(name).ok_or_else(|| format!("unknown sound event {name}"))
}

fn attribute_entry(v: &Value) -> Result<AttributeEntry, String> {
    let name = v.get("attribute").and_then(Value::as_str).ok_or("missing attribute")?;
    let attribute = Attribute::by_name(name).ok_or_else(|| format!("unknown attribute {name}"))?;
    let id = v.get("id").and_then(Value::as_str).and_then(Identifier::try_parse).ok_or("missing modifier id")?;
    let amount = v.get("amount").and_then(Value::as_f64).ok_or("missing modifier amount")?;
    let operation = match v.get("operation").and_then(Value::as_str) {
        Some("add_value") => AttributeModifierOperation::AddValue,
        Some("add_multiplied_base") => AttributeModifierOperation::AddMultipliedBase,
        Some("add_multiplied_total") => AttributeModifierOperation::AddMultipliedTotal,
        other => return Err(format!("invalid modifier operation {other:?}")),
    };
    Ok(AttributeEntry { attribute, modifier: AttributeModifier { id, amount, operation } })
}

fn explosion_data(v: &Value) -> Result<ExplosionData, String> {
    let power = v.get("power").and_then(Value::as_i64).filter(|p| *p >= 0).ok_or("invalid explosion power")? as i32;
    let causes_fire = v.get("causes_fire").and_then(Value::as_bool).ok_or("missing causes_fire")?;
    let fuse = v.get("fuse").and_then(Value::as_i64).filter(|f| *f > 0).ok_or("invalid explosion fuse")? as i32;
    Ok(ExplosionData { power, causes_fire, fuse })
}

fn contact_damage(v: &Value) -> Result<ContactDamage, String> {
    let damage_type = v.get("damage_type").and_then(Value::as_str).and_then(Identifier::try_parse).ok_or("missing damage_type")?;
    let amount = FloatProvider::from_json_min(v.get("amount").ok_or("missing amount")?, 0.0).map_err(|e| format!("{e:?}"))?;
    let attribute_to_source = v.get("attribute_to_source").and_then(Value::as_bool).ok_or("missing attribute_to_source")?;
    Ok(ContactDamage { damage_type, amount, attribute_to_source })
}

fn decode(json: &Value, _ctx: &crate::registry::codec::RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(SulfurCubeArchetype::from_json(json)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::SULFUR_CUBE_ARCHETYPE, decode };

/// `registryAccess().lookupOrThrow(SULFUR_CUBE_ARCHETYPE).stream().filter(arch -> stack.is(arch.items()))`, in registry order.
pub fn matching_archetypes(registries: &crate::registry::RegistryAccess, item: Item) -> Vec<SulfurCubeArchetype> {
    let Some(registry) = registries.registry(keys::SULFUR_CUBE_ARCHETYPE) else { return Vec::new() };
    (0..registry.len() as u32)
        .filter_map(|id| registry.get_by_id::<SulfurCubeArchetype>(id))
        .filter(|archetype| archetype.matches_item(item))
        .cloned()
        .collect()
}
