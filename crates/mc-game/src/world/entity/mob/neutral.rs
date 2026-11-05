//! `NeutralMob`: persistent anger (timer, target), `updatePersistentAnger`, NBT.
//!
//! State lives in [`NeutralData`] (`MobData.neutral`, set by the concrete class constructor); the interface's default
//! methods are free functions over `&mut dyn Entity`. Classes override `start_persistent_anger_timer` (an abstract method
//! of the interface) through the `Entity::start_persistent_anger_timer` hook.

use crate::server::level::ServerLevel;
use crate::world::entity::mob::ext;
use crate::world::entity::{Entity, EntityId};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use uuid::Uuid;

pub const NO_ANGER_END_TIME: i64 = -1;

#[derive(Debug)]
pub struct NeutralData {
    pub persistent_anger_end_time: i64,
    /// `EntityReference<LivingEntity>` persistent anger target.
    pub persistent_anger_target: Option<Uuid>,
}

impl Default for NeutralData {
    fn default() -> Self {
        Self { persistent_anger_end_time: NO_ANGER_END_TIME, persistent_anger_target: None }
    }
}

fn nd(e: &dyn Entity) -> &NeutralData {
    e.mob().and_then(|m| m.neutral.as_ref()).expect("neutral mob")
}

fn ndm(e: &mut dyn Entity) -> &mut NeutralData {
    e.mob_mut().and_then(|m| m.neutral.as_mut()).expect("neutral mob")
}

/// `getPersistentAngerEndTime()`.
pub fn get_persistent_anger_end_time(e: &dyn Entity) -> i64 {
    nd(e).persistent_anger_end_time
}

/// `setPersistentAngerEndTime(endTime)`.
pub fn set_persistent_anger_end_time(e: &mut dyn Entity, end_time: i64) {
    ndm(e).persistent_anger_end_time = end_time;
}

/// `setTimeToRemainAngry(remainingTime)`.
pub fn set_time_to_remain_angry(e: &mut dyn Entity, level: &ServerLevel, remaining_time: i64) {
    set_persistent_anger_end_time(e, level.game_time() + remaining_time);
}

/// `getPersistentAngerTarget()`.
pub fn get_persistent_anger_target(e: &dyn Entity) -> Option<Uuid> {
    nd(e).persistent_anger_target
}

/// `setPersistentAngerTarget(target)`.
pub fn set_persistent_anger_target(e: &mut dyn Entity, target: Option<Uuid>) {
    ndm(e).persistent_anger_target = target;
}

/// `addPersistentAngerSaveData(output)`.
pub fn add_persistent_anger_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    out.put_long("anger_end_time", get_persistent_anger_end_time(e));
    if let Some(uuid) = get_persistent_anger_target(e) {
        out.put_int_array("angry_at", mc_core::uuid_util::uuid_to_int_array(&uuid).to_vec());
    }
}

/// `readPersistentAngerSaveData(level, input)`. The `setTarget` of the reference is resolved later by the next
/// `updatePersistentAnger` (the referenced entity is not loaded yet while chunks load).
pub fn read_persistent_anger_save_data(e: &mut dyn Entity, level: Option<&ServerLevel>, input: &CompoundTag) {
    if let Some(t) = input.get("anger_end_time").and_then(|t| t.as_number()) {
        set_persistent_anger_end_time(e, t.long_value());
    } else if let Some(t) = input.get("AngerTime").and_then(|t| t.as_number()) {
        let game_time = level.map_or(0, |l| l.game_time());
        set_persistent_anger_end_time(e, game_time + i64::from(t.int_value()));
    } else {
        set_persistent_anger_end_time(e, -1);
    }
    set_persistent_anger_target(e, crate::world::entity::entity::read_uuid(input.get("angry_at")));
    if let Some(level) = level {
        let target = get_persistent_anger_target(e).and_then(|u| level.get_entity_by_uuid(u)).filter(|t| t.living().is_some()).map(|t| t.id());
        e.set_target(level, target);
    }
}

fn is_valid_player_target(level: &ServerLevel, target: &dyn Entity) -> bool {
    target.is_player() && !ext::is_creative_player(target) && !target.is_spectator() && level.difficulty() != Difficulty::Peaceful
}

/// `updatePersistentAnger(level, stayAngryIfTargetPresent)`.
pub fn update_persistent_anger(e: &mut dyn Entity, level: &mut ServerLevel, stay_angry_if_target_present: bool) {
    let previous_target = e.get_target();
    let persistent_target = get_persistent_anger_target(e);
    let previous_dead_mob = previous_target
        .and_then(|id| level.get_entity(id))
        .filter(|t| t.is_dead_or_dying() && persistent_target == Some(t.uuid()) && t.mob().is_some())
        .is_some();
    if previous_dead_mob {
        stop_being_angry(e, level);
        return;
    }
    let target = super::tick::get_target_checked(e, level);
    if let Some(target_id) = target {
        let target_uuid = level.get_entity(target_id).map(|t| t.uuid());
        let new_target = persistent_target.is_none() || persistent_target != target_uuid;
        if new_target {
            set_persistent_anger_target(e, target_uuid);
        }
        if new_target || stay_angry_if_target_present {
            e.start_persistent_anger_timer(level);
        }
    }
    let valid_player_target = target.and_then(|id| level.get_entity(id)).is_some_and(|t| is_valid_player_target(level, t));
    if persistent_target.is_some() && !is_angry(e, level) && (target.is_none() || !valid_player_target || !stay_angry_if_target_present) {
        stop_being_angry(e, level);
    }
    let persistent_player_invalid = persistent_target
        .and_then(|u| level.get_entity_by_uuid(u))
        .filter(|p| p.is_player())
        .is_some_and(|p| ext::is_creative_player(p) || p.is_spectator() || level.difficulty() == Difficulty::Peaceful);
    if persistent_player_invalid {
        stop_being_angry(e, level);
    }
}

/// `isAngryAt(entity, level)`.
pub fn is_angry_at(e: &dyn Entity, level: &ServerLevel, entity: &dyn Entity) -> bool {
    if !e.can_attack(level, entity) {
        return false;
    }
    if is_valid_player_target(level, entity) && is_angry_at_all_players(e, level) {
        return true;
    }
    get_persistent_anger_target(e) == Some(entity.uuid())
}

/// `isAngryAtAllPlayers(level)`.
pub fn is_angry_at_all_players(e: &dyn Entity, level: &ServerLevel) -> bool {
    level.universal_anger() && is_angry(e, level) && get_persistent_anger_target(e).is_none()
}

/// `isAngry()`.
pub fn is_angry(e: &dyn Entity, level: &ServerLevel) -> bool {
    let end_time = get_persistent_anger_end_time(e);
    end_time > 0 && end_time - level.game_time() > 0
}

/// `playerDied(level, player)`.
pub fn player_died(e: &mut dyn Entity, level: &mut ServerLevel, player: &dyn Entity) {
    if level.forgive_dead_players() && get_persistent_anger_target(e) == Some(player.uuid()) {
        stop_being_angry(e, level);
    }
}

/// `forgetCurrentTargetAndRefreshUniversalAnger()`.
pub fn forget_current_target_and_refresh_universal_anger(e: &mut dyn Entity, level: &mut ServerLevel) {
    stop_being_angry(e, level);
    e.start_persistent_anger_timer(level);
}

/// `stopBeingAngry()`.
pub fn stop_being_angry(e: &mut dyn Entity, level: &ServerLevel) {
    let tick_count = e.base().tick_count;
    if let Some(l) = e.living_mut() {
        l.last_hurt_by_mob = None;
        l.last_hurt_by_mob_timestamp = tick_count;
    }
    set_persistent_anger_target(e, None);
    e.set_target(level, None);
    set_persistent_anger_end_time(e, -1);
}

#[allow(dead_code)]
fn _unused(_: EntityId) {}
