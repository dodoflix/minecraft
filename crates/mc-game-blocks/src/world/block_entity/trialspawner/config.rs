//! `TrialSpawnerConfig` and `TrialSpawner.FullConfig`.

use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::util::weighted_list::{Weighted, WeightedList};
use mc_game_core::world::level::spawn_data::SpawnData;
use mc_core::ident::Identifier;
use mc_dfu::extra::{IDENTIFIER, NON_NEGATIVE_INT};
use mc_dfu::{Codec, DataResult, Either, Dyn, FLOAT, INT, float_range, int_range, record};
use mc_data::EntityType;
use serde_json::Value;
use std::sync::{Arc, LazyLock};

fn id(path: &str) -> Identifier {
    Identifier::parse(&format!("minecraft:{path}")).expect("constant identifier")
}

/// `TrialSpawnerConfig`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrialSpawnerConfig {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i32,
    pub spawn_potentials_definition: WeightedList<SpawnData>,
    pub loot_tables_to_eject: WeightedList<Identifier>,
    pub items_to_drop_when_ominous: Identifier,
}

/// `TrialSpawnerConfig.DEFAULT` (`builder().build()`).
pub static DEFAULT: LazyLock<Arc<TrialSpawnerConfig>> = LazyLock::new(|| {
    Arc::new(TrialSpawnerConfig {
        spawn_range: 4,
        total_mobs: 6.0,
        simultaneous_mobs: 2.0,
        total_mobs_added_per_player: 2.0,
        simultaneous_mobs_added_per_player: 1.0,
        ticks_between_spawn: 40,
        spawn_potentials_definition: WeightedList::empty(),
        loot_tables_to_eject: WeightedList::of(vec![
            Weighted { value: id("spawners/trial_chamber/consumables"), weight: 1 },
            Weighted { value: id("spawners/trial_chamber/key"), weight: 1 },
        ])
        .expect("constant weights"),
        items_to_drop_when_ominous: id("spawners/trial_chamber/items_to_drop_when_ominous"),
    })
});

impl TrialSpawnerConfig {
    /// `calculateTargetTotalMobs(additionalPlayers)`.
    pub fn calculate_target_total_mobs(&self, additional_players: i32) -> i32 {
        (self.total_mobs + self.total_mobs_added_per_player * additional_players as f32).floor() as i32
    }

    /// `calculateTargetSimultaneousMobs(additionalPlayers)`.
    pub fn calculate_target_simultaneous_mobs(&self, additional_players: i32) -> i32 {
        (self.simultaneous_mobs + self.simultaneous_mobs_added_per_player * additional_players as f32).floor() as i32
    }

    /// `ticksBetweenItemSpawners()`.
    pub fn ticks_between_item_spawners(&self) -> i64 {
        160
    }

    /// `withSpawning(type)`.
    pub fn with_spawning(&self, ty: EntityType) -> TrialSpawnerConfig {
        let spawn_data = SpawnData::of_entity_id(&Identifier::parse(ty.data().name).expect("registered entity type"));
        self.with_potentials(spawn_data)
    }

    /// `withSpawning(entityData)`: `TypedEntityData.loadInto(spawnData, registry)` on an empty `SpawnData`.
    pub fn with_spawning_data(&self, ty: EntityType, data: &mc_nbt::CompoundTag) -> TrialSpawnerConfig {
        let mut tag = mc_nbt::CompoundTag::new();
        tag.put_string("id", ty.data().name);
        tag.merge(data);
        self.with_potentials(SpawnData::new(tag, None, None))
    }

    fn with_potentials(&self, spawn_data: SpawnData) -> TrialSpawnerConfig {
        TrialSpawnerConfig { spawn_potentials_definition: WeightedList::of_value(spawn_data), ..self.clone() }
    }

    /// `TrialSpawnerConfig.DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<Arc<TrialSpawnerConfig>> {
        static C: LazyLock<Codec<Arc<TrialSpawnerConfig>>> = LazyLock::new(|| {
            let d = &**DEFAULT;
            let loot_key = IDENTIFIER.clone();
            record!(TrialSpawnerConfig {
                spawn_range: int_range(1, 128).optional_field_of_default("spawn_range", d.spawn_range),
                total_mobs: float_range(0.0, f32::MAX).optional_field_of_default("total_mobs", d.total_mobs),
                simultaneous_mobs: float_range(0.0, f32::MAX).optional_field_of_default("simultaneous_mobs", d.simultaneous_mobs),
                total_mobs_added_per_player: float_range(0.0, f32::MAX).optional_field_of_default("total_mobs_added_per_player", d.total_mobs_added_per_player),
                simultaneous_mobs_added_per_player: float_range(0.0, f32::MAX)
                    .optional_field_of_default("simultaneous_mobs_added_per_player", d.simultaneous_mobs_added_per_player),
                ticks_between_spawn: int_range(0, i32::MAX).optional_field_of_default("ticks_between_spawn", d.ticks_between_spawn),
                spawn_potentials_definition: SpawnData::list_codec().optional_field_of_default("spawn_potentials", WeightedList::empty()),
                loot_tables_to_eject: WeightedList::codec(&loot_key).optional_field_of_default("loot_tables_to_eject", d.loot_tables_to_eject.clone()),
                items_to_drop_when_ominous: loot_key.optional_field_of_default("items_to_drop_when_ominous", d.items_to_drop_when_ominous.clone()),
            })
            .codec()
            .xmap(Arc::new, |c: &Arc<TrialSpawnerConfig>| (**c).clone())
        });
        C.clone()
    }
}

/// `Holder<TrialSpawnerConfig>`: a reference to the `trial_spawner` registry entry or an inline definition.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigHolder {
    Reference(Identifier),
    Direct(Arc<TrialSpawnerConfig>),
}

impl ConfigHolder {
    /// `holder.value()`.
    pub fn value(&self, level: &ServerLevel) -> Arc<TrialSpawnerConfig> {
        match self {
            ConfigHolder::Direct(config) => config.clone(),
            ConfigHolder::Reference(key) => level.registries.get::<Arc<TrialSpawnerConfig>>(keys::TRIAL_SPAWNER_CONFIG, key).cloned().unwrap_or_else(|| DEFAULT.clone()),
        }
    }

    /// `TrialSpawnerConfig.CODEC` (`RegistryCodecs.holder(TRIAL_SPAWNER_CONFIG, DIRECT_CODEC)`, in a registry-aware ops).
    pub fn codec() -> Codec<ConfigHolder> {
        static C: LazyLock<Codec<ConfigHolder>> = LazyLock::new(|| {
            IDENTIFIER
                .either(&TrialSpawnerConfig::direct_codec())
                .flat_xmap(
                    |either| match either {
                        Either::Left(key) if mc_protocol::registry_context::registry_contains(keys::TRIAL_SPAWNER_CONFIG, &key) => DataResult::success(ConfigHolder::Reference(key)),
                        Either::Left(key) => DataResult::error(move || format!("Failed to get element {key} from registry minecraft:trial_spawner")),
                        Either::Right(config) => DataResult::success(ConfigHolder::Direct(config)),
                    },
                    |holder: &ConfigHolder| {
                        DataResult::success(match holder {
                            ConfigHolder::Reference(key) => Either::Left(key.clone()),
                            ConfigHolder::Direct(config) => Either::Right(config.clone()),
                        })
                    },
                )
        });
        C.clone()
    }
}

/// `TrialSpawner.FullConfig(normal, ominous, targetCooldownLength, requiredPlayerRange)`.
#[derive(Clone, Debug, PartialEq)]
pub struct FullConfig {
    pub normal: ConfigHolder,
    pub ominous: ConfigHolder,
    pub target_cooldown_length: i32,
    pub required_player_range: i32,
}

impl Default for FullConfig {
    /// `FullConfig.DEFAULT`.
    fn default() -> Self {
        FullConfig {
            normal: ConfigHolder::Direct(DEFAULT.clone()),
            ominous: ConfigHolder::Direct(DEFAULT.clone()),
            target_cooldown_length: 36000,
            required_player_range: 14,
        }
    }
}

impl FullConfig {
    /// `FullConfig.MAP_CODEC`.
    pub fn map_codec() -> mc_dfu::MapCodec<FullConfig> {
        let d = FullConfig::default();
        record!(FullConfig {
            normal: ConfigHolder::codec().optional_field_of_default("normal_config", d.normal.clone()),
            ominous: ConfigHolder::codec().optional_field_of_default("ominous_config", d.ominous.clone()),
            target_cooldown_length: NON_NEGATIVE_INT.optional_field_of_default("target_cooldown_length", 36000),
            required_player_range: int_range(1, 128).optional_field_of_default("required_player_range", 14),
        })
    }

    /// `overrideEntity(type)`.
    pub fn override_entity(&self, level: &ServerLevel, ty: EntityType) -> FullConfig {
        FullConfig {
            normal: ConfigHolder::Direct(Arc::new(self.normal.value(level).with_spawning(ty))),
            ominous: ConfigHolder::Direct(Arc::new(self.ominous.value(level).with_spawning(ty))),
            ..self.clone()
        }
    }

    /// `overrideEntityData(entityData)`.
    pub fn override_entity_data(&self, level: &ServerLevel, ty: EntityType, data: &mc_nbt::CompoundTag) -> FullConfig {
        FullConfig {
            normal: ConfigHolder::Direct(Arc::new(self.normal.value(level).with_spawning_data(ty, data))),
            ominous: ConfigHolder::Direct(Arc::new(self.ominous.value(level).with_spawning_data(ty, data))),
            ..self.clone()
        }
    }
}

/// The `trial_spawner` registry decoder: entries are `TrialSpawnerConfig.DIRECT_CODEC` JSON.
fn decode_entry(json: &Value, _ctx: &mc_game_core::registry::codec::RegistryDecodeContext) -> Result<mc_game_core::registry::decoders::DecodedEntry, String> {
    let value = Dyn::from_json_str(&json.to_string()).map_err(|e| format!("Failed to parse JSON: {e}"))?;
    match TrialSpawnerConfig::direct_codec().parse(&mc_dfu::JSON, &value) {
        DataResult::Success(config) => Ok(mc_game_core::registry::decoders::DecodedEntry::new(config)),
        DataResult::Error { message, .. } => Err(message.get()),
    }
}

#[linkme::distributed_slice(mc_game_core::registry::decoders::DYNAMIC_REGISTRY_DECODERS)]
static TRIAL_SPAWNER_CONFIG_DECODER: mc_game_core::registry::decoders::DynamicRegistryDecoder =
    mc_game_core::registry::decoders::DynamicRegistryDecoder { registry: keys::TRIAL_SPAWNER_CONFIG, decode: decode_entry };
