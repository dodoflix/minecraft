//! `net.minecraft.util.datafix.DataFixTypes`.

use crate::data_fixers::CURRENT_VERSION;
use crate::dsl::TypeReference;
use crate::fixer::DataFixer;
use crate::fixes::references::References;
use mc_dfu::{Codec, DataResult, Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, Tag};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(non_camel_case_types)]
pub enum DataFixTypes {
    LEVEL,
    LEVEL_SUMMARY,
    PLAYER,
    CHUNK,
    HOTBAR,
    OPTIONS,
    STRUCTURE,
    STATS,
    SAVED_DATA_COMMAND_STORAGE,
    SAVED_DATA_CUSTOM_BOSS_EVENTS,
    SAVED_DATA_ENDER_DRAGON_FIGHT,
    SAVED_DATA_GAME_RULES,
    SAVED_DATA_FORCED_CHUNKS,
    SAVED_DATA_MAP_DATA,
    SAVED_DATA_MAP_INDEX,
    SAVED_DATA_RAIDS,
    SAVED_DATA_RANDOM_SEQUENCES,
    SAVED_DATA_SCHEDULED_EVENTS,
    SAVED_DATA_SCOREBOARD,
    SAVED_DATA_STOPWATCHES,
    SAVED_DATA_STRUCTURE_FEATURE_INDICES,
    SAVED_DATA_WANDERING_TRADER,
    SAVED_DATA_WEATHER,
    SAVED_DATA_WORLD_BORDER,
    SAVED_DATA_WORLD_CLOCKS,
    SAVED_DATA_WORLD_GEN_SETTINGS,
    ADVANCEMENTS,
    POI_CHUNK,
    WORLD_GEN_SETTINGS,
    ENTITY_CHUNK,
    DEBUG_PROFILE,
}

impl DataFixTypes {
    /// All constants in declaration order.
    pub const ALL: &'static [DataFixTypes] = &[
        DataFixTypes::LEVEL,
        DataFixTypes::LEVEL_SUMMARY,
        DataFixTypes::PLAYER,
        DataFixTypes::CHUNK,
        DataFixTypes::HOTBAR,
        DataFixTypes::OPTIONS,
        DataFixTypes::STRUCTURE,
        DataFixTypes::STATS,
        DataFixTypes::SAVED_DATA_COMMAND_STORAGE,
        DataFixTypes::SAVED_DATA_CUSTOM_BOSS_EVENTS,
        DataFixTypes::SAVED_DATA_ENDER_DRAGON_FIGHT,
        DataFixTypes::SAVED_DATA_GAME_RULES,
        DataFixTypes::SAVED_DATA_FORCED_CHUNKS,
        DataFixTypes::SAVED_DATA_MAP_DATA,
        DataFixTypes::SAVED_DATA_MAP_INDEX,
        DataFixTypes::SAVED_DATA_RAIDS,
        DataFixTypes::SAVED_DATA_RANDOM_SEQUENCES,
        DataFixTypes::SAVED_DATA_SCHEDULED_EVENTS,
        DataFixTypes::SAVED_DATA_SCOREBOARD,
        DataFixTypes::SAVED_DATA_STOPWATCHES,
        DataFixTypes::SAVED_DATA_STRUCTURE_FEATURE_INDICES,
        DataFixTypes::SAVED_DATA_WANDERING_TRADER,
        DataFixTypes::SAVED_DATA_WEATHER,
        DataFixTypes::SAVED_DATA_WORLD_BORDER,
        DataFixTypes::SAVED_DATA_WORLD_CLOCKS,
        DataFixTypes::SAVED_DATA_WORLD_GEN_SETTINGS,
        DataFixTypes::ADVANCEMENTS,
        DataFixTypes::POI_CHUNK,
        DataFixTypes::WORLD_GEN_SETTINGS,
        DataFixTypes::ENTITY_CHUNK,
        DataFixTypes::DEBUG_PROFILE,
    ];

    /// `TYPES_FOR_LEVEL_LIST`.
    pub const TYPES_FOR_LEVEL_LIST: &'static [DataFixTypes] = &[DataFixTypes::LEVEL_SUMMARY];

    /// The Java enum constant name (`SAVED_DATA_MAP_DATA`, ...).
    pub fn name(&self) -> &'static str {
        match self {
            DataFixTypes::LEVEL => "LEVEL",
            DataFixTypes::LEVEL_SUMMARY => "LEVEL_SUMMARY",
            DataFixTypes::PLAYER => "PLAYER",
            DataFixTypes::CHUNK => "CHUNK",
            DataFixTypes::HOTBAR => "HOTBAR",
            DataFixTypes::OPTIONS => "OPTIONS",
            DataFixTypes::STRUCTURE => "STRUCTURE",
            DataFixTypes::STATS => "STATS",
            DataFixTypes::SAVED_DATA_COMMAND_STORAGE => "SAVED_DATA_COMMAND_STORAGE",
            DataFixTypes::SAVED_DATA_CUSTOM_BOSS_EVENTS => "SAVED_DATA_CUSTOM_BOSS_EVENTS",
            DataFixTypes::SAVED_DATA_ENDER_DRAGON_FIGHT => "SAVED_DATA_ENDER_DRAGON_FIGHT",
            DataFixTypes::SAVED_DATA_GAME_RULES => "SAVED_DATA_GAME_RULES",
            DataFixTypes::SAVED_DATA_FORCED_CHUNKS => "SAVED_DATA_FORCED_CHUNKS",
            DataFixTypes::SAVED_DATA_MAP_DATA => "SAVED_DATA_MAP_DATA",
            DataFixTypes::SAVED_DATA_MAP_INDEX => "SAVED_DATA_MAP_INDEX",
            DataFixTypes::SAVED_DATA_RAIDS => "SAVED_DATA_RAIDS",
            DataFixTypes::SAVED_DATA_RANDOM_SEQUENCES => "SAVED_DATA_RANDOM_SEQUENCES",
            DataFixTypes::SAVED_DATA_SCHEDULED_EVENTS => "SAVED_DATA_SCHEDULED_EVENTS",
            DataFixTypes::SAVED_DATA_SCOREBOARD => "SAVED_DATA_SCOREBOARD",
            DataFixTypes::SAVED_DATA_STOPWATCHES => "SAVED_DATA_STOPWATCHES",
            DataFixTypes::SAVED_DATA_STRUCTURE_FEATURE_INDICES => {
                "SAVED_DATA_STRUCTURE_FEATURE_INDICES"
            }
            DataFixTypes::SAVED_DATA_WANDERING_TRADER => "SAVED_DATA_WANDERING_TRADER",
            DataFixTypes::SAVED_DATA_WEATHER => "SAVED_DATA_WEATHER",
            DataFixTypes::SAVED_DATA_WORLD_BORDER => "SAVED_DATA_WORLD_BORDER",
            DataFixTypes::SAVED_DATA_WORLD_CLOCKS => "SAVED_DATA_WORLD_CLOCKS",
            DataFixTypes::SAVED_DATA_WORLD_GEN_SETTINGS => "SAVED_DATA_WORLD_GEN_SETTINGS",
            DataFixTypes::ADVANCEMENTS => "ADVANCEMENTS",
            DataFixTypes::POI_CHUNK => "POI_CHUNK",
            DataFixTypes::WORLD_GEN_SETTINGS => "WORLD_GEN_SETTINGS",
            DataFixTypes::ENTITY_CHUNK => "ENTITY_CHUNK",
            DataFixTypes::DEBUG_PROFILE => "DEBUG_PROFILE",
        }
    }

    pub fn by_name(name: &str) -> Option<DataFixTypes> {
        DataFixTypes::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// The `References` type updated by this constant.
    pub fn type_ref(&self) -> TypeReference {
        match self {
            DataFixTypes::LEVEL => References::LEVEL,
            DataFixTypes::LEVEL_SUMMARY => References::LIGHTWEIGHT_LEVEL,
            DataFixTypes::PLAYER => References::PLAYER,
            DataFixTypes::CHUNK => References::CHUNK,
            DataFixTypes::HOTBAR => References::HOTBAR,
            DataFixTypes::OPTIONS => References::OPTIONS,
            DataFixTypes::STRUCTURE => References::STRUCTURE,
            DataFixTypes::STATS => References::STATS,
            DataFixTypes::SAVED_DATA_COMMAND_STORAGE => References::SAVED_DATA_COMMAND_STORAGE,
            DataFixTypes::SAVED_DATA_CUSTOM_BOSS_EVENTS => {
                References::SAVED_DATA_CUSTOM_BOSS_EVENTS
            }
            DataFixTypes::SAVED_DATA_ENDER_DRAGON_FIGHT => {
                References::SAVED_DATA_ENDER_DRAGON_FIGHT
            }
            DataFixTypes::SAVED_DATA_GAME_RULES => References::SAVED_DATA_GAME_RULES,
            DataFixTypes::SAVED_DATA_FORCED_CHUNKS => References::SAVED_DATA_TICKETS,
            DataFixTypes::SAVED_DATA_MAP_DATA => References::SAVED_DATA_MAP_DATA,
            DataFixTypes::SAVED_DATA_MAP_INDEX => References::SAVED_DATA_MAP_INDEX,
            DataFixTypes::SAVED_DATA_RAIDS => References::SAVED_DATA_RAIDS,
            DataFixTypes::SAVED_DATA_RANDOM_SEQUENCES => References::SAVED_DATA_RANDOM_SEQUENCES,
            DataFixTypes::SAVED_DATA_SCHEDULED_EVENTS => References::SAVED_DATA_SCHEDULED_EVENTS,
            DataFixTypes::SAVED_DATA_SCOREBOARD => References::SAVED_DATA_SCOREBOARD,
            DataFixTypes::SAVED_DATA_STOPWATCHES => References::SAVED_DATA_STOPWATCHES,
            DataFixTypes::SAVED_DATA_STRUCTURE_FEATURE_INDICES => {
                References::SAVED_DATA_STRUCTURE_FEATURE_INDICES
            }
            DataFixTypes::SAVED_DATA_WANDERING_TRADER => References::SAVED_DATA_WANDERING_TRADER,
            DataFixTypes::SAVED_DATA_WEATHER => References::SAVED_DATA_WEATHER,
            DataFixTypes::SAVED_DATA_WORLD_BORDER => References::SAVED_DATA_WORLD_BORDER,
            DataFixTypes::SAVED_DATA_WORLD_CLOCKS => References::SAVED_DATA_WORLD_CLOCKS,
            DataFixTypes::SAVED_DATA_WORLD_GEN_SETTINGS => {
                References::SAVED_DATA_WORLD_GEN_SETTINGS
            }
            DataFixTypes::ADVANCEMENTS => References::ADVANCEMENTS,
            DataFixTypes::POI_CHUNK => References::POI_CHUNK,
            DataFixTypes::WORLD_GEN_SETTINGS => References::WORLD_GEN_SETTINGS,
            DataFixTypes::ENTITY_CHUNK => References::ENTITY_CHUNK,
            DataFixTypes::DEBUG_PROFILE => References::DEBUG_PROFILE,
        }
    }

    /// `update(fixer, input, fromVersion, toVersion)`.
    pub fn update(
        &self,
        fixer: &DataFixer,
        input: Dynamic,
        from_version: i32,
        to_version: i32,
    ) -> Dynamic {
        fixer.update(&self.type_ref(), input, from_version, to_version)
    }

    /// `updateToCurrentVersion(fixer, input, dataVersion)`.
    pub fn update_to_current_version(
        &self,
        fixer: &DataFixer,
        input: Dynamic,
        data_version: i32,
    ) -> Dynamic {
        self.update(fixer, input, data_version, CURRENT_VERSION)
    }

    /// `update(fixer, CompoundTag, fromVersion, toVersion)`.
    pub fn update_tag(
        &self,
        fixer: &DataFixer,
        tag: &CompoundTag,
        from_version: i32,
        to_version: i32,
    ) -> CompoundTag {
        let input = Dynamic::new(NBT, Dyn::from(&Tag::Compound(tag.clone())));
        match self
            .update(fixer, input, from_version, to_version)
            .value
            .to_tag()
        {
            Ok(Tag::Compound(c)) => c,
            Ok(other) => panic!("data fixing turned a compound into {other:?}"),
            Err(e) => panic!("data fixing produced an invalid NBT tree: {e}"),
        }
    }

    /// `updateToCurrentVersion(fixer, CompoundTag, fromVersion)`.
    pub fn update_tag_to_current_version(
        &self,
        fixer: &DataFixer,
        tag: &CompoundTag,
        from_version: i32,
    ) -> CompoundTag {
        self.update_tag(fixer, tag, from_version, CURRENT_VERSION)
    }

    /// `wrapCodec(codec, dataFixer, defaultVersion)`: encodes with a `DataVersion` entry, decodes through the fixer.
    pub fn wrap_codec<A: 'static>(
        self,
        codec: &Codec<A>,
        fixer: &'static DataFixer,
        default_version: i32,
    ) -> Codec<A> {
        let (enc, dec) = (codec.clone(), codec.clone());
        Codec::from_fns(
            move |ops, input: &A, prefix| {
                enc.encode(ops, input, prefix).flat_map(|data| {
                    ops.merge_to_map_one(
                        &data,
                        ops.create_string("DataVersion"),
                        ops.create_int(CURRENT_VERSION),
                    )
                })
            },
            move |ops, input| {
                let from_version = match ops.get_map(input) {
                    DataResult::Success(m) => m
                        .get("DataVersion")
                        .and_then(|v| ops.get_number_value(v).result())
                        .map(|n| n.int_value())
                        .unwrap_or(default_version),
                    DataResult::Error { .. } => default_version,
                };
                let without_version = Dynamic::new(*ops, ops.remove(input, "DataVersion"));
                let fixed = self.update_to_current_version(fixer, without_version, from_version);
                dec.decode(ops, &fixed.value)
            },
        )
    }
}
