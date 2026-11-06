//! `TrialSpawner`: the logic of the trial spawner block entity (state machine entry, mob spawning, reward ejection,
//! ominous transitions). The block entity owns it and mirrors its block state into it (`StateAccessor`).

use super::config::{FullConfig, TrialSpawnerConfig};
use super::player_detector::{PlayerDetector, in_line_of_sight};
use super::state::TrialSpawnerStateExt;
use super::state_data::{Packed, TrialSpawnerStateData, dispensing_from_drops};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::{EntitySpawnReason, TrialSpawnerState};
use mc_data::{EntityType, GameEvent, MobEffect, SoundEvent, props};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::set_changed;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::factory::{self, EntitySpawnRequest};
use mc_game_core::world::entity::{Entity, RemovalReason};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::base_spawner::{
    load_entity_tree, read_or_random_spawn_pos, spawn_aabb,
};
use mc_game_core::world::level::collision::CollisionGetter;
use mc_game_core::world::level::spawn_data::{SpawnData, has_no_configuration};
use mc_game_core::world::level::spawner_mob_seam::{self, MobSpawnSteps};
use mc_game_core::world::level::storage::loot::loading::get_loot_table;
use mc_game_core::world::level::storage::loot::{LootContextParamSets, LootParams};
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_protocol::packets::play::level::SoundSource;
use std::sync::Arc;
use uuid::Uuid;

/// `MAX_MOB_TRACKING_DISTANCE_SQR` (`Mth.square(47)`).
const MAX_MOB_TRACKING_DISTANCE_SQR: f64 = 47.0 * 47.0;
/// `TRIAL_OMEN_PER_BAD_OMEN_LEVEL`.
const TRIAL_OMEN_PER_BAD_OMEN_LEVEL: i32 = 18000;

/// `TrialSpawner.FlameParticle`, as the data of level events 3011/3012.
pub const FLAME_NORMAL: i32 = 0;
pub const FLAME_OMINOUS: i32 = 1;

pub struct TrialSpawner {
    pub data: TrialSpawnerStateData,
    config: FullConfig,
    pos: BlockPos,
    /// `StateAccessor`: the block entity's cached block state (`getBlockState()`).
    block_state: BlockState,
    player_detector: PlayerDetector,
    is_ominous: bool,
}

impl TrialSpawner {
    pub fn new(pos: BlockPos, block_state: BlockState) -> Self {
        Self {
            data: TrialSpawnerStateData::default(),
            config: FullConfig::default(),
            pos,
            block_state,
            player_detector: PlayerDetector::NoCreativePlayers,
            is_ominous: false,
        }
    }

    /// The block entity's `setBlockState(state)`.
    pub fn set_cached_block_state(&mut self, state: BlockState) {
        self.block_state = state;
    }

    pub fn full_config(&self) -> &FullConfig {
        &self.config
    }

    /// `activeConfig()`.
    pub fn active_config(&self, level: &ServerLevel) -> Arc<TrialSpawnerConfig> {
        if self.is_ominous {
            self.config.ominous.value(level)
        } else {
            self.config.normal.value(level)
        }
    }

    /// `ominousConfig()`.
    pub fn ominous_config(&self, level: &ServerLevel) -> Arc<TrialSpawnerConfig> {
        self.config.ominous.value(level)
    }

    /// `load(input)`.
    pub fn load(&mut self, tag: &mc_nbt::CompoundTag) {
        let root = mc_nbt::Tag::Compound(tag.clone());
        if let Some(packed) =
            mc_dfu::parse_nbt(&Packed::map_codec().codec(), &root).result_or_partial()
        {
            self.data.apply(packed);
        }
        self.config = mc_dfu::parse_nbt(&FullConfig::map_codec().codec(), &root)
            .result_or_partial()
            .unwrap_or_default();
    }

    /// `store(output)`.
    pub fn store(&self, tag: &mut mc_nbt::CompoundTag) {
        if let Ok(mc_nbt::Tag::Compound(packed)) =
            mc_dfu::encode_nbt(&Packed::map_codec().codec(), &self.data.pack()).into_result()
        {
            tag.merge(&packed);
        }
        if let Ok(mc_nbt::Tag::Compound(config)) =
            mc_dfu::encode_nbt(&FullConfig::map_codec().codec(), &self.config).into_result()
        {
            tag.merge(&config);
        }
    }

    /// `applyOminous(level, spawnerPos)`.
    pub fn apply_ominous(&mut self, level: &mut ServerLevel, spawner_pos: BlockPos) {
        let state = level.get_block_state(spawner_pos).set(props::OMINOUS, true);
        level.set_block_and_update(spawner_pos, state);
        level.level_event(None, 3020, spawner_pos, 1);
        self.is_ominous = true;
        self.reset_after_becoming_ominous(level);
    }

    /// `removeOminous(level, spawnerPos)`.
    pub fn remove_ominous(&mut self, level: &mut ServerLevel, spawner_pos: BlockPos) {
        let state = level
            .get_block_state(spawner_pos)
            .set(props::OMINOUS, false);
        level.set_block_and_update(spawner_pos, state);
        self.is_ominous = false;
    }

    pub fn is_ominous(&self) -> bool {
        self.is_ominous
    }

    pub fn target_cooldown_length(&self) -> i32 {
        self.config.target_cooldown_length
    }

    pub fn required_player_range(&self) -> i32 {
        self.config.required_player_range
    }

    pub fn player_detector(&self) -> PlayerDetector {
        self.player_detector
    }

    /// `getState()` (`INACTIVE` when the cached block state has no such property).
    pub fn get_state(&self) -> TrialSpawnerState {
        if self.block_state.has_property(props::TRIAL_SPAWNER_STATE) {
            self.block_state.get(props::TRIAL_SPAWNER_STATE)
        } else {
            TrialSpawnerState::Inactive
        }
    }

    /// `setState(level, state)` (`TrialSpawnerBlockEntity.setState`).
    pub fn set_state(&mut self, level: &mut ServerLevel, state: TrialSpawnerState) {
        set_changed(level, self.pos, self.block_state);
        level.set_block_and_update(
            self.pos,
            self.block_state.set(props::TRIAL_SPAWNER_STATE, state),
        );
    }

    /// `markUpdated()` (`TrialSpawnerBlockEntity.markUpdated`).
    pub fn mark_updated(&self, level: &mut ServerLevel) {
        set_changed(level, self.pos, self.block_state);
        level.send_block_updated(self.pos, self.block_state, self.block_state, 3);
    }

    /// `canSpawnInLevel(level)`.
    pub fn can_spawn_in_level(&self, level: &ServerLevel) -> bool {
        if !level.spawner_blocks_work() {
            return false;
        }
        level.difficulty() != mc_world::level_storage::Difficulty::Peaceful && level.spawn_mobs()
    }

    /// `getOrCreateNextSpawnData(trialSpawner, random)`.
    pub fn get_or_create_next_spawn_data(&mut self, level: &mut ServerLevel) -> SpawnData {
        if let Some(next) = &self.data.next_spawn_data {
            return next.clone();
        }
        let potentials = self
            .active_config(level)
            .spawn_potentials_definition
            .clone();
        let selected = if potentials.is_empty() {
            None
        } else {
            potentials.get_random(level.random()).cloned()
        };
        let next = selected.unwrap_or_else(SpawnData::empty);
        self.data.next_spawn_data = Some(next.clone());
        self.mark_updated(level);
        next
    }

    /// `hasMobToSpawn(trialSpawner, random)`.
    pub fn has_mob_to_spawn(&mut self, level: &mut ServerLevel) -> bool {
        let has_next = matches!(
            self.get_or_create_next_spawn_data(level)
                .entity_to_spawn
                .get("id"),
            Some(mc_nbt::Tag::String(_))
        );
        has_next
            || !self
                .active_config(level)
                .spawn_potentials_definition
                .is_empty()
    }

    /// `getOrCreateDisplayEntity(trialSpawner, level, state)` (only whether it exists matters on the server).
    pub fn get_or_create_display_entity(
        &mut self,
        level: &mut ServerLevel,
        state: TrialSpawnerState,
    ) -> bool {
        if !state.has_spinning_mob() {
            return false;
        }
        if !self.data.display_entity_created {
            let tag = self.get_or_create_next_spawn_data(level).entity_to_spawn;
            if matches!(tag.get("id"), Some(mc_nbt::Tag::String(_))) {
                let mut out = Vec::new();
                factory::load_entity_recursive(
                    &tag,
                    level,
                    EntitySpawnRequest {
                        reason: EntitySpawnReason::TrialSpawner,
                        ignore_checks: true,
                    },
                    &mut out,
                );
                self.data.display_entity_created = !out.is_empty();
            }
        }
        self.data.display_entity_created
    }

    /// `tryDetectPlayers(level, pos, trialSpawner)`.
    pub fn try_detect_players(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        let throttled = (pos.as_long().wrapping_add(level.game_time())) % 20 != 0;
        if throttled {
            return;
        }
        if self.get_state() == TrialSpawnerState::Cooldown && self.is_ominous {
            return;
        }
        let range = f64::from(self.required_player_range());
        let in_line_of_sight_players = self.player_detector.detect(level, pos, range, true);
        let became_ominous;
        if !self.is_ominous && !in_line_of_sight_players.is_empty() {
            let with_effect = find_player_with_ominous_effect(level, &in_line_of_sight_players);
            if let Some((player, effect)) = with_effect {
                if effect == MobEffect::BAD_OMEN {
                    transform_bad_omen_into_trial_omen(level, player);
                }
                let eye = level
                    .get_entity(player)
                    .map(|p| p.base().get_eye_position())
                    .unwrap_or(Vec3::ZERO);
                level.level_event(None, 3020, BlockPos::containing(eye.x, eye.y, eye.z), 0);
                self.apply_ominous(level, pos);
            }
            became_ominous = with_effect.is_some();
        } else {
            became_ominous = false;
        }
        if self.get_state() != TrialSpawnerState::Cooldown || became_ominous {
            let searching_for_first_player = self.data.detected_players.is_empty();
            let found = if searching_for_first_player {
                in_line_of_sight_players
            } else {
                self.player_detector.detect(level, pos, range, false)
            };
            if self.data.detected_players.extend_from(found) {
                self.data.next_mob_spawns_at =
                    (level.game_time() + 40).max(self.data.next_mob_spawns_at);
                if !became_ominous {
                    let event = if self.is_ominous { 3019 } else { 3013 };
                    level.level_event(None, event, pos, self.data.detected_players.len() as i32);
                }
            }
        }
    }

    /// `resetAfterBecomingOminous(trialSpawner, level)`.
    fn reset_after_becoming_ominous(&mut self, level: &mut ServerLevel) {
        let mobs: Vec<Uuid> = self.data.current_mobs.iter().copied().collect();
        for uuid in mobs {
            let Some(id) = level.get_entity_by_uuid(uuid).map(|e| e.id()) else {
                continue;
            };
            let block_pos = level.get_entity(id).map(|e| e.block_position());
            if let Some(block_pos) = block_pos {
                level.level_event(None, 3012, block_pos, FLAME_NORMAL);
            }
            spawner_mob_seam::drop_preserved_equipment(level, id);
            level.with_entity(id, |_, e| e.remove(RemovalReason::Discarded));
        }
        let ominous = self.ominous_config(level);
        if !ominous.spawn_potentials_definition.is_empty() {
            self.data.next_spawn_data = None;
        }
        self.data.total_mobs_spawned = 0;
        self.data.current_mobs.clear();
        self.data.next_mob_spawns_at = level.game_time() + i64::from(ominous.ticks_between_spawn);
        self.mark_updated(level);
        self.data.cooldown_ends_at = level.game_time() + ominous.ticks_between_item_spawners();
    }

    /// `spawnMob(level, spawnerPos)`: the UUID of the spawned entity.
    pub fn spawn_mob(&mut self, level: &mut ServerLevel, spawner_pos: BlockPos) -> Option<Uuid> {
        let next_spawn_data = self.get_or_create_next_spawn_data(level);
        let entity_type: EntityType = factory::by(&next_spawn_data.entity_to_spawn)?;
        let spawn_range = self.active_config(level).spawn_range;
        let spawn_pos = read_or_random_spawn_pos(
            &next_spawn_data.entity_to_spawn,
            level,
            spawner_pos,
            spawn_range,
        );
        if !level.no_collision(
            None,
            None,
            &spawn_aabb(entity_type, spawn_pos.x, spawn_pos.y, spawn_pos.z),
        ) {
            return None;
        }
        if !in_line_of_sight(level, Vec3::at_center_of(&spawner_pos), spawn_pos) {
            return None;
        }
        let spawn_block_pos = BlockPos::containing(spawn_pos.x, spawn_pos.y, spawn_pos.z);
        if !spawner_mob_seam::check_spawn_placements(
            level,
            entity_type,
            EntitySpawnReason::TrialSpawner,
            spawn_block_pos,
        ) {
            return None;
        }
        if let Some(rules) = &next_spawn_data.custom_spawn_rules
            && !rules.is_valid_position(spawn_block_pos, &*level)
        {
            return None;
        }
        let mut entities = load_entity_tree(
            &next_spawn_data.entity_to_spawn,
            level,
            EntitySpawnReason::TrialSpawner,
        )?;
        for entity in &mut entities {
            let yaw = level.random().next_float() * 360.0;
            entity.snap_to(spawn_pos.x, spawn_pos.y, spawn_pos.z, yaw, 0.0);
        }
        let steps = MobSpawnSteps {
            check_spawn_rules: false,
            finalize_spawn: has_no_configuration(&next_spawn_data.entity_to_spawn),
            set_persistence_required: true,
            equipment: next_spawn_data.equipment.as_ref(),
        };
        if !spawner_mob_seam::prepare_mob(
            entities[0].as_mut(),
            level,
            EntitySpawnReason::TrialSpawner,
            &steps,
        ) {
            return None;
        }
        let (uuid, id) = (entities[0].uuid(), entities[0].id());
        if !level.try_add_fresh_entity_with_passengers(entities) {
            return None;
        }
        let flame = if self.is_ominous {
            FLAME_OMINOUS
        } else {
            FLAME_NORMAL
        };
        level.level_event(None, 3011, spawner_pos, flame);
        level.level_event(None, 3012, spawn_block_pos, flame);
        level.game_event_at_block(
            GameEvent::ENTITY_PLACE,
            spawn_block_pos,
            &mc_game_core::world::level::gameevent::GameEventContext::of_entity(Some(id)),
        );
        Some(uuid)
    }

    /// `ejectReward(level, pos, ejectingLootTable)`.
    pub fn eject_reward(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        ejecting_loot_table: &Identifier,
    ) {
        let table = get_loot_table(&level.registries, ejecting_loot_table);
        let drops = {
            let params = LootParams::builder(level).create(&LootContextParamSets::EMPTY);
            table.get_random_items(&params)
        };
        if !drops.is_empty() {
            for item in drops {
                crate::world::block_entity::loot_roll::spawn_item(
                    level,
                    item,
                    2,
                    mc_core::direction::Direction::Up,
                    Vec3::at_bottom_center_of(&pos)
                        .relative(mc_core::direction::Direction::Up, 1.2),
                );
            }
            level.level_event(None, 3014, pos, 0);
        }
    }

    /// `getDispensingItems(level, config, pos)` (`None` for the empty list, which is not cached).
    pub fn get_dispensing_items(
        &mut self,
        level: &mut ServerLevel,
        config: &TrialSpawnerConfig,
        pos: BlockPos,
    ) -> Option<&mc_game_core::util::weighted_list::WeightedList<ItemStack>> {
        if self.data.dispensing.is_none() {
            let table = get_loot_table(&level.registries, &config.items_to_drop_when_ominous);
            let seed = low_resolution_position(level, pos);
            let drops = {
                let params = LootParams::builder(level).create(&LootContextParamSets::EMPTY);
                table.get_random_items_with_seed(&params, seed)
            };
            if drops.is_empty() {
                return None;
            }
            self.data.dispensing = Some(dispensing_from_drops(drops));
        }
        self.data.dispensing.as_ref()
    }

    /// `tickServer(serverLevel, spawnerPos, isOminous)`.
    pub fn tick_server(
        &mut self,
        level: &mut ServerLevel,
        spawner_pos: BlockPos,
        is_ominous: bool,
    ) {
        self.is_ominous = is_ominous;
        let current_state = self.get_state();
        let spawner_block = spawner_pos;
        if self
            .data
            .current_mobs
            .remove_if(|id| should_mob_be_untracked(level, spawner_block, id))
        {
            self.data.next_mob_spawns_at =
                level.game_time() + i64::from(self.active_config(level).ticks_between_spawn);
        }
        let next_state = current_state.tick_and_get_next(spawner_pos, self, level);
        if next_state != current_state {
            self.set_state(level, next_state);
        }
    }

    /// `overrideEntityToSpawn(type, level)`.
    pub fn override_entity_to_spawn(&mut self, level: &mut ServerLevel, ty: EntityType) {
        self.data.reset();
        self.config = self.config.override_entity(level, ty);
        self.set_state(level, TrialSpawnerState::Inactive);
    }

    /// `overrideEntityToSpawn(entityData, level)`.
    pub fn override_entity_data(
        &mut self,
        level: &mut ServerLevel,
        ty: EntityType,
        data: &mc_nbt::CompoundTag,
    ) {
        self.data.reset();
        self.config = self.config.override_entity_data(level, ty, data);
        self.set_state(level, TrialSpawnerState::Inactive);
    }
}

fn should_mob_be_untracked(level: &ServerLevel, spawner_pos: BlockPos, id: &Uuid) -> bool {
    match level.get_entity_by_uuid(*id) {
        None => true,
        Some(entity) => {
            !entity.is_alive()
                || entity.block_position().dist_sqr(&spawner_pos) > MAX_MOB_TRACKING_DISTANCE_SQR
        }
    }
}

/// `findPlayerWithOminousEffect(level, inLineOfSightPlayers)`: a player with trial omen, else the last one with bad omen.
fn find_player_with_ominous_effect(
    level: &ServerLevel,
    players: &[Uuid],
) -> Option<(mc_game_core::world::entity::EntityId, MobEffect)> {
    let mut player_with_bad_omen = None;
    for uuid in players {
        let Some(id) = level.get_player_by_uuid(*uuid) else {
            continue;
        };
        let Some(player) = level.get_entity(id) else {
            continue;
        };
        if player.has_effect(MobEffect::TRIAL_OMEN) {
            return Some((id, MobEffect::TRIAL_OMEN));
        }
        if player.has_effect(MobEffect::BAD_OMEN) {
            player_with_bad_omen = Some(id);
        }
    }
    player_with_bad_omen.map(|id| (id, MobEffect::BAD_OMEN))
}

/// `transformBadOmenIntoTrialOmen(player)`.
fn transform_bad_omen_into_trial_omen(
    level: &mut ServerLevel,
    player: mc_game_core::world::entity::EntityId,
) {
    level.with_entity(player, |level, p| {
        if let Some(bad_omen) = p.get_effect(MobEffect::BAD_OMEN) {
            let amplifier = bad_omen.amplifier + 1;
            let duration = TRIAL_OMEN_PER_BAD_OMEN_LEVEL.wrapping_mul(amplifier);
            p.remove_effect(level, MobEffect::BAD_OMEN);
            p.add_effect(
                level,
                MobEffectInstance::of(MobEffect::TRIAL_OMEN, duration, 0),
                None,
            );
        }
    });
}

/// `lowResolutionPosition(level, pos)`.
fn low_resolution_position(level: &ServerLevel, pos: BlockPos) -> i64 {
    let low = BlockPos::new(
        mc_core::math::mth::floor_f32(pos.x() as f32 / 30.0),
        mc_core::math::mth::floor_f32(pos.y() as f32 / 20.0),
        mc_core::math::mth::floor_f32(pos.z() as f32 / 30.0),
    );
    level
        .server_state
        .loot
        .world_seed
        .wrapping_add(low.as_long())
}

/// The sounds the state machine plays (`SoundSource.BLOCKS`, volume 1, pitch 1).
pub fn play_block_sound(level: &mut ServerLevel, pos: BlockPos, sound: SoundEvent) {
    level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
}
