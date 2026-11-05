//! `net.minecraft.world.entity.raid.Raider` (+ the `PatrollingMonster` layer under it): the base of illagers, witches and
//! ravagers. A concrete raider embeds [`RaiderData`], exposes it through `Entity::raider()`/`raider_mut()` and calls the
//! functions of this module as its `super` calls (see [`raider_entity!`](crate::raider_entity)).

use super::raid::{MAX_NO_ACTION_TIME as _, Raid, RaiderToken};
use super::raids;
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::mob::{self, MobData, navigation};
use crate::world::entity::monster::patrolling_monster::{self, LongDistancePatrolGoal, PatrolData};
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::synched_data::SynchedEntityDataBuilder;
use crate::world::entity::{Entity, EntityId, equipment::EquipmentSlot};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use crate::world::level::pathfinder::Path;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ClientboundTakeItemEntityPacket;
use std::collections::HashMap;

define_accessor!(pub IS_CELEBRATING: Boolean = Raider);

/// The fields of `Raider` (and of `PatrollingMonster`, [`RaiderData::patrol`]).
#[derive(Clone, Debug, Default)]
pub struct RaiderData {
    pub patrol: PatrolData,
    /// `raid`: the id of the raid in the level's `Raids` (see the module docs of `raid.rs`).
    pub raid: Option<i32>,
    pub wave: i32,
    pub can_join_raid: bool,
    pub ticks_outside_raid: i32,
    /// `RaidId` read from NBT, joined to the raid once the raider is in the level ([`resolve_pending_raid`]).
    pending_raid_id: Option<i32>,
}

/// Defines `IS_CELEBRATING` in `Raider.defineSynchedData` position (call first inside the `MobData::new` closure of every
/// raider class, before the class's own accessors).
pub fn define_synched_data(builder: &mut SynchedEntityDataBuilder) {
    builder.define(IS_CELEBRATING, false);
}

fn data(e: &dyn Entity) -> &RaiderData {
    e.raider().expect("raider")
}

fn data_mut(e: &mut dyn Entity) -> &mut RaiderData {
    e.raider_mut().expect("raider")
}

/// Implements the `Entity` methods every raider inherits (`Raider`/`PatrollingMonster` overrides) for a concrete type whose
/// `RaiderData` is the field `$raider`; list the overrides the class does NOT define itself:
/// `raider_entity!(raider; ai_step, update_no_action_time, die, hurt_server, can_join_patrol, remove_when_far_away, requires_custom_persistence, finalize_spawn, pick_up_item, nbt)`.
#[macro_export]
macro_rules! raider_entity {
    ($raider:ident; $($m:ident),* $(,)?) => {
        fn raider(&self) -> Option<&$crate::world::entity::raid::raider::RaiderData> {
            Some(&self.$raider)
        }
        fn raider_mut(&mut self) -> Option<&mut $crate::world::entity::raid::raider::RaiderData> {
            Some(&mut self.$raider)
        }
        $( $crate::raider_entity!(@method $m); )*
    };
    (@method ai_step) => {
        fn ai_step(&mut self, level: &mut $crate::server::level::ServerLevel) {
            $crate::world::entity::raid::raider::ai_step(self, level);
        }
    };
    (@method update_no_action_time) => {
        fn update_no_action_time(&mut self, _level: &$crate::server::level::ServerLevel) {
            $crate::world::entity::raid::raider::update_no_action_time(self);
        }
    };
    (@method die) => {
        fn die(&mut self, level: &mut $crate::server::level::ServerLevel, source: &$crate::world::damagesource::DamageSource) {
            $crate::world::entity::raid::raider::die(self, level, source);
        }
    };
    (@method hurt_server) => {
        fn hurt_server(&mut self, level: &mut $crate::server::level::ServerLevel, source: &$crate::world::damagesource::DamageSource, amount: f32) -> bool {
            $crate::world::entity::raid::raider::hurt_server(self, level, source, amount)
        }
    };
    (@method can_join_patrol) => {
        fn can_join_patrol(&self, level: &$crate::server::level::ServerLevel) -> bool {
            !$crate::world::entity::raid::raider::has_active_raid(self, level)
        }
    };
    (@method remove_when_far_away) => {
        fn remove_when_far_away(&self, dist_sqr: f64) -> bool {
            $crate::world::entity::raid::raider::remove_when_far_away(self, dist_sqr)
        }
    };
    (@method requires_custom_persistence) => {
        fn requires_custom_persistence(&self) -> bool {
            $crate::world::entity::raid::raider::requires_custom_persistence(self)
        }
    };
    (@method finalize_spawn) => {
        fn finalize_spawn(
            &mut self,
            level: &mut dyn $crate::world::level::server_level_accessor::ServerLevelAccessor,
            difficulty: $crate::world::difficulty_instance::DifficultyInstance,
            reason: mc_data::enums::EntitySpawnReason,
            group_data: Option<Box<dyn $crate::world::entity::spawn_group_data::SpawnGroupData>>,
        ) -> Option<Box<dyn $crate::world::entity::spawn_group_data::SpawnGroupData>> {
            $crate::world::entity::raid::raider::finalize_spawn(self, level, difficulty, reason, group_data)
        }
    };
    (@method pick_up_item) => {
        fn pick_up_item(&mut self, level: &mut $crate::server::level::ServerLevel, item: $crate::world::entity::EntityId) {
            $crate::world::entity::raid::raider::pick_up_item(self, level, item);
        }
    };
    (@method nbt) => {
        fn add_additional_save_data(&self, out: &mut mc_nbt::CompoundTag) {
            $crate::world::entity::raid::raider::add_additional_save_data(self, out);
        }
        fn read_additional_save_data(&mut self, input: &mc_nbt::CompoundTag) {
            $crate::world::entity::raid::raider::read_additional_save_data(self, input);
        }
    };
    (@method after_loaded) => {
        fn after_loaded_to_level(&mut self, level: &mut $crate::server::level::ServerLevel) {
            $crate::world::entity::raid::raider::resolve_pending_raid(self, level);
        }
    };
}

// ---- accessors -----------------------------------------------------------------------------------------------------

/// `canBeLeader()`.
pub fn can_be_leader(e: &dyn Entity) -> bool {
    e.can_be_leader()
}

pub use patrolling_monster::{find_patrol_target, get_patrol_target, has_patrol_target, is_patrol_leader, is_patrolling, set_patrol_leader, set_patrol_target, set_patrolling};

pub fn can_join_raid(e: &dyn Entity) -> bool {
    data(e).can_join_raid
}

pub fn set_can_join_raid(e: &mut dyn Entity, can_join_raid: bool) {
    data_mut(e).can_join_raid = can_join_raid;
}

/// `getCurrentRaid()` (the raid id).
pub fn get_current_raid(e: &dyn Entity) -> Option<i32> {
    data(e).raid
}

/// `setCurrentRaid(raid)`.
pub fn set_current_raid(e: &mut dyn Entity, raid: Option<i32>) {
    data_mut(e).raid = raid;
}

pub fn set_wave(e: &mut dyn Entity, wave: i32) {
    data_mut(e).wave = wave;
}

pub fn get_wave(e: &dyn Entity) -> i32 {
    data(e).wave
}

pub fn ticks_outside_raid(e: &dyn Entity) -> i32 {
    data(e).ticks_outside_raid
}

pub fn set_ticks_outside_raid(e: &mut dyn Entity, ticks: i32) {
    data_mut(e).ticks_outside_raid = ticks;
}

pub fn is_celebrating(e: &dyn Entity) -> bool {
    e.base().entity_data.get(IS_CELEBRATING)
}

pub fn set_celebrating(e: &mut dyn Entity, celebrating: bool) {
    e.base_mut().entity_data.set(IS_CELEBRATING, celebrating);
}

/// `isCaptain()`.
pub fn is_captain(e: &dyn Entity, registries: &crate::registry::RegistryAccess) -> bool {
    let banner = e.get_item_by_slot(EquipmentSlot::Head);
    let has_captain_banner = !banner.is_empty() && ItemStack::matches(&banner, &Raid::get_ominous_banner_instance(registries));
    has_captain_banner && is_patrol_leader(e)
}

/// `hasRaid()`.
pub fn has_raid(e: &dyn Entity, level: &ServerLevel) -> bool {
    get_current_raid(e).is_some() || level.get_raid_at(e.block_position()).is_some()
}

/// `hasActiveRaid()`.
pub fn has_active_raid(e: &dyn Entity, level: &ServerLevel) -> bool {
    get_current_raid(e).and_then(|id| level.get_raid(id)).is_some_and(|r| r.is_active())
}

/// `RaiderPredicate`: `(hasRaid(), isCaptain())` of a raider, `None` for other entities.
pub fn raider_state(e: &dyn Entity, level: &ServerLevel) -> Option<(bool, bool)> {
    e.raider()?;
    Some((has_raid(e, level), is_captain(e, &level.registries)))
}

// ---- behavior ------------------------------------------------------------------------------------------------------

/// `Raider.aiStep()`.
pub fn ai_step(e: &mut dyn Entity, level: &mut ServerLevel) {
    if e.is_alive() {
        resolve_pending_raid(e, level);
        let current_raid = get_current_raid(e);
        if can_join_raid(e) {
            if current_raid.is_none() {
                if level.game_time() % 20 == 0 {
                    let nearby = level.get_raids().and_then(|r| r.get_nearby_raid(e.block_position(), super::raid::VALID_RAID_RADIUS_SQR));
                    if let Some(raid_id) = nearby
                        && raids::can_join_raid(e)
                    {
                        level.with_raid(raid_id, |level, raid| {
                            let groups = raid.get_groups_spawned();
                            raid.join_raid_existing(level, groups, e);
                        });
                    }
                }
            } else {
                let target = mob::tick::get_target_checked(e, level);
                if target.and_then(|t| level.get_entity(t)).is_some_and(|t| t.get_type() == EntityType::PLAYER || t.get_type() == EntityType::IRON_GOLEM) {
                    e.living_mut().expect("living").no_action_time = 0;
                }
            }
        }
    }
    mob::tick::ai_step(e, level);
}

/// `Raider.updateNoActionTime()`.
pub fn update_no_action_time(e: &mut dyn Entity) {
    let l = e.living_mut().expect("living");
    l.no_action_time = l.no_action_time.wrapping_add(2);
}

/// `Raider.die(source)`.
pub fn die(e: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    if let Some(raid_id) = get_current_raid(e) {
        let wave = get_wave(e);
        let leader = is_patrol_leader(e);
        let killer_player = source.causing_info().filter(|k| k.is_player).map(|k| k.uuid);
        let token = RaiderToken { id: e.id(), wave, health: e.get_health() };
        level.with_raid(raid_id, |level, raid| {
            if leader {
                raid.remove_leader(wave);
            }
            if let Some(uuid) = killer_player {
                raid.add_hero_of_the_village(uuid);
            }
            raid.remove_from_raid(level, token, false, Some(e));
        });
    }
    crate::world::entity::living_entity::damage::die(e, level, source);
}

/// `Raider.hurtServer(level, source, damage)`.
pub fn hurt_server(e: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
    if has_active_raid(e, level) {
        let raid_id = get_current_raid(e).expect("active raid");
        level.with_raid(raid_id, |level, raid| raid.update_bossbar(level, Some(&*e)));
    }
    crate::world::entity::living_entity::damage::hurt_server(e, level, source, damage)
}

/// `Raider.removeWhenFarAway(distSqr)`.
pub fn remove_when_far_away(e: &dyn Entity, dist_sqr: f64) -> bool {
    if get_current_raid(e).is_none() { patrolling_monster::remove_when_far_away(e, dist_sqr) } else { false }
}

/// `Raider.requiresCustomPersistence()`.
pub fn requires_custom_persistence(e: &dyn Entity) -> bool {
    (e.base().is_passenger() || mob::leashable::is_leashed(e)) || get_current_raid(e).is_some()
}

/// `Raider.finalizeSpawn(level, difficulty, reason, groupData)`.
pub fn finalize_spawn(e: &mut dyn Entity, level: &mut dyn ServerLevelAccessor, difficulty: DifficultyInstance, reason: EntitySpawnReason, group_data: Option<Box<dyn SpawnGroupData>>) -> Option<Box<dyn SpawnGroupData>> {
    let can_join = e.get_type() != EntityType::WITCH || reason != EntitySpawnReason::Natural;
    set_can_join_raid(e, can_join);
    patrolling_monster::finalize_spawn(e, level, difficulty, reason, group_data)
}

/// `Raider.addAdditionalSaveData(output)` (including `Monster`'s and `PatrollingMonster`'s).
pub fn add_additional_save_data(e: &dyn Entity, out: &mut CompoundTag) {
    mob::nbt::add_additional_save_data(e, out);
    patrolling_monster::add_additional_save_data(e, out);
    let d = data(e);
    out.put_int("Wave", d.wave);
    out.put_boolean("CanJoinRaid", d.can_join_raid);
    if let Some(id) = d.raid {
        out.put_int("RaidId", id);
    }
}

/// `Raider.readAdditionalSaveData(input)`: the raid is joined by [`resolve_pending_raid`] once the level is at hand.
pub fn read_additional_save_data(e: &mut dyn Entity, input: &CompoundTag) {
    mob::nbt::read_additional_save_data(e, input);
    patrolling_monster::read_additional_save_data(e, input);
    let d = data_mut(e);
    d.wave = input.get_int_or("Wave", 0);
    d.can_join_raid = input.get_boolean_or("CanJoinRaid", false);
    d.pending_raid_id = input.get_int("RaidId");
}

/// The level-dependent half of `readAdditionalSaveData`: `raid = level.getRaids().get(raidId); if (raid != null) {
/// raid.addWaveMob(level, wave, this, false); if (isPatrolLeader()) raid.setLeader(wave, this); }`.
pub fn resolve_pending_raid(e: &mut dyn Entity, level: &mut ServerLevel) {
    let Some(raid_id) = data_mut(e).pending_raid_id.take() else { return };
    if !level.get_raids_mut().contains(raid_id) {
        return;
    }
    let wave = data(e).wave;
    let leader = is_patrol_leader(e);
    data_mut(e).raid = Some(raid_id);
    level.with_raid(raid_id, |level, raid| {
        raid.add_wave_mob(level, wave, &*e, false);
        if leader {
            raid.set_leader(level, wave, e);
        }
    });
}

/// `Raider.pickUpItem(level, entity)`.
pub fn pick_up_item(e: &mut dyn Entity, level: &mut ServerLevel, item_id: EntityId) {
    let Some(item_stack) = level.get_entity(item_id).and_then(|x| x.as_any().downcast_ref::<ItemEntity>()).map(|x| x.get_item()) else { return };
    let raid_id = get_current_raid(e);
    let has_active = has_active_raid(e, level);
    let wave = get_wave(e);
    let has_raid_leader = has_active && raid_id.and_then(|id| level.get_raid(id)).is_some_and(|r| r.get_leader(wave).is_some());
    if has_active && !has_raid_leader && ItemStack::matches(&item_stack, &Raid::get_ominous_banner_instance(&level.registries)) {
        let slot = EquipmentSlot::Head;
        let current = e.get_item_by_slot(slot);
        let drop_chance = e.mob().expect("mob").drop_chances.by_equipment(slot);
        if !current.is_empty() && (e.base_mut().random.next_float() - 0.1).max(0.0) < drop_chance {
            crate::world::entity::item::spawn_at_location(level, e, current, Vec3::ZERO);
        }
        // `onItemPickup(entity)` is empty for mobs; `take(entity, count)`
        if !level.get_entity(item_id).is_some_and(|x| x.is_removed()) {
            let packet = ClientboundTakeItemEntityPacket { item_id, player_id: e.id(), amount: item_stack.count() };
            level.entity_tracker.send_to_tracking_players(item_id, &crate::server::level::server_entity::encode_payload(&packet));
        }
        mob::equipment::set_item_slot(e, level, slot, item_stack.clone());
        level.with_entity(item_id, |_, item| item.discard());
        level.with_raid(raid_id.expect("active raid"), |level, raid| raid.set_leader(level, wave, e));
        set_patrol_leader(e, true);
    } else {
        mob::equipment::pick_up_item(e, level, item_id);
    }
}

/// `Raider.applyRaidBuffs` is `Entity::apply_raid_buffs`; `getCelebrateSound()` is `Entity::get_celebrate_sound`.
///
/// `Raider.registerGoals()` (after `PatrollingMonster.registerGoals()`): the goals every raider has, in vanilla's order.
pub fn register_goals(mob: &mut MobData) {
    mob.goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
    mob.goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal::new()));
    mob.goal_selector.add_goal(3, Box::new(crate::world::entity::ai::goal::PathfindToRaidGoal::new()));
    mob.goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05, 1)));
    mob.goal_selector.add_goal(5, Box::new(RaiderCelebration::new()));
}

fn banner_matches(registries: &crate::registry::RegistryAccess, stack: &ItemStack) -> bool {
    ItemStack::matches(stack, &Raid::get_ominous_banner_instance(registries))
}

/// `Raider.HoldGroundAttackGoal`.
pub struct HoldGroundAttackGoal {
    hostile_radius_sqr: f32,
    shout_targeting: TargetingConditions,
}

impl HoldGroundAttackGoal {
    pub fn new(hostile_radius: f32) -> Self {
        HoldGroundAttackGoal {
            hostile_radius_sqr: hostile_radius * hostile_radius,
            shout_targeting: TargetingConditions::for_non_combat().range(8.0).ignore_line_of_sight().ignore_invisibility_testing(),
        }
    }

    fn nearby_raiders(&self, mob: &dyn Entity, level: &ServerLevel) -> Vec<EntityId> {
        let bb = mob.get_bounding_box().inflate(8.0, 8.0, 8.0);
        mob::ext::get_nearby_entities(level, |e| e.raider().is_some(), &self.shout_targeting, mob, &bb)
    }
}

impl Goal for HoldGroundAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let last_hurt_by_mob_is_player = mob.living().and_then(|l| l.last_hurt_by_mob).and_then(|u| level.get_entity_by_uuid(u)).is_some_and(|e| e.get_type() == EntityType::PLAYER);
        get_current_raid(mob).is_none() && is_patrolling(mob) && mob::tick::get_target_checked(mob, level).is_some() && !mob.mob().expect("mob").is_aggressive() && !last_hurt_by_mob_is_player
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
        let target = mob::tick::get_target_checked(mob, level);
        for other in self.nearby_raiders(mob, level) {
            level.with_entity(other, |level, e| mob::tick::set_target(e, level, target));
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target = mob::tick::get_target_checked(mob, level);
        if target.is_some() {
            for other in self.nearby_raiders(mob, level) {
                level.with_entity(other, |level, e| {
                    mob::tick::set_target(e, level, target);
                    e.mob_mut().expect("mob").set_aggressive(true);
                });
            }
            mob.mob_mut().expect("mob").set_aggressive(true);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = mob::tick::get_target_checked(mob, level) {
            let dist_sqr = level.get_entity(target).map(|t| mob.base().distance_to_sqr_vec(t.position()));
            if let Some(dist_sqr) = dist_sqr
                && dist_sqr > f64::from(self.hostile_radius_sqr)
            {
                if let Some(t) = level.get_entity(target) {
                    mob::look_at_entity_full(mob, t, 30.0, 30.0);
                }
                if mob.base_mut().random.next_int_bounded(50) == 0 {
                    mob.play_ambient_sound(level);
                }
            } else {
                mob.mob_mut().expect("mob").set_aggressive(true);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `Raider.ObtainRaidLeaderBannerGoal`.
pub struct ObtainRaidLeaderBannerGoal {
    unreachable_banner_cache: HashMap<EntityId, i64>,
    path_to_banner: Option<Path>,
    pursued_banner_item_entity: Option<EntityId>,
}

impl ObtainRaidLeaderBannerGoal {
    pub fn new() -> Self {
        ObtainRaidLeaderBannerGoal { unreachable_banner_cache: HashMap::new(), path_to_banner: None, pursued_banner_item_entity: None }
    }

    fn cannot_pick_up_banner(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        if !has_active_raid(mob, level) {
            return true;
        }
        let raid = get_current_raid(mob).and_then(|id| level.get_raid(id)).expect("active raid");
        if raid.is_over() {
            true
        } else if !mob.can_be_leader() {
            true
        } else if banner_matches(&level.registries, &mob.get_item_by_slot(EquipmentSlot::Head)) {
            true
        } else {
            let leader = raid.get_leader(get_wave(mob)).and_then(|id| level.get_entity(id));
            leader.is_some_and(|l| l.is_alive())
        }
    }
}

impl Default for ObtainRaidLeaderBannerGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for ObtainRaidLeaderBannerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.cannot_pick_up_banner(mob, level) {
            return false;
        }
        let mut temp_cache: HashMap<EntityId, i64> = HashMap::new();
        let follow_range = mob::attribute_value(mob, Attribute::FOLLOW_RANGE);
        let bb = mob.get_bounding_box().inflate(follow_range, 8.0, follow_range);
        let banner = Raid::get_ominous_banner_instance(&level.registries);
        let banners = level.get_entities_of_class(
            &bb,
            |e| e.get_type() == EntityType::ITEM,
            |e| e.as_any().downcast_ref::<ItemEntity>().is_some_and(|i| !i.has_pick_up_delay() && e.is_alive() && ItemStack::matches(&i.get_item(), &banner)),
        );
        for banner_id in banners {
            let unreachable_until_time = self.unreachable_banner_cache.get(&banner_id).copied().unwrap_or(i64::MIN);
            if level.game_time() < unreachable_until_time {
                temp_cache.insert(banner_id, unreachable_until_time);
            } else {
                let target_pos = level.get_entity(banner_id).map(|b| b.block_position());
                let path = target_pos.and_then(|p| navigation::create_path_entity(mob, level, p, 1));
                if let Some(path) = path.filter(|p| p.can_reach()) {
                    self.path_to_banner = Some(path);
                    self.pursued_banner_item_entity = Some(banner_id);
                    return true;
                }
                temp_cache.insert(banner_id, level.game_time() + 600);
            }
        }
        self.unreachable_banner_cache = temp_cache;
        false
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(pursued) = self.pursued_banner_item_entity else { return false };
        if self.path_to_banner.as_ref().is_none_or(|p| p.is_done()) {
            false
        } else if level.get_entity(pursued).is_none_or(|b| b.is_removed()) {
            false
        } else {
            !self.cannot_pick_up_banner(mob, level)
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to_path(mob, level, self.path_to_banner.clone(), 1.15);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.path_to_banner = None;
        self.pursued_banner_item_entity = None;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(pursued) = self.pursued_banner_item_entity
            && level.get_entity(pursued).is_some_and(|b| b.base().distance_to_sqr_vec(mob.position()) < 1.414 * 1.414)
        {
            mob.pick_up_item(level, pursued);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Raider.RaiderCelebration`.
pub struct RaiderCelebration;

impl RaiderCelebration {
    pub fn new() -> Self {
        RaiderCelebration
    }
}

impl Default for RaiderCelebration {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for RaiderCelebration {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let current_raid = get_current_raid(mob).and_then(|id| level.get_raid(id));
        mob.is_alive() && mob::tick::get_target_checked(mob, level).is_none() && current_raid.is_some_and(|r| r.is_loss())
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_celebrating(mob, true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        set_celebrating(mob, false);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let delay100 = self.adjusted_tick_delay(100);
        if !mob.base().is_silent() && mob.base_mut().random.next_int_bounded(delay100) == 0 {
            if let Some(sound) = mob.get_celebrate_sound() {
                crate::world::entity::living_entity::damage::make_sound(mob, level, sound);
            }
        }
        let delay50 = self.adjusted_tick_delay(50);
        if !mob.base().is_passenger() && mob.base_mut().random.next_int_bounded(delay50) == 0 {
            mob::jump_control_jump(mob);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Raider.RaiderMoveThroughVillageGoal`.
pub struct RaiderMoveThroughVillageGoal {
    speed_modifier: f64,
    poi_pos: Option<BlockPos>,
    visited: Vec<BlockPos>,
    distance_to_poi: i32,
    stuck: bool,
}

impl RaiderMoveThroughVillageGoal {
    pub fn new(speed_modifier: f64, distance_to_poi: i32) -> Self {
        RaiderMoveThroughVillageGoal { speed_modifier, poi_pos: None, visited: Vec::new(), distance_to_poi, stuck: false }
    }

    fn is_valid_raid(mob: &dyn Entity, level: &ServerLevel) -> bool {
        has_active_raid(mob, level) && !get_current_raid(mob).and_then(|id| level.get_raid(id)).is_some_and(|r| r.is_over())
    }

    fn has_suitable_poi(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let pos = mob.block_position();
        let visited = self.visited.clone();
        let has_not_visited = move |p: BlockPos| !visited.contains(&p);
        let home_pos = super::raid_world::random_home_poi(level, &has_not_visited, pos, 48, &mut mob.base_mut().random);
        match home_pos {
            None => false,
            Some(p) => {
                self.poi_pos = Some(p);
                true
            }
        }
    }

    fn update_visited(&mut self) {
        if self.visited.len() > 2 {
            self.visited.remove(0);
        }
    }
}

impl Goal for RaiderMoveThroughVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.update_visited();
        Self::is_valid_raid(mob, level) && self.has_suitable_poi(mob, level) && mob::tick::get_target_checked(mob, level).is_none()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if navigation::is_done(mob, level) {
            false
        } else {
            let poi = self.poi_pos.expect("started with a poi");
            mob::tick::get_target_checked(mob, level).is_none() && !poi.closer_to_center_than(&mob.position(), f64::from(mob.base().get_bb_width()) + f64::from(self.distance_to_poi)) && !self.stuck
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let poi = self.poi_pos.expect("started with a poi");
        if poi.closer_to_center_than(&mob.position(), f64::from(self.distance_to_poi)) {
            self.visited.push(poi);
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.living_mut().expect("living").no_action_time = 0;
        let poi = self.poi_pos.expect("started with a poi");
        navigation::move_to(mob, level, f64::from(poi.x()), f64::from(poi.y()), f64::from(poi.z()), self.speed_modifier);
        self.stuck = false;
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if navigation::is_done(mob, level) {
            let poi = self.poi_pos.expect("started with a poi");
            let poi_vec = Vec3::at_bottom_center_of(&poi);
            let mut next_pos = default_random_pos::get_pos_towards(mob, level, 16, 7, poi_vec, f64::from(std::f32::consts::PI / 10.0));
            if next_pos.is_none() {
                next_pos = default_random_pos::get_pos_towards(mob, level, 8, 7, poi_vec, f64::from(std::f32::consts::PI / 2.0));
            }
            let Some(next_pos) = next_pos else {
                self.stuck = true;
                return;
            };
            navigation::move_to(mob, level, next_pos.x, next_pos.y, next_pos.z, self.speed_modifier);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

#[allow(dead_code)]
fn _unused(_: SoundEvent) {}
