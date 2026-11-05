//! `net.minecraft.world.entity.variant`: spawn-condition driven variants (`SpawnCondition`, `SpawnContext`,
//! `PriorityProvider`, `SpawnPrioritySelectors`, `VariantUtils`) plus the shared codecs of the mob variant registries.
//!
//! # How a mob uses a variant registry
//! Variants are data-driven registry entries (`wolf_variant`, `cat_variant`, `frog_variant`, `pig_variant`, `cow_variant`,
//! `chicken_variant`, `zombie_nautilus_variant`, and the sound variants). Every registry has a typed value
//! (`animal::wolf::WolfVariant`, ...) readable with `registries.get::<T>(registry_key, &id)` /
//! `Registry::get_by_id::<T>(id)`; the entity stores the registry *id* (the synched data value, `u32`/`i32`) or the entry's
//! `Identifier` (NBT `variant`, [`write_variant`] / [`read_variant_key`]) and caches what it needs from the entry.
//!
//! * Choosing a variant for a newly spawned mob (`VariantUtils.selectVariantToSpawn(SpawnContext, registry)`):
//!   build a [`SpawnContext::create`]`(&*level_accessor, pos)` and call
//!   [`select_variant_to_spawn::<T>`]`(&context, keys::X_VARIANT, random)`; `random` is `level.getRandom()` of vanilla (take
//!   it out of the level with `ServerLevelAccessor::with_random_taken`). The RNG use is exactly vanilla's: the matching
//!   entries (highest matched priority wins, ties kept in registry order) are collected without drawing and
//!   one `random.nextInt(count)` picks among them (nothing is drawn when none matches). `None` = vanilla's empty
//!   `Optional`; fall back to [`get_default_or_any`] (`VariantUtils.getDefaultOrAny`) / [`get_any`].
//! * Random sound variants (`XSoundVariants.pickRandomSoundVariant`) are [`pick_random_entry`] (`Registry.getRandom`).
//! * Typed variants implement [`PriorityProvider`] (`selectors()`), so tests and goldens can call [`select_entries`] directly.
//! * Spawn conditions: `biome`, `moon_brightness`, `structure` ([`SpawnCondition`]); the level side they read
//!   (`getBiome`, `environmentAttributes().getValue(MOON_PHASE)`, `structureManager().getStructureWithPieceAt`) is
//!   [`SpawnContextLevel`] (`biome_at`, `moon_brightness_at`, `has_structure_piece_at`).
//!
//! The registry decoders (`DYNAMIC_REGISTRY_DECODERS`) of the variant registries live next to the variant types under
//! `world::entity::animal::*`; they produce the typed value and the exact `registry_data` NBT (the `NETWORK_CODEC` encoding:
//! `spawn_conditions` are not sent to the client).

pub mod level;
pub mod model_and_texture;

use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_dfu::{Codec, DataResult, Dyn, INT, JSON, NBT, record};
use mc_nbt::CompoundTag;
use mc_protocol::components::values::predicates::Doubles;
use mc_protocol::components::{Holder, HolderSet, NoDirect, SoundEvent as ComponentSoundEvent};
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::DecodedEntry;
use crate::registry::{RegistryAccess, keys};
use crate::world::item::enchantment::codecs::{holder_set_codec, with_decode_context as with_scoped_context};
use crate::world::level::storage::loot::holder::{TypeTable, lift, with_decode_context};
use mc_core::pos::BlockPos;

// ---- SpawnContext ----------------------------------------------------------------------------------------------------

/// What the spawn conditions read from the level (`ServerLevelAccessor` extends it, so any level accessor is a
/// `&dyn SpawnContextLevel`; tests implement just this).
pub trait SpawnContextLevel {
    /// `registryAccess()`.
    fn variant_registries(&self) -> &RegistryAccess;
    /// `getBiome(pos)` as the biome's registry id (`None` while a needed chunk is missing).
    fn biome_at(&self, pos: BlockPos) -> Option<Identifier>;
    /// `DimensionType.MOON_BRIGHTNESS_PER_PHASE[environmentAttributes().getValue(MOON_PHASE, Vec3.atCenterOf(pos))]`
    /// (`MoonBrightnessCheck`).
    fn moon_brightness_at(&self, pos: BlockPos) -> f32;
    /// `getLevel().structureManager().getStructureWithPieceAt(pos, structures).isValid()` where `matcher` is the structure
    /// set's `contains` (`StructureCheck`).
    fn has_structure_piece_at(&self, pos: BlockPos, matcher: &dyn Fn(&Identifier) -> bool) -> bool;
}

/// `SpawnContext`.
pub struct SpawnContext<'a> {
    pub pos: BlockPos,
    pub level: &'a dyn SpawnContextLevel,
    /// The biome at `pos` (`Holder<Biome>`, by name); `None` while the chunk is missing.
    pub biome: Option<Identifier>,
}

impl<'a> SpawnContext<'a> {
    /// `SpawnContext.create(level, pos)`.
    pub fn create(level: &'a dyn SpawnContextLevel, pos: BlockPos) -> Self {
        Self { pos, level, biome: level.biome_at(pos) }
    }

    /// A context over an explicit biome (`new SpawnContext(pos, level, attributes, biome)`).
    pub fn new(pos: BlockPos, level: &'a dyn SpawnContextLevel, biome: Option<Identifier>) -> Self {
        Self { pos, level, biome }
    }
}

// ---- SpawnCondition --------------------------------------------------------------------------------------------------

/// `BiomeCheck`.
#[derive(Clone, Debug, PartialEq)]
pub struct BiomeCheck {
    pub required_biomes: HolderSet<NoDirect>,
}

/// `MoonBrightnessCheck`.
#[derive(Clone, Debug, PartialEq)]
pub struct MoonBrightnessCheck {
    pub range: Doubles,
}

/// `StructureCheck`.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureCheck {
    pub required_structures: HolderSet<NoDirect>,
}

/// `SpawnCondition` (the `spawn_condition_type` registry: `structure`, `moon_brightness`, `biome`).
#[derive(Clone, Debug, PartialEq)]
pub enum SpawnCondition {
    Biome(BiomeCheck),
    MoonBrightness(MoonBrightnessCheck),
    Structure(StructureCheck),
}

/// `holderSet.contains(holder)` of a reference-only set of the registry `registry` (tags resolve through `registries`).
pub fn holder_set_contains(registries: &RegistryAccess, registry: &str, set: &HolderSet<NoDirect>, id: &Identifier) -> bool {
    match set {
        HolderSet::Direct(entries) => entries.iter().any(|h| h.name() == Some(id)),
        HolderSet::Named(tag) => registries.is_in_tag(registry, id, tag),
    }
}

impl SpawnCondition {
    /// The registered type name.
    pub fn type_name(&self) -> &'static str {
        match self {
            SpawnCondition::Biome(_) => "biome",
            SpawnCondition::MoonBrightness(_) => "moon_brightness",
            SpawnCondition::Structure(_) => "structure",
        }
    }

    /// `test(context)`.
    pub fn test(&self, context: &SpawnContext<'_>) -> bool {
        match self {
            SpawnCondition::Biome(check) => {
                let Some(biome) = &context.biome else { return false };
                holder_set_contains(context.level.variant_registries(), keys::BIOME, &check.required_biomes, biome)
            }
            SpawnCondition::MoonBrightness(check) => {
                check.range.matches(f64::from(context.level.moon_brightness_at(context.pos)))
            }
            SpawnCondition::Structure(check) => {
                let registries = context.level.variant_registries();
                let matcher = |id: &Identifier| holder_set_contains(registries, keys::STRUCTURE, &check.required_structures, id);
                context.level.has_structure_piece_at(context.pos, &matcher)
            }
        }
    }

    /// `SpawnCondition.CODEC`.
    pub fn codec() -> Codec<SpawnCondition> {
        static TYPES: LazyLock<Arc<TypeTable<SpawnCondition>>> = LazyLock::new(|| {
            fn as_structure(c: &SpawnCondition) -> Option<&StructureCheck> {
                if let SpawnCondition::Structure(s) = c { Some(s) } else { None }
            }
            fn as_moon(c: &SpawnCondition) -> Option<&MoonBrightnessCheck> {
                if let SpawnCondition::MoonBrightness(s) = c { Some(s) } else { None }
            }
            fn as_biome(c: &SpawnCondition) -> Option<&BiomeCheck> {
                if let SpawnCondition::Biome(s) = c { Some(s) } else { None }
            }
            TypeTable::new(
                "minecraft:spawn_condition_type",
                vec![
                    (
                        "structure",
                        lift(
                            record!(StructureCheck { required_structures: holder_set_codec::<NoDirect>(keys::STRUCTURE).field_of("structures") }),
                            |s| Arc::new(SpawnCondition::Structure(s)),
                            as_structure,
                        ),
                    ),
                    (
                        "moon_brightness",
                        lift(record!(MoonBrightnessCheck { range: Doubles::codec().field_of("range") }), |s| Arc::new(SpawnCondition::MoonBrightness(s)), as_moon),
                    ),
                    (
                        "biome",
                        lift(
                            record!(BiomeCheck { required_biomes: holder_set_codec::<NoDirect>(keys::BIOME).field_of("biomes") }),
                            |s| Arc::new(SpawnCondition::Biome(s)),
                            as_biome,
                        ),
                    ),
                ],
            )
        });
        static C: LazyLock<Codec<SpawnCondition>> = LazyLock::new(|| {
            TYPES.codec(|c| c.type_name()).xmap(|a: Arc<SpawnCondition>| (*a).clone(), |c: &SpawnCondition| Arc::new(c.clone()))
        });
        C.clone()
    }
}

// ---- PriorityProvider / selectors -------------------------------------------------------------------------------------

/// `PriorityProvider.Selector<SpawnContext, SpawnCondition>`: no condition = always true.
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub condition: Option<SpawnCondition>,
    pub priority: i32,
}

impl Selector {
    /// `Selector.codec(SpawnCondition.CODEC)`.
    pub fn codec() -> Codec<Selector> {
        static C: LazyLock<Codec<Selector>> = LazyLock::new(|| {
            record!(Selector { condition: SpawnCondition::codec().optional_field_of("condition"), priority: INT.field_of("priority") }).codec()
        });
        C.clone()
    }
}

/// `SpawnPrioritySelectors`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnPrioritySelectors {
    pub selectors: Vec<Selector>,
}

impl SpawnPrioritySelectors {
    /// `SpawnPrioritySelectors.EMPTY`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// `SpawnPrioritySelectors.single(condition, priority)`.
    pub fn single(condition: SpawnCondition, priority: i32) -> Self {
        Self { selectors: vec![Selector { condition: Some(condition), priority }] }
    }

    /// `SpawnPrioritySelectors.fallback(priority)`.
    pub fn fallback(priority: i32) -> Self {
        Self { selectors: vec![Selector { condition: None, priority }] }
    }

    /// `SpawnPrioritySelectors.CODEC`.
    pub fn codec() -> Codec<SpawnPrioritySelectors> {
        static C: LazyLock<Codec<SpawnPrioritySelectors>> =
            LazyLock::new(|| Selector::codec().list_of().xmap(|selectors| SpawnPrioritySelectors { selectors }, |s: &SpawnPrioritySelectors| s.selectors.clone()));
        C.clone()
    }
}

/// `PriorityProvider<SpawnContext, SpawnCondition>`: what the typed variants implement.
pub trait PriorityProvider {
    /// `selectors()`.
    fn selectors(&self) -> &[Selector];
}

/// `PriorityProvider.select(entries, extractor, context)` over `(entry, selectors)`: the entries whose selectors win
/// (highest matched priority; entries with several winning selectors appear once per selector, like vanilla's stream).
pub fn select_entries<K: Clone>(entries: impl IntoIterator<Item = (K, Vec<Selector>)>, context: &SpawnContext<'_>) -> Vec<K> {
    struct Unpacked<K> {
        entry: K,
        priority: i32,
        condition: Option<SpawnCondition>,
    }
    let mut unpacked: Vec<Unpacked<K>> = Vec::new();
    for (entry, selectors) in entries {
        for s in selectors {
            unpacked.push(Unpacked { entry: entry.clone(), priority: s.priority, condition: s.condition });
        }
    }
    // `HIGHEST_PRIORITY_FIRST` (`comparingInt(priority).reversed()`) on a stable `List.sort`
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

/// [`select_entries`] over registry ids (`u32` entry type).
pub fn select(entries: impl Iterator<Item = (u32, SpawnPrioritySelectors)>, context: &SpawnContext<'_>) -> Vec<u32> {
    select_entries(entries.map(|(id, s)| (id, s.selectors)), context)
}

/// `Util.getRandomSafe(list, random)`.
pub fn get_random_safe<T: Copy>(list: &[T], random: &mut dyn RandomSource) -> Option<T> {
    if list.is_empty() { None } else { Some(list[random.next_int_bounded(list.len() as i32) as usize]) }
}

/// `PriorityProvider.pick(entries, extractor, random, context)`.
pub fn pick<K: Clone>(entries: impl IntoIterator<Item = (K, Vec<Selector>)>, random: &mut dyn RandomSource, context: &SpawnContext<'_>) -> Option<K> {
    let selected = select_entries(entries, context);
    if selected.is_empty() { None } else { Some(selected[random.next_int_bounded(selected.len() as i32) as usize].clone()) }
}

// ---- VariantUtils ----------------------------------------------------------------------------------------------------

/// `VariantUtils.TAG_VARIANT`.
pub const TAG_VARIANT: &str = "variant";

/// `VariantUtils.selectVariantToSpawn(context, variantRegistry)`: the id of the picked entry of `registry`.
pub fn select_variant_to_spawn<T: PriorityProvider + Send + Sync + 'static>(
    context: &SpawnContext<'_>,
    registry: &str,
    random: &mut dyn RandomSource,
) -> Option<Identifier> {
    let reg = context.level.variant_registries().registry_or_throw(registry);
    let entries = reg.entries().iter().filter_map(|e| reg.get::<T>(&e.id).map(|v| (e.id.clone(), v.selectors().to_vec())));
    pick(entries, random, context)
}

/// `Registry.getRandom(random)`: `Util.getRandomSafe(byId, random)` as the entry id.
pub fn pick_random_entry(registries: &RegistryAccess, registry: &str, random: &mut dyn RandomSource) -> Option<Identifier> {
    let reg = registries.registry_or_throw(registry);
    let count = reg.len();
    if count == 0 {
        return None;
    }
    reg.name_of(random.next_int_bounded(count as i32) as u32)
}

/// `VariantUtils.getDefaultOrAny(registryAccess, id)`: the registry id of `default`, else the first entry.
pub fn get_default_or_any(registries: &RegistryAccess, registry: &str, default: &Identifier) -> u32 {
    let reg = registries.registry_or_throw(registry);
    reg.id_of(default).or_else(|| (reg.len() > 0).then_some(0)).expect("variant registry must not be empty")
}

/// `VariantUtils.getAny(registryAccess, registryId)`: the first entry's registry id.
pub fn get_any(registries: &RegistryAccess, registry: &str) -> u32 {
    assert!(registries.registry_or_throw(registry).len() > 0, "variant registry must not be empty");
    0
}

/// `VariantUtils.writeVariant(output, holder)`.
pub fn write_variant(out: &mut CompoundTag, key: &Identifier) {
    out.put_string(TAG_VARIANT, &key.to_string());
}

/// `VariantUtils.readVariant(input, registryId)`: the variant's identifier (resolve it against the registry).
pub fn read_variant_key(input: &CompoundTag) -> Option<Identifier> {
    input.get_string(TAG_VARIANT).and_then(Identifier::try_parse)
}

// ---- shared codecs of the variant registries --------------------------------------------------------------------------

/// `SoundEvent.CODEC` of the sound-set records, as the registered sound (inline definitions resolve by their id, like
/// `holder_sound`; unknown sounds fail).
pub fn sound_event_codec() -> Codec<mc_data::SoundEvent> {
    ComponentSoundEvent::holder_codec().flat_xmap(
        |holder: Holder<ComponentSoundEvent>| match crate::world::item::holder_sound(&holder) {
            Some(sound) => DataResult::success(sound),
            None => DataResult::error_str("Unknown sound event"),
        },
        |sound: &mc_data::SoundEvent| DataResult::success(Holder::Reference(sound.identifier())),
    )
}

/// Parses `json` with a variant registry's `DIRECT_CODEC` and encodes the value with its `NETWORK_CODEC` (what `registry_data`
/// carries).
pub fn decode_variant<T: Clone + Send + Sync + 'static>(
    direct: &Codec<T>,
    network: &Codec<T>,
    json: &Value,
    ctx: &RegistryDecodeContext,
) -> Result<DecodedEntry, String> {
    let input = Dyn::from_json_str(&json.to_string())?;
    let value = with_decode_context(ctx, || with_scoped_context(ctx, || direct.parse(&JSON, &input))).into_result()?;
    let tag = network.encode_start(&NBT, &value).into_result()?.to_tag()?;
    Ok(DecodedEntry::with_network(value, tag))
}
