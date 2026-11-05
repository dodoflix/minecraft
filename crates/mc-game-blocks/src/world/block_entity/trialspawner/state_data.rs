//! `TrialSpawnerStateData`: the mutable run state of a trial spawner (tracked players and mobs, counters, next spawn
//! data, the ejecting loot table) and its saved form (`Packed`).

use crate::util::java_uuid_hash_set::UuidHashSet;
use crate::util::weighted_list::{Weighted, WeightedList};
use crate::world::item::ItemStack;
use crate::world::level::spawn_data::SpawnData;
use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, LONG, MapCodec, int_range, record};
use mc_protocol::components::uuid_codec;
use std::sync::LazyLock;
use uuid::Uuid;

#[derive(Default)]
pub struct TrialSpawnerStateData {
    pub detected_players: UuidHashSet,
    pub current_mobs: UuidHashSet,
    pub cooldown_ends_at: i64,
    pub next_mob_spawns_at: i64,
    pub total_mobs_spawned: i32,
    pub next_spawn_data: Option<SpawnData>,
    pub ejecting_loot_table: Option<Identifier>,
    /// `displayEntity != null`: the entity of the next spawn data was loaded once (the server never uses it beyond that).
    pub display_entity_created: bool,
    /// `dispensing`: the items an ominous spawner drops, computed once per block entity.
    pub dispensing: Option<WeightedList<ItemStack>>,
}

/// `TrialSpawnerStateData.Packed`.
pub struct Packed {
    pub detected_players: Vec<Uuid>,
    pub current_mobs: Vec<Uuid>,
    pub cooldown_ends_at: i64,
    pub next_mob_spawns_at: i64,
    pub total_mobs_spawned: i32,
    pub next_spawn_data: Option<SpawnData>,
    pub ejecting_loot_table: Option<Identifier>,
}

impl Packed {
    /// `Packed.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<Packed> {
        static ITEMS: LazyLock<Codec<Vec<Uuid>>> = LazyLock::new(|| uuid_codec().list_of());
        record!(Packed {
            detected_players: ITEMS.lenient_optional_field_of_default("registered_players", Vec::new()),
            current_mobs: ITEMS.lenient_optional_field_of_default("current_mobs", Vec::new()),
            cooldown_ends_at: LONG.lenient_optional_field_of_default("cooldown_ends_at", 0),
            next_mob_spawns_at: LONG.lenient_optional_field_of_default("next_mob_spawns_at", 0),
            total_mobs_spawned: int_range(0, i32::MAX).lenient_optional_field_of_default("total_mobs_spawned", 0),
            next_spawn_data: SpawnData::codec().lenient_optional_field_of("spawn_data"),
            ejecting_loot_table: IDENTIFIER.lenient_optional_field_of("ejecting_loot_table"),
        })
    }
}

impl TrialSpawnerStateData {
    /// `pack()`.
    pub fn pack(&self) -> Packed {
        Packed {
            detected_players: self.detected_players.iter().copied().collect(),
            current_mobs: self.current_mobs.iter().copied().collect(),
            cooldown_ends_at: self.cooldown_ends_at,
            next_mob_spawns_at: self.next_mob_spawns_at,
            total_mobs_spawned: self.total_mobs_spawned,
            next_spawn_data: self.next_spawn_data.clone(),
            ejecting_loot_table: self.ejecting_loot_table.clone(),
        }
    }

    /// `apply(packed)`: the packed sets are `HashSet`s built from the saved lists, added into the live (kept) tables.
    pub fn apply(&mut self, packed: Packed) {
        self.detected_players.clear();
        let players = UuidHashSet::from_collection(&packed.detected_players);
        self.detected_players.extend_from(players.iter().copied());
        self.current_mobs.clear();
        let mobs = UuidHashSet::from_collection(&packed.current_mobs);
        self.current_mobs.extend_from(mobs.iter().copied());
        self.cooldown_ends_at = packed.cooldown_ends_at;
        self.next_mob_spawns_at = packed.next_mob_spawns_at;
        self.total_mobs_spawned = packed.total_mobs_spawned;
        self.next_spawn_data = packed.next_spawn_data;
        self.ejecting_loot_table = packed.ejecting_loot_table;
    }

    /// `reset()`.
    pub fn reset(&mut self) {
        self.current_mobs.clear();
        self.next_spawn_data = None;
        self.reset_statistics();
    }

    /// `resetStatistics()`.
    pub fn reset_statistics(&mut self) {
        self.detected_players.clear();
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
        self.cooldown_ends_at = 0;
    }

    /// `hasFinishedSpawningAllMobs(config, additionalPlayers)`.
    pub fn has_finished_spawning_all_mobs(&self, target_total_mobs: i32) -> bool {
        self.total_mobs_spawned >= target_total_mobs
    }

    /// `haveAllCurrentMobsDied()`.
    pub fn have_all_current_mobs_died(&self) -> bool {
        self.current_mobs.is_empty()
    }

    /// `isReadyToSpawnNextMob(level, config, additionalPlayers)`.
    pub fn is_ready_to_spawn_next_mob(&self, game_time: i64, target_simultaneous_mobs: i32) -> bool {
        game_time >= self.next_mob_spawns_at && (self.current_mobs.len() as i32) < target_simultaneous_mobs
    }

    /// `countAdditionalPlayers(pos)`.
    pub fn count_additional_players(&self) -> i32 {
        (self.detected_players.len() as i32 - 1).max(0)
    }

    /// `isReadyToOpenShutter(level, delayBeforeOpen, targetCooldownLength)`.
    pub fn is_ready_to_open_shutter(&self, game_time: i64, delay_before_open: f32, target_cooldown_length: i32) -> bool {
        let cooldown_started_at = self.cooldown_ends_at.wrapping_sub(i64::from(target_cooldown_length));
        game_time as f32 >= cooldown_started_at as f32 + delay_before_open
    }

    /// `isReadyToEjectItems(level, timeBetweenEjections, targetCooldownLength)`.
    pub fn is_ready_to_eject_items(&self, game_time: i64, time_between_ejections: f32, target_cooldown_length: i32) -> bool {
        let cooldown_started_at = self.cooldown_ends_at.wrapping_sub(i64::from(target_cooldown_length));
        // Java `float % float`: the IEEE remainder with the sign of the dividend, like Rust's `%`
        (game_time.wrapping_sub(cooldown_started_at) as f32) % time_between_ejections == 0.0
    }

    /// `isCooldownFinished(level)`.
    pub fn is_cooldown_finished(&self, game_time: i64) -> bool {
        game_time >= self.cooldown_ends_at
    }

    /// `getUpdateTag(state)`.
    pub fn get_update_tag(&self, active: bool) -> mc_nbt::CompoundTag {
        let mut tag = mc_nbt::CompoundTag::new();
        if active {
            tag.put_long("next_mob_spawns_at", self.next_mob_spawns_at);
        }
        if let Some(spawn_data) = &self.next_spawn_data
            && let Ok(encoded) = mc_dfu::encode_nbt(&SpawnData::codec(), spawn_data).into_result()
        {
            tag.put("spawn_data", encoded);
        }
        tag
    }
}

/// `WeightedList.builder().add(drop.copyWithCount(1), drop.getCount())` over the loot drops.
pub fn dispensing_from_drops(drops: Vec<ItemStack>) -> WeightedList<ItemStack> {
    let items = drops
        .into_iter()
        .map(|drop| Weighted { weight: drop.count(), value: drop.copy_with_count(1) })
        .collect();
    WeightedList::of(items).unwrap_or_default()
}
