//! `TrialSpawnerState`: per-state constants and the state machine step (`tickAndGetNext`).

use super::trial_spawner::{TrialSpawner, play_block_sound};
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid, CollisionContext};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::enums::TrialSpawnerState;
use mc_data::SoundEvent;
use uuid::Uuid;

/// `TIME_BETWEEN_EACH_EJECTION` (`Mth.floor(30.0F)`).
const TIME_BETWEEN_EACH_EJECTION: f32 = 30.0;
/// `DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB`.
const DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB: f32 = 40.0;

pub trait TrialSpawnerStateExt: Copy {
    /// `lightLevel()`.
    fn light_level(self) -> i32;
    /// `spinningMobSpeed()`.
    fn spinning_mob_speed(self) -> f64;
    /// `hasSpinningMob()`.
    fn has_spinning_mob(self) -> bool {
        self.spinning_mob_speed() >= 0.0
    }
    /// `isCapableOfSpawning()`.
    fn is_capable_of_spawning(self) -> bool;
    /// `tickAndGetNext(spawnerPos, trialSpawner, serverLevel)`.
    fn tick_and_get_next(self, spawner_pos: BlockPos, trial_spawner: &mut TrialSpawner, level: &mut ServerLevel) -> TrialSpawnerState;
}

impl TrialSpawnerStateExt for TrialSpawnerState {
    fn light_level(self) -> i32 {
        match self {
            TrialSpawnerState::Inactive | TrialSpawnerState::Cooldown => 0,
            TrialSpawnerState::WaitingForPlayers => 4,
            _ => 8,
        }
    }

    fn spinning_mob_speed(self) -> f64 {
        match self {
            TrialSpawnerState::WaitingForPlayers => 200.0,
            TrialSpawnerState::Active => 1000.0,
            _ => -1.0,
        }
    }

    fn is_capable_of_spawning(self) -> bool {
        matches!(self, TrialSpawnerState::WaitingForPlayers | TrialSpawnerState::Active)
    }

    fn tick_and_get_next(self, spawner_pos: BlockPos, ts: &mut TrialSpawner, level: &mut ServerLevel) -> TrialSpawnerState {
        use TrialSpawnerState::*;
        let config = ts.active_config(level);
        match self {
            Inactive => {
                if ts.get_or_create_display_entity(level, WaitingForPlayers) { WaitingForPlayers } else { self }
            }
            WaitingForPlayers => {
                if !ts.can_spawn_in_level(level) {
                    ts.data.reset_statistics();
                    self
                } else if !ts.has_mob_to_spawn(level) {
                    Inactive
                } else {
                    ts.try_detect_players(level, spawner_pos);
                    if ts.data.detected_players.is_empty() { self } else { Active }
                }
            }
            Active => {
                if !ts.can_spawn_in_level(level) {
                    ts.data.reset_statistics();
                    WaitingForPlayers
                } else if !ts.has_mob_to_spawn(level) {
                    Inactive
                } else {
                    let additional_players = ts.data.count_additional_players();
                    ts.try_detect_players(level, spawner_pos);
                    if ts.is_ominous() {
                        spawn_ominous_item_spawner(level, spawner_pos, ts);
                    }
                    if ts.data.has_finished_spawning_all_mobs(config.calculate_target_total_mobs(additional_players)) {
                        if ts.data.have_all_current_mobs_died() {
                            ts.data.cooldown_ends_at = level.game_time() + i64::from(ts.target_cooldown_length());
                            ts.data.total_mobs_spawned = 0;
                            ts.data.next_mob_spawns_at = 0;
                            return WaitingForRewardEjection;
                        }
                    } else if ts.data.is_ready_to_spawn_next_mob(level.game_time(), config.calculate_target_simultaneous_mobs(additional_players))
                        && let Some(entity_id) = ts.spawn_mob(level, spawner_pos)
                    {
                        ts.data.current_mobs.insert(entity_id);
                        ts.data.total_mobs_spawned += 1;
                        ts.data.next_mob_spawns_at = level.game_time() + i64::from(config.ticks_between_spawn);
                        if let Some(entry) = config.spawn_potentials_definition.get_random(level.random()).cloned() {
                            ts.data.next_spawn_data = Some(entry);
                            ts.mark_updated(level);
                        }
                    }
                    self
                }
            }
            WaitingForRewardEjection => {
                if ts.data.is_ready_to_open_shutter(level.game_time(), DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB, ts.target_cooldown_length()) {
                    play_block_sound(level, spawner_pos, SoundEvent::BLOCK_TRIAL_SPAWNER_OPEN_SHUTTER);
                    EjectingReward
                } else {
                    self
                }
            }
            EjectingReward => {
                if !ts.data.is_ready_to_eject_items(level.game_time(), TIME_BETWEEN_EACH_EJECTION, ts.target_cooldown_length()) {
                    self
                } else if ts.data.detected_players.is_empty() {
                    play_block_sound(level, spawner_pos, SoundEvent::BLOCK_TRIAL_SPAWNER_CLOSE_SHUTTER);
                    ts.data.ejecting_loot_table = None;
                    Cooldown
                } else {
                    if ts.data.ejecting_loot_table.is_none() {
                        ts.data.ejecting_loot_table = config.loot_tables_to_eject.get_random(level.random()).cloned();
                    }
                    if let Some(table) = ts.data.ejecting_loot_table.clone() {
                        ts.eject_reward(level, spawner_pos, &table);
                    }
                    let first = ts.data.detected_players.iter().next().copied();
                    if let Some(first) = first {
                        ts.data.detected_players.remove(&first);
                    }
                    self
                }
            }
            Cooldown => {
                ts.try_detect_players(level, spawner_pos);
                if !ts.data.detected_players.is_empty() {
                    ts.data.total_mobs_spawned = 0;
                    ts.data.next_mob_spawns_at = 0;
                    Active
                } else if ts.data.is_cooldown_finished(level.game_time()) {
                    ts.remove_ominous(level, spawner_pos);
                    ts.data.reset();
                    WaitingForPlayers
                } else {
                    self
                }
            }
        }
    }
}

/// `spawnOminousOminousItemSpawner(level, trialSpawnerPos, trialSpawner)`.
fn spawn_ominous_item_spawner(level: &mut ServerLevel, trial_spawner_pos: BlockPos, ts: &mut TrialSpawner) {
    let config = ts.active_config(level);
    let item = match ts.get_dispensing_items(level, &config, trial_spawner_pos) {
        Some(list) => list.get_random(level.random()).cloned(),
        None => None,
    };
    let Some(item_to_dispense) = item.filter(|i| !i.is_empty()) else { return };
    if level.game_time() < ts.data.cooldown_ends_at {
        return;
    }
    if let Some(pos) = calculate_position_to_spawn_spawner(level, trial_spawner_pos, ts) {
        let spawner = crate::world::entity::ominous_item_spawner::OminousItemSpawner::create(level, item_to_dispense);
        let mut spawner = spawner;
        spawner.snap_to_pos(pos.x, pos.y, pos.z);
        level.add_fresh_entity(Box::new(spawner));
        let pitch = (level.random().next_float() - level.random().next_float()) * 0.2 + 1.0;
        level.play_sound(
            None,
            BlockPos::containing(pos.x, pos.y, pos.z),
            SoundEvent::BLOCK_TRIAL_SPAWNER_SPAWN_ITEM_BEGIN,
            mc_protocol::packets::play::level::SoundSource::Blocks,
            1.0,
            pitch,
        );
        ts.data.cooldown_ends_at = level.game_time() + ts.ominous_config(level).ticks_between_item_spawners();
    }
}

/// `calculatePositionToSpawnSpawner(level, trialSpawnerPos, trialSpawner, data)`.
fn calculate_position_to_spawn_spawner(level: &mut ServerLevel, trial_spawner_pos: BlockPos, ts: &TrialSpawner) -> Option<Vec3> {
    let center = Vec3::at_center_of(&trial_spawner_pos);
    let range = f64::from(ts.required_player_range());
    let nearby_players: Vec<EntityId> = ts
        .data
        .detected_players
        .iter()
        .filter_map(|uuid| level.get_player_by_uuid(*uuid))
        .filter(|&id| {
            level.get_entity(id).is_some_and(|p| {
                let creative = p.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().is_some_and(crate::world::entity::ServerPlayer::is_creative);
                !creative && !p.is_spectator() && p.is_alive() && p.base().distance_to_sqr_vec(center) <= range * range
            })
        })
        .collect();
    if nearby_players.is_empty() {
        return None;
    }
    let entity = select_entity_to_spawn_item_above(&nearby_players, ts.data.current_mobs.iter().copied().collect(), ts, trial_spawner_pos, level)?;
    calculate_position_above(entity, level)
}

/// `calculatePositionAbove(entityToSpawnItemAbove, level)`.
fn calculate_position_above(entity: EntityId, level: &mut ServerLevel) -> Option<Vec3> {
    let (entity_pos, bb_height) = {
        let e = level.get_entity(entity)?;
        (e.base().position(), e.base().get_bb_height())
    };
    let try_spawn_pos = entity_pos.relative(Direction::Up, f64::from(bb_height + 2.0 + level.random().next_int_bounded(4) as f32));
    let hit = level.clip(&ClipContext::new(entity_pos, try_spawn_pos, ClipBlock::Visual, ClipFluid::None, CollisionContext::empty()));
    let down = Vec3::at_center_of(&hit.block_pos).relative(Direction::Down, 1.0);
    let block_pos_down = BlockPos::containing(down.x, down.y, down.z);
    let state = level.get_block_state(block_pos_down);
    if state.get_collision_shape(&*level, block_pos_down).is_empty() { Some(down) } else { None }
}

/// `selectEntityToSpawnItemAbove(nearbyPlayers, mobIds, trialSpawner, spawnerPos, level)`.
fn select_entity_to_spawn_item_above(nearby_players: &[EntityId], mob_ids: Vec<Uuid>, ts: &TrialSpawner, spawner_pos: BlockPos, level: &mut ServerLevel) -> Option<EntityId> {
    let center = Vec3::at_center_of(&spawner_pos);
    let range = f64::from(ts.required_player_range());
    let use_mobs = level.random().next_boolean();
    let eligible: Vec<EntityId> = if use_mobs {
        mob_ids
            .iter()
            .filter_map(|uuid| level.get_entity_by_uuid(*uuid))
            .filter(|e| e.is_alive() && e.base().distance_to_sqr_vec(center) <= range * range)
            .map(|e| e.id())
            .collect()
    } else {
        nearby_players.to_vec()
    };
    match eligible.len() {
        0 => None,
        1 => Some(eligible[0]),
        n => Some(eligible[level.random().next_int_bounded(n as i32) as usize]),
    }
}
