//! `WardenSpawnTracker` logic (the state is [`WardenSpawnTracker`] in `server_player.rs`, saved with the player).

use super::ServerPlayer;
use super::server_player::WardenSpawnTracker;
use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::EntityType;

pub const MAX_WARNING_LEVEL: i32 = 4;
const PLAYER_SEARCH_RADIUS: f64 = 16.0;
const WARNING_CHECK_DIAMETER: f64 = 48.0;
const DECREASE_WARNING_LEVEL_EVERY_INTERVAL: i32 = 12000;
const WARNING_LEVEL_INCREASE_COOLDOWN: i32 = 200;

impl WardenSpawnTracker {
    /// `tick()`.
    pub fn tick(&mut self) {
        if self.ticks_since_last_warning >= DECREASE_WARNING_LEVEL_EVERY_INTERVAL {
            self.decrease_warning_level();
            self.ticks_since_last_warning = 0;
        } else {
            self.ticks_since_last_warning += 1;
        }
        if self.cooldown_ticks > 0 {
            self.cooldown_ticks -= 1;
        }
    }

    /// `reset()`.
    pub fn reset(&mut self) {
        *self = WardenSpawnTracker::default();
    }

    fn on_cooldown(&self) -> bool {
        self.cooldown_ticks > 0
    }

    fn increase_warning_level(&mut self) {
        if !self.on_cooldown() {
            self.ticks_since_last_warning = 0;
            self.cooldown_ticks = WARNING_LEVEL_INCREASE_COOLDOWN;
            self.set_warning_level(self.get_warning_level() + 1);
        }
    }

    fn decrease_warning_level(&mut self) {
        self.set_warning_level(self.get_warning_level() - 1);
    }

    /// `setWarningLevel(warningLevel)`.
    pub fn set_warning_level(&mut self, warning_level: i32) {
        self.warning_level = mth::clamp_i32(warning_level, 0, MAX_WARNING_LEVEL);
    }

    /// `getWarningLevel()`.
    pub fn get_warning_level(&self) -> i32 {
        self.warning_level
    }

    /// `WardenSpawnTracker.tryWarn(level, pos, triggerPlayer)`: the new warning level, `None` when a warden is near or
    /// a player is on cooldown.
    pub fn try_warn(
        level: &mut ServerLevel,
        pos: BlockPos,
        trigger_player: EntityId,
    ) -> Option<i32> {
        if has_nearby_warden(level, pos) {
            return None;
        }
        let mut players = get_nearby_players(level, pos);
        if !players.contains(&trigger_player) {
            players.push(trigger_player);
        }
        let trackers: Vec<WardenSpawnTracker> = players
            .iter()
            .map(|id| tracker_of(level, *id))
            .collect::<Option<_>>()?;
        if trackers.iter().any(WardenSpawnTracker::on_cooldown) {
            return None;
        }
        // `Stream.max(Comparator.comparingInt(..))` keeps the first of equal maxima
        let mut spawn_tracker = trackers[0];
        for tracker in &trackers[1..] {
            if tracker.get_warning_level() > spawn_tracker.get_warning_level() {
                spawn_tracker = *tracker;
            }
        }
        spawn_tracker.increase_warning_level();
        for id in players {
            // the trigger player may be the lent entity that is stepping on the shrieker
            level.with_player(id, |_, player| player.warden_spawn_tracker = spawn_tracker);
        }
        Some(spawn_tracker.warning_level)
    }
}

fn tracker_of(level: &ServerLevel, id: EntityId) -> Option<WardenSpawnTracker> {
    level
        .get_entity(id)?
        .as_any()
        .downcast_ref::<ServerPlayer>()
        .map(|p| p.warden_spawn_tracker)
}

/// `hasNearbyWarden(level, pos)`.
fn has_nearby_warden(level: &ServerLevel, pos: BlockPos) -> bool {
    let area_to_check = AABB::of_size(
        Vec3::at_center_of(&pos),
        WARNING_CHECK_DIAMETER,
        WARNING_CHECK_DIAMETER,
        WARNING_CHECK_DIAMETER,
    );
    !level
        .get_entities_of_class(
            &area_to_check,
            |e| e.get_type() == EntityType::WARDEN,
            |_| true,
        )
        .is_empty()
}

/// `getNearbyPlayers(level, pos)`.
fn get_nearby_players(level: &ServerLevel, pos: BlockPos) -> Vec<EntityId> {
    let origin = Vec3::at_center_of(&pos);
    level
        .players()
        .iter()
        .copied()
        .filter(|id| {
            level.get_entity(*id).is_some_and(|p| {
                !p.is_spectator()
                    && p.position().closer_than(origin, PLAYER_SEARCH_RADIUS)
                    && p.is_alive()
            })
        })
        .collect()
}
