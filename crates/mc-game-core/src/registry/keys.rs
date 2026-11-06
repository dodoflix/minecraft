//! Registry keys (`Registries.*`) and the load lists of `RegistryDataLoader`.
//!
//! The first group names registries by their full key (`minecraft:<path>`), which is also the key used in
//! [`RegistryAccess`](super::RegistryAccess); the directory of a registry's elements is the path, its tags live in `tags/<path>`.

macro_rules! keys {
    ($($name:ident = $path:literal),* $(,)?) => {
        $(pub const $name: &str = concat!("minecraft:", $path);)*
    };
}

keys! {
    DIMENSION_TYPE = "dimension_type",
    BIOME = "worldgen/biome",
    CHAT_TYPE = "chat_type",
    CARVER = "worldgen/carver",
    FEATURE = "worldgen/feature",
    PLACED_FEATURE = "worldgen/placed_feature",
    STRUCTURE = "worldgen/structure",
    STRUCTURE_SET = "worldgen/structure_set",
    PROCESSOR_LIST = "worldgen/processor_list",
    TEMPLATE_POOL = "worldgen/template_pool",
    NOISE_SETTINGS = "worldgen/noise_settings",
    NOISE = "worldgen/noise",
    DENSITY_FUNCTION = "worldgen/density_function",
    MATERIAL_RULE = "worldgen/material_rule",
    MATERIAL_CONDITION = "worldgen/material_condition",
    WORLD_PRESET = "worldgen/world_preset",
    FLAT_LEVEL_GENERATOR_PRESET = "worldgen/flat_level_generator_preset",
    TRIM_PATTERN = "trim_pattern",
    TRIM_MATERIAL = "trim_material",
    TRIAL_SPAWNER_CONFIG = "trial_spawner",
    WOLF_VARIANT = "wolf_variant",
    WOLF_SOUND_VARIANT = "wolf_sound_variant",
    PIG_VARIANT = "pig_variant",
    PIG_SOUND_VARIANT = "pig_sound_variant",
    FROG_VARIANT = "frog_variant",
    CAT_VARIANT = "cat_variant",
    CAT_SOUND_VARIANT = "cat_sound_variant",
    COW_VARIANT = "cow_variant",
    COW_SOUND_VARIANT = "cow_sound_variant",
    SULFUR_CUBE_ARCHETYPE = "sulfur_cube_archetype",
    CHICKEN_VARIANT = "chicken_variant",
    CHICKEN_SOUND_VARIANT = "chicken_sound_variant",
    ZOMBIE_NAUTILUS_VARIANT = "zombie_nautilus_variant",
    PAINTING_VARIANT = "painting_variant",
    DAMAGE_TYPE = "damage_type",
    MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST = "worldgen/multi_noise_biome_source_parameter_list",
    BANNER_PATTERN = "banner_pattern",
    ENCHANTMENT = "enchantment",
    ENCHANTMENT_PROVIDER = "enchantment_provider",
    JUKEBOX_SONG = "jukebox_song",
    INSTRUMENT = "instrument",
    TEST_ENVIRONMENT = "test_environment",
    TEST_INSTANCE = "test_instance",
    DIALOG = "dialog",
    WORLD_CLOCK = "world_clock",
    TIMELINE = "timeline",
    VILLAGER_TRADE = "villager_trade",
    TRADE_SET = "trade_set",
    DECORATED_POT_PATTERN = "decorated_pot_pattern",
    BLOCK_TRANSFORMER = "block_transformer",
    BLOCK_STATE_PROVIDER = "worldgen/block_state_provider",
    LEVEL_STEM = "dimension",
    LOOT_TABLE = "loot_table",
    PREDICATE = "predicate",
    CONTEXT_FLOAT_PROVIDER = "context_float_provider",
    CONTEXT_INT_PROVIDER = "context_int_provider",
    ITEM_MODIFIER = "item_modifier",
    SLOT_SOURCE = "slot_source",
    ADVANCEMENT = "advancement",
    RECIPE = "recipe",
    // static registries referenced by codecs
    BLOCK = "block",
    ITEM = "item",
    ENTITY_TYPE = "entity_type",
    SOUND_EVENT = "sound_event",
    ENVIRONMENT_ATTRIBUTE = "environment_attribute",
    ACTIVITY = "activity",
}

/// `RegistryValidator`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegistryValidator {
    None,
    /// `RegistryValidator.nonEmpty()`
    NonEmpty,
}

/// `RegistryDataLoader.RegistryData` (the element codec lives in [`super::decoders`]).
#[derive(Clone, Copy, Debug)]
pub struct RegistryData {
    pub key: &'static str,
    pub validator: RegistryValidator,
}

const fn data(key: &'static str) -> RegistryData {
    RegistryData {
        key,
        validator: RegistryValidator::None,
    }
}

const fn non_empty(key: &'static str) -> RegistryData {
    RegistryData {
        key,
        validator: RegistryValidator::NonEmpty,
    }
}

/// `RegistryDataLoader.WORLD_REGISTRIES` (the `WORLDGEN_REGISTRIES` stage), in vanilla's order.
pub const WORLD_REGISTRIES: &[RegistryData] = &[
    data(DIMENSION_TYPE),
    data(BIOME),
    data(CHAT_TYPE),
    data(CARVER),
    data(FEATURE),
    data(PLACED_FEATURE),
    data(STRUCTURE),
    data(STRUCTURE_SET),
    data(PROCESSOR_LIST),
    data(TEMPLATE_POOL),
    data(NOISE_SETTINGS),
    data(NOISE),
    data(DENSITY_FUNCTION),
    data(MATERIAL_RULE),
    data(MATERIAL_CONDITION),
    data(WORLD_PRESET),
    data(FLAT_LEVEL_GENERATOR_PRESET),
    data(TRIM_PATTERN),
    data(TRIM_MATERIAL),
    data(TRIAL_SPAWNER_CONFIG),
    non_empty(WOLF_VARIANT),
    non_empty(WOLF_SOUND_VARIANT),
    non_empty(PIG_VARIANT),
    non_empty(PIG_SOUND_VARIANT),
    non_empty(FROG_VARIANT),
    non_empty(CAT_VARIANT),
    non_empty(CAT_SOUND_VARIANT),
    non_empty(COW_VARIANT),
    non_empty(COW_SOUND_VARIANT),
    data(SULFUR_CUBE_ARCHETYPE),
    non_empty(CHICKEN_VARIANT),
    non_empty(CHICKEN_SOUND_VARIANT),
    non_empty(ZOMBIE_NAUTILUS_VARIANT),
    non_empty(PAINTING_VARIANT),
    data(DAMAGE_TYPE),
    data(MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST),
    data(BANNER_PATTERN),
    data(ENCHANTMENT),
    data(ENCHANTMENT_PROVIDER),
    data(JUKEBOX_SONG),
    data(INSTRUMENT),
    data(TEST_ENVIRONMENT),
    data(TEST_INSTANCE),
    data(DIALOG),
    data(WORLD_CLOCK),
    data(TIMELINE),
    data(VILLAGER_TRADE),
    data(TRADE_SET),
    data(DECORATED_POT_PATTERN),
    data(BLOCK_TRANSFORMER),
    data(BLOCK_STATE_PROVIDER),
];

/// `RegistryDataLoader.DIMENSION_REGISTRIES`.
pub const DIMENSION_REGISTRIES: &[RegistryData] = &[data(LEVEL_STEM)];

/// `RegistryDataLoader.RELOADABLE_REGISTRIES`.
pub const RELOADABLE_REGISTRIES: &[RegistryData] = &[
    data(LOOT_TABLE),
    data(PREDICATE),
    data(CONTEXT_FLOAT_PROVIDER),
    data(CONTEXT_INT_PROVIDER),
    data(ITEM_MODIFIER),
    data(SLOT_SOURCE),
    data(ADVANCEMENT),
    data(RECIPE),
];

/// `RegistryDataLoader.SYNCHRONIZED_REGISTRIES`, in the order `RegistrySynchronization.packRegistries` sends them.
pub const SYNCHRONIZED_REGISTRIES: &[&str] = &[
    BIOME,
    CHAT_TYPE,
    TRIM_PATTERN,
    TRIM_MATERIAL,
    WOLF_VARIANT,
    WOLF_SOUND_VARIANT,
    PIG_VARIANT,
    PIG_SOUND_VARIANT,
    FROG_VARIANT,
    CAT_VARIANT,
    CAT_SOUND_VARIANT,
    COW_SOUND_VARIANT,
    COW_VARIANT,
    CHICKEN_SOUND_VARIANT,
    CHICKEN_VARIANT,
    ZOMBIE_NAUTILUS_VARIANT,
    PAINTING_VARIANT,
    SULFUR_CUBE_ARCHETYPE,
    DIMENSION_TYPE,
    DAMAGE_TYPE,
    BANNER_PATTERN,
    ENCHANTMENT,
    JUKEBOX_SONG,
    INSTRUMENT,
    TEST_ENVIRONMENT,
    TEST_INSTANCE,
    DIALOG,
    WORLD_CLOCK,
    TIMELINE,
    DECORATED_POT_PATTERN,
    BLOCK_TRANSFORMER,
    BLOCK_STATE_PROVIDER,
];

/// `RegistryLoadTask` directory of a registry: its key path (`worldgen/biome`).
pub fn dir_path(registry: &str) -> &str {
    registry.strip_prefix("minecraft:").unwrap_or(registry)
}

/// `Registries.tagsDirPath`.
pub fn tags_dir_path(registry: &str) -> String {
    format!("tags/{}", dir_path(registry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match_vanilla() {
        assert_eq!(WORLD_REGISTRIES.len(), 51);
        assert_eq!(SYNCHRONIZED_REGISTRIES.len(), 32);
        // every synchronized registry is loaded in the world stage
        for key in SYNCHRONIZED_REGISTRIES {
            assert!(WORLD_REGISTRIES.iter().any(|r| r.key == *key), "{key}");
        }
    }
}
