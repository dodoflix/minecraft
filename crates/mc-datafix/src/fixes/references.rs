//! `net.minecraft.util.datafix.fixes.References` (generated from the Java constants, same order).

use crate::dsl::TypeReference;

pub struct References;

#[allow(
    dead_code,
    reason = "complete table of the vanilla References constants"
)]
impl References {
    pub const LEVEL: TypeReference = TypeReference("level");
    pub const LIGHTWEIGHT_LEVEL: TypeReference = TypeReference("lightweight_level");
    pub const PLAYER: TypeReference = TypeReference("player");
    pub const CHUNK: TypeReference = TypeReference("chunk");
    pub const HOTBAR: TypeReference = TypeReference("hotbar");
    pub const OPTIONS: TypeReference = TypeReference("options");
    pub const STRUCTURE: TypeReference = TypeReference("structure");
    pub const STATS: TypeReference = TypeReference("stats");
    pub const SAVED_DATA_COMMAND_STORAGE: TypeReference =
        TypeReference("saved_data/command_storage");
    pub const SAVED_DATA_CUSTOM_BOSS_EVENTS: TypeReference =
        TypeReference("saved_data/custom_boss_events");
    pub const SAVED_DATA_ENDER_DRAGON_FIGHT: TypeReference =
        TypeReference("saved_data/ender_dragon_fight");
    pub const SAVED_DATA_GAME_RULES: TypeReference = TypeReference("saved_data/game_rules");
    pub const SAVED_DATA_TICKETS: TypeReference = TypeReference("saved_data/tickets");
    pub const SAVED_DATA_MAP_DATA: TypeReference = TypeReference("saved_data/map_data");
    pub const SAVED_DATA_MAP_INDEX: TypeReference = TypeReference("saved_data/idcounts");
    pub const SAVED_DATA_RAIDS: TypeReference = TypeReference("saved_data/raids");
    pub const SAVED_DATA_RANDOM_SEQUENCES: TypeReference =
        TypeReference("saved_data/random_sequences");
    pub const SAVED_DATA_SCHEDULED_EVENTS: TypeReference =
        TypeReference("saved_data/scheduled_events");
    pub const SAVED_DATA_SCOREBOARD: TypeReference = TypeReference("saved_data/scoreboard");
    pub const SAVED_DATA_STOPWATCHES: TypeReference = TypeReference("saved_data/stopwatches");
    pub const SAVED_DATA_STRUCTURE_FEATURE_INDICES: TypeReference =
        TypeReference("saved_data/structure_feature_indices");
    pub const SAVED_DATA_WANDERING_TRADER: TypeReference =
        TypeReference("saved_data/wandering_trader");
    pub const SAVED_DATA_WEATHER: TypeReference = TypeReference("saved_data/weather");
    pub const SAVED_DATA_WORLD_BORDER: TypeReference = TypeReference("saved_data/world_border");
    pub const SAVED_DATA_WORLD_CLOCKS: TypeReference = TypeReference("saved_data/world_clocks");
    pub const SAVED_DATA_WORLD_GEN_SETTINGS: TypeReference =
        TypeReference("saved_data/world_gen_settings");
    pub const ADVANCEMENTS: TypeReference = TypeReference("advancements");
    pub const POI_CHUNK: TypeReference = TypeReference("poi_chunk");
    pub const ENTITY_CHUNK: TypeReference = TypeReference("entity_chunk");
    pub const DEBUG_PROFILE: TypeReference = TypeReference("debug_profile");
    pub const BLOCK_ENTITY: TypeReference = TypeReference("block_entity");
    pub const ITEM_STACK: TypeReference = TypeReference("item_stack");
    pub const BLOCK_STATE: TypeReference = TypeReference("block_state");
    pub const FLAT_BLOCK_STATE: TypeReference = TypeReference("flat_block_state");
    pub const DATA_COMPONENTS: TypeReference = TypeReference("data_components");
    pub const VILLAGER_TRADE: TypeReference = TypeReference("villager_trade");
    pub const PARTICLE: TypeReference = TypeReference("particle");
    pub const TEXT_COMPONENT: TypeReference = TypeReference("text_component");
    pub const ENTITY_EQUIPMENT: TypeReference = TypeReference("entity_equipment");
    pub const ENTITY_NAME: TypeReference = TypeReference("entity_name");
    pub const ENTITY_TREE: TypeReference = TypeReference("entity_tree");
    pub const ENTITY: TypeReference = TypeReference("entity");
    pub const BLOCK_NAME: TypeReference = TypeReference("block_name");
    pub const ITEM_NAME: TypeReference = TypeReference("item_name");
    pub const GAME_EVENT_NAME: TypeReference = TypeReference("game_event_name");
    pub const UNTAGGED_SPAWNER: TypeReference = TypeReference("untagged_spawner");
    pub const STRUCTURE_FEATURE: TypeReference = TypeReference("structure_feature");
    pub const OBJECTIVE: TypeReference = TypeReference("objective");
    pub const TEAM: TypeReference = TypeReference("team");
    pub const RECIPE: TypeReference = TypeReference("recipe");
    pub const BIOME: TypeReference = TypeReference("biome");
    pub const MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST: TypeReference =
        TypeReference("multi_noise_biome_source_parameter_list");
    pub const WORLD_GEN_SETTINGS: TypeReference = TypeReference("world_gen_settings");

    /// Every reference, in declaration order.
    pub const ALL: &'static [TypeReference] = &[
        References::LEVEL,
        References::LIGHTWEIGHT_LEVEL,
        References::PLAYER,
        References::CHUNK,
        References::HOTBAR,
        References::OPTIONS,
        References::STRUCTURE,
        References::STATS,
        References::SAVED_DATA_COMMAND_STORAGE,
        References::SAVED_DATA_CUSTOM_BOSS_EVENTS,
        References::SAVED_DATA_ENDER_DRAGON_FIGHT,
        References::SAVED_DATA_GAME_RULES,
        References::SAVED_DATA_TICKETS,
        References::SAVED_DATA_MAP_DATA,
        References::SAVED_DATA_MAP_INDEX,
        References::SAVED_DATA_RAIDS,
        References::SAVED_DATA_RANDOM_SEQUENCES,
        References::SAVED_DATA_SCHEDULED_EVENTS,
        References::SAVED_DATA_SCOREBOARD,
        References::SAVED_DATA_STOPWATCHES,
        References::SAVED_DATA_STRUCTURE_FEATURE_INDICES,
        References::SAVED_DATA_WANDERING_TRADER,
        References::SAVED_DATA_WEATHER,
        References::SAVED_DATA_WORLD_BORDER,
        References::SAVED_DATA_WORLD_CLOCKS,
        References::SAVED_DATA_WORLD_GEN_SETTINGS,
        References::ADVANCEMENTS,
        References::POI_CHUNK,
        References::ENTITY_CHUNK,
        References::DEBUG_PROFILE,
        References::BLOCK_ENTITY,
        References::ITEM_STACK,
        References::BLOCK_STATE,
        References::FLAT_BLOCK_STATE,
        References::DATA_COMPONENTS,
        References::VILLAGER_TRADE,
        References::PARTICLE,
        References::TEXT_COMPONENT,
        References::ENTITY_EQUIPMENT,
        References::ENTITY_NAME,
        References::ENTITY_TREE,
        References::ENTITY,
        References::BLOCK_NAME,
        References::ITEM_NAME,
        References::GAME_EVENT_NAME,
        References::UNTAGGED_SPAWNER,
        References::STRUCTURE_FEATURE,
        References::OBJECTIVE,
        References::TEAM,
        References::RECIPE,
        References::BIOME,
        References::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST,
        References::WORLD_GEN_SETTINGS,
    ];

    /// `TypeReference` by its type name (`"item_stack"`, `"saved_data/map_data"`, ...).
    pub fn by_name(name: &str) -> Option<TypeReference> {
        References::ALL
            .iter()
            .copied()
            .find(|r| r.type_name() == name)
    }
}
