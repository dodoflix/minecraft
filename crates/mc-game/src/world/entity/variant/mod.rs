//! `net.minecraft.world.entity.variant`: spawn-condition driven variants (`PriorityProvider`, `SpawnContext`,
//! `VariantUtils`). Variants are data-driven registry entries; an entity stores the registry id of its variant (the
//! synched data value) and caches whatever it needs from the entry.
//!
//! Supported spawn conditions: `biome`. `moon_brightness` / `structure` (cats) are added by the owner of the first variant
//! that uses them: they need the level's moon phase / structure manager through `ServerLevelAccessor`.

use crate::registry::RegistryAccess;
use crate::registry::keys;
use crate::registry::typed::HolderSetRef;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

/// `SpawnContext`.
pub struct SpawnContext<'a> {
    pub pos: BlockPos,
    pub level: &'a dyn ServerLevelAccessor,
    /// The biome at `pos` (name).
    pub biome: Option<Identifier>,
}

impl<'a> SpawnContext<'a> {
    /// `SpawnContext.create(level, pos)`.
    pub fn create(level: &'a dyn ServerLevelAccessor, pos: BlockPos) -> Self {
        let biome = level.get_biome_id(pos).and_then(|id| level.registry_access().registry(keys::BIOME)?.name_of(id.0));
        Self { pos, level, biome }
    }
}

/// `SpawnCondition`.
#[derive(Clone, Debug)]
pub enum SpawnCondition {
    /// `BiomeCheck`.
    Biome(HolderSetRef),
}

impl SpawnCondition {
    /// `test(context)`.
    pub fn test(&self, context: &SpawnContext<'_>) -> bool {
        match self {
            SpawnCondition::Biome(set) => {
                let (Some(biome), Some(registry)) = (&context.biome, context.level.registry_access().registry(keys::BIOME)) else { return false };
                match set {
                    HolderSetRef::Elements(ids) => ids.contains(biome),
                    HolderSetRef::Tag(tag) => registry.id_of(biome).is_some_and(|id| registry.is_in_tag(id, tag)),
                }
            }
        }
    }

    /// `SpawnCondition.CODEC` decode.
    pub fn from_json(v: &Value) -> Result<SpawnCondition, String> {
        let ty = v.get("type").and_then(Value::as_str).ok_or("spawn condition without type")?;
        match ty.strip_prefix("minecraft:").unwrap_or(ty) {
            "biome" => v.get("biomes").and_then(HolderSetRef::from_json).map(SpawnCondition::Biome).ok_or_else(|| "invalid biomes".to_string()),
            other => Err(format!("unsupported spawn condition type {other}")),
        }
    }
}

/// `PriorityProvider.Selector<SpawnContext, SpawnCondition>`.
#[derive(Clone, Debug)]
pub struct Selector {
    pub condition: Option<SpawnCondition>,
    pub priority: i32,
}

/// `SpawnPrioritySelectors`.
#[derive(Clone, Debug, Default)]
pub struct SpawnPrioritySelectors {
    pub selectors: Vec<Selector>,
}

impl SpawnPrioritySelectors {
    /// `SpawnPrioritySelectors.CODEC` decode.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let list = v.as_array().ok_or("spawn_conditions must be a list")?;
        let mut selectors = Vec::new();
        for s in list {
            let priority = s.get("priority").and_then(Value::as_i64).ok_or("selector without priority")? as i32;
            let condition = match s.get("condition") {
                Some(c) => Some(SpawnCondition::from_json(c)?),
                None => None,
            };
            selectors.push(Selector { condition, priority });
        }
        Ok(Self { selectors })
    }
}

/// `PriorityProvider.select(entries, extractor, context)`: the entries (registry ids) whose selectors win.
pub fn select(entries: impl Iterator<Item = (u32, SpawnPrioritySelectors)>, context: &SpawnContext<'_>) -> Vec<u32> {
    struct Unpacked {
        entry: u32,
        priority: i32,
        condition: Option<SpawnCondition>,
    }
    let mut unpacked: Vec<Unpacked> = Vec::new();
    for (id, selectors) in entries {
        for s in selectors.selectors {
            unpacked.push(Unpacked { entry: id, priority: s.priority, condition: s.condition });
        }
    }
    // HIGHEST_PRIORITY_FIRST: stable sort by priority, descending
    unpacked.sort_by(|a, b| b.priority.cmp(&a.priority));
    let mut highest_matched_priority = i32::MIN;
    let mut result = Vec::new();
    for entry in unpacked {
        if entry.priority < highest_matched_priority {
            continue;
        }
        if entry.condition.as_ref().is_none_or(|c| c.test(context)) {
            highest_matched_priority = entry.priority;
            result.push(entry.entry);
        }
    }
    result
}

/// `Util.getRandomSafe(list, random)`.
pub fn get_random_safe<T: Copy>(list: &[T], random: &mut dyn RandomSource) -> Option<T> {
    if list.is_empty() { None } else { Some(list[random.next_int_bounded(list.len() as i32) as usize]) }
}

/// `VariantUtils.getDefaultOrAny(registryAccess, id)`: the registry id of `default`, else the first entry.
pub fn get_default_or_any(registries: &RegistryAccess, registry: &str, default: &Identifier) -> u32 {
    let reg = registries.registry_or_throw(registry);
    reg.id_of(default).or_else(|| (reg.len() > 0).then_some(0)).expect("variant registry must not be empty")
}

/// `VariantUtils.writeVariant(output, holder)`.
pub fn write_variant(out: &mut CompoundTag, key: &Identifier) {
    out.put_string("variant", &key.to_string());
}

/// `VariantUtils.readVariant(input, registryId)`: the variant's identifier.
pub fn read_variant_key(input: &CompoundTag) -> Option<Identifier> {
    input.get_string("variant").and_then(Identifier::try_parse)
}

#[allow(dead_code)]
fn _unused(_: Tag) {}
