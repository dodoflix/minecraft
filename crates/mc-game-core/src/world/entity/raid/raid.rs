//! `net.minecraft.world.entity.raid.Raid`: one raid of a village (state machine, wave composition, boss bar, saved data).
//!
//! Vanilla's raid holds `Raider` object references; here a raid holds `(entity id, uuid)` pairs ([`RaiderRef`]) and reads
//! the raiders through the level. A raid is *checked out* of [`Raids`](super::raids::Raids) while it ticks or is mutated
//! together with the level (`ServerLevel::with_raid`); a raider that is itself taken out of the level (it is ticking) is
//! passed in explicitly where vanilla would read it through the object reference.

use super::raider;
use crate::server::bossevents::{BossBarColor, BossBarOverlay};
use crate::server::level::ServerLevel;
use super::raid_boss_bar::RaidBossBar as ServerBossEvent;
use super::raid_world;
use crate::server::players::PlayerHandle;
use crate::stats::stats::Stats;
use crate::util::codec::{self, DataResult, DecodeResult};
use crate::util::java_hash_order::JavaHashCapacity;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::spawn_placements;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::ItemStack;
use crate::world::level::server_level_accessor::ServerLevelAccessor as _;
use crate::world::level::{BlockGetter as _, LevelReader as _};
use crate::world::level::block::BlockStateExt as _;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::{self, LegacyRandomSource, RandomSource};
use mc_core::uuid_util::{java_hash_code, uuid_from_int_array, uuid_to_int_array};
use mc_data::block::Block;
use mc_data::enums::EntitySpawnReason;
use mc_data::{DataComponentType, EntityType, Item, SoundEvent};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::components::{BANNER_PATTERNS, BannerPatternLayer, BannerPatternLayers, DataComponentPatch, DyeColor, Holder, ITEM_NAME, RARITY, Rarity, TOOLTIP_DISPLAY, TooltipDisplay};
use mc_protocol::packets::play::{ClientboundSoundPacket, SoundEventHolder, SoundSource};
use mc_text::Component;
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::Difficulty;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const VALID_RAID_RADIUS_SQR: i32 = 9216;
pub const RAID_REMOVAL_THRESHOLD_SQR: i32 = 12544;
pub const MAX_NO_ACTION_TIME: i32 = 2400;
pub const MAX_CELEBRATION_TICKS: i32 = 600;
pub const VILLAGE_RADIUS_BUFFER: i32 = 16;
pub const DEFAULT_MAX_RAID_OMEN_LEVEL: i32 = 5;
const RAID_TIMEOUT_TICKS: i64 = 48000;
const POST_RAID_TICK_LIMIT: i32 = 40;
const DEFAULT_PRE_RAID_TICKS: i32 = 300;
const OUTSIDE_RAID_BOUNDS_TIMEOUT: i32 = 30;
const HERO_OF_THE_VILLAGE_DURATION: i32 = 48000;
const RAIDERS_REMAINING: &str = "event.minecraft.raid.raiders_remaining";

fn raid_name() -> Component {
    Component::translatable("event.minecraft.raid", vec![])
}

fn raid_bar_victory() -> Component {
    Component::translatable("event.minecraft.raid.victory.full", vec![])
}

fn raid_bar_defeat() -> Component {
    Component::translatable("event.minecraft.raid.defeat.full", vec![])
}

/// `Raid.RaidStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RaidStatus {
    Ongoing,
    Victory,
    Loss,
    Stopped,
}

impl RaidStatus {
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            RaidStatus::Ongoing => "ongoing",
            RaidStatus::Victory => "victory",
            RaidStatus::Loss => "loss",
            RaidStatus::Stopped => "stopped",
        }
    }

    fn by_name(name: &str) -> Option<RaidStatus> {
        [RaidStatus::Ongoing, RaidStatus::Victory, RaidStatus::Loss, RaidStatus::Stopped].into_iter().find(|s| s.get_serialized_name() == name)
    }
}

/// `Raid.RaiderType` (the enum order is the spawn order of a wave).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RaiderType {
    Vindicator,
    Evoker,
    Pillager,
    Witch,
    Ravager,
}

impl RaiderType {
    const VALUES: [RaiderType; 5] = [RaiderType::Vindicator, RaiderType::Evoker, RaiderType::Pillager, RaiderType::Witch, RaiderType::Ravager];

    fn entity_type(self) -> EntityType {
        match self {
            RaiderType::Vindicator => EntityType::VINDICATOR,
            RaiderType::Evoker => EntityType::EVOKER,
            RaiderType::Pillager => EntityType::PILLAGER,
            RaiderType::Witch => EntityType::WITCH,
            RaiderType::Ravager => EntityType::RAVAGER,
        }
    }

    /// `spawnsPerWaveBeforeBonus`.
    fn spawns_per_wave_before_bonus(self) -> [i32; 8] {
        match self {
            RaiderType::Vindicator => [0, 0, 2, 0, 1, 4, 2, 5],
            RaiderType::Evoker => [0, 0, 0, 0, 0, 1, 1, 2],
            RaiderType::Pillager => [0, 4, 3, 3, 4, 4, 4, 2],
            RaiderType::Witch => [0, 0, 0, 0, 3, 0, 0, 1],
            RaiderType::Ravager => [0, 0, 0, 1, 0, 1, 0, 2],
        }
    }
}

/// A raider of the raid: what the raid keeps of vanilla's `Raider` reference. `health` is the last health read from the
/// live entity (used while the raider is unloaded).
#[derive(Clone, Copy, Debug)]
pub struct RaiderRef {
    pub id: EntityId,
    pub uuid: Uuid,
    pub health: f32,
}

/// `HashSet<Raider>`: members in insertion order plus the table capacity that decides the iteration order
/// (`Entity.hashCode()` is the entity id).
#[derive(Clone, Default, Debug)]
struct RaiderSet {
    members: Vec<RaiderRef>,
    capacity: JavaHashCapacity,
}

impl RaiderSet {
    fn len(&self) -> usize {
        self.members.len()
    }

    fn contains_id(&self, id: EntityId) -> bool {
        self.members.iter().any(|r| r.id == id)
    }

    fn add(&mut self, r: RaiderRef) -> bool {
        if self.contains_id(r.id) {
            return false;
        }
        self.members.push(r);
        self.capacity.grow_to_fit(self.members.len());
        true
    }

    fn remove(&mut self, id: EntityId) -> bool {
        match self.members.iter().position(|r| r.id == id) {
            Some(i) => {
                self.members.remove(i);
                true
            }
            None => false,
        }
    }

    /// Iteration order of the Java set.
    fn ordered(&self) -> Vec<RaiderRef> {
        self.capacity.iteration_order(self.members.iter().copied(), |r| r.id)
    }
}

/// What other raids look like to the raid that ticks (`Raids.getNearbyRaid` reads every raid's center and activity).
#[derive(Clone, Copy, Debug)]
pub struct RaidMeta {
    pub id: i32,
    pub center: BlockPos,
    pub active: bool,
}

/// `Raid`.
pub struct Raid {
    /// The key in `Raids.raidMap` (`Raids.getId(raid)`); 0 until the raid is registered.
    id: i32,
    group_to_leader_map: BTreeMap<i32, RaiderRef>,
    group_raider_map: BTreeMap<i32, RaiderSet>,
    heroes_of_the_village: Vec<Uuid>,
    heroes_capacity: JavaHashCapacity,
    ticks_active: i64,
    center: BlockPos,
    started: bool,
    total_health: f32,
    raid_omen_level: i32,
    active: bool,
    groups_spawned: i32,
    post_raid_ticks: i32,
    raid_cooldown_ticks: i32,
    random: LegacyRandomSource,
    raid_event: ServerBossEvent,
    num_groups: i32,
    status: RaidStatus,
    celebration_ticks: i32,
    wave_spawn_pos: Option<BlockPos>,
}

impl Raid {
    /// `new Raid(center, difficulty)`.
    pub fn new(center: BlockPos, difficulty: Difficulty) -> Raid {
        Self::with_random(center, difficulty, random::create_new())
    }

    /// `new Raid(center, difficulty)` with the raid's `random` given (the field is `RandomSource.create()` in vanilla; the
    /// boss bar id is drawn from it first).
    pub fn with_random(center: BlockPos, difficulty: Difficulty, mut random: LegacyRandomSource) -> Raid {
        let event_id = mth::create_insecure_uuid(&mut random);
        let mut raid = Raid::blank(center, event_id, random);
        raid.active = true;
        raid.raid_cooldown_ticks = DEFAULT_PRE_RAID_TICKS;
        raid.raid_event.set_progress(0.0);
        raid.num_groups = Raid::get_num_groups_for(difficulty);
        raid.status = RaidStatus::Ongoing;
        raid
    }

    fn blank(center: BlockPos, event_id: Uuid, random: LegacyRandomSource) -> Raid {
        Raid {
            id: 0,
            group_to_leader_map: BTreeMap::new(),
            group_raider_map: BTreeMap::new(),
            heroes_of_the_village: Vec::new(),
            heroes_capacity: JavaHashCapacity::default(),
            ticks_active: 0,
            center,
            started: false,
            total_health: 0.0,
            raid_omen_level: 0,
            active: false,
            groups_spawned: 0,
            post_raid_ticks: 0,
            raid_cooldown_ticks: 0,
            random,
            raid_event: ServerBossEvent::new(event_id, raid_name(), BossBarColor::Red, BossBarOverlay::Notched10),
            num_groups: 0,
            status: RaidStatus::Ongoing,
            celebration_ticks: 0,
            wave_spawn_pos: None,
        }
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub(super) fn set_id(&mut self, id: i32) {
        self.id = id;
    }

    pub fn is_over(&self) -> bool {
        self.is_victory() || self.is_loss()
    }

    pub fn is_between_waves(&self) -> bool {
        self.has_first_wave_spawned() && self.get_total_raiders_alive() == 0 && self.raid_cooldown_ticks > 0
    }

    pub fn has_first_wave_spawned(&self) -> bool {
        self.groups_spawned > 0
    }

    pub fn is_stopped(&self) -> bool {
        self.status == RaidStatus::Stopped
    }

    pub fn is_victory(&self) -> bool {
        self.status == RaidStatus::Victory
    }

    pub fn is_loss(&self) -> bool {
        self.status == RaidStatus::Loss
    }

    pub fn get_total_health(&self) -> f32 {
        self.total_health
    }

    /// `getAllRaiders()` (ids).
    pub fn get_all_raiders(&self) -> Vec<EntityId> {
        self.group_raider_map.values().flat_map(|s| s.ordered()).map(|r| r.id).collect()
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn get_groups_spawned(&self) -> i32 {
        self.groups_spawned
    }

    pub fn get_boss_event(&self) -> &ServerBossEvent {
        &self.raid_event
    }

    pub fn get_max_raid_omen_level(&self) -> i32 {
        5
    }

    pub fn get_raid_omen_level(&self) -> i32 {
        self.raid_omen_level
    }

    pub fn set_raid_omen_level(&mut self, raid_omen_level: i32) {
        self.raid_omen_level = raid_omen_level;
    }

    pub fn get_center(&self) -> BlockPos {
        self.center
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn meta(&self) -> RaidMeta {
        RaidMeta { id: self.id, center: self.center, active: self.active }
    }

    /// `validPlayer()`: alive and `player.level().getRaidAt(pos) == this` (`getNearbyRaid(pos, 9216)` over `view`, which
    /// holds every raid of the level in map order; this raid's own entry is read live).
    fn valid_player(&self, view: &[RaidMeta], player: &dyn Entity) -> bool {
        let pos = player.block_position();
        player.is_alive() && super::raids::nearby_raid_id(view.iter().map(|m| if m.id == self.id { self.meta() } else { *m }), pos, VALID_RAID_RADIUS_SQR) == Some(self.id)
    }

    /// `updatePlayers(level)`.
    fn update_players(&mut self, level: &ServerLevel, view: &[RaidMeta]) {
        let current: Vec<PlayerHandle> = self.raid_event.get_players().to_vec();
        let mut new_players: Vec<PlayerHandle> = Vec::new();
        for &pid in level.players() {
            let Some(player) = level.get_entity(pid) else { continue };
            if self.valid_player(view, player)
                && let Some(handle) = player.as_any().downcast_ref::<ServerPlayer>().and_then(|p| p.handle.clone())
            {
                new_players.push(handle);
            }
        }
        for player in &new_players {
            if !current.iter().any(|c| c.same_player(player)) {
                self.raid_event.add_player(player);
            }
        }
        for player in &current {
            if !new_players.iter().any(|n| n.same_player(player)) {
                self.raid_event.remove_player(player);
            }
        }
    }

    /// `absorbRaidOmen(player)`.
    pub fn absorb_raid_omen(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) -> bool {
        let Some(amplifier) = player.get_effect(mc_data::MobEffect::RAID_OMEN).map(|e| e.amplifier) else {
            return false;
        };
        self.raid_omen_level = self.raid_omen_level.wrapping_add(amplifier).wrapping_add(1);
        self.raid_omen_level = mth::clamp_i32(self.raid_omen_level, 0, self.get_max_raid_omen_level());
        if !self.has_first_wave_spawned() {
            player.award_stat_once(level, Stats::RAID_TRIGGER);
            // CriteriaTriggers::RAID_OMEN.trigger(level, player);
        }
        true
    }

    /// `stop()`.
    pub fn stop(&mut self) {
        self.active = false;
        self.raid_event.remove_all_players();
        self.status = RaidStatus::Stopped;
    }

    /// `tick(level)`; `view` = every raid of the level in `Raids` map order (see [`RaidMeta`]).
    pub fn tick(&mut self, level: &mut ServerLevel, view: &[RaidMeta]) {
        if self.is_stopped() {
            return;
        }
        if self.status == RaidStatus::Ongoing {
            let old_active = self.active;
            self.active = level.has_chunk(self.center.x() >> 4, self.center.z() >> 4);
            if level.difficulty() == Difficulty::Peaceful {
                self.stop();
                return;
            }
            if old_active != self.active {
                self.raid_event.set_visible(self.active);
            }
            if !self.active {
                return;
            }
            if !raid_world::is_village(level, self.center) {
                self.move_raid_center_to_nearby_village_section(level);
            }
            if !raid_world::is_village(level, self.center) {
                if self.groups_spawned > 0 {
                    self.status = RaidStatus::Loss;
                } else {
                    self.stop();
                }
            }
            self.ticks_active += 1;
            if self.ticks_active >= RAID_TIMEOUT_TICKS {
                self.stop();
                return;
            }
            let raiders_alive = self.get_total_raiders_alive();
            if raiders_alive == 0 && self.has_more_waves() {
                if self.raid_cooldown_ticks <= 0 {
                    if self.raid_cooldown_ticks == 0 && self.groups_spawned > 0 {
                        self.raid_cooldown_ticks = DEFAULT_PRE_RAID_TICKS;
                        self.raid_event.set_name(raid_name());
                        return;
                    }
                } else {
                    let has_cached_wave_spawn_pos = self.wave_spawn_pos.is_some();
                    let mut should_try_to_find_spawn_pos = !has_cached_wave_spawn_pos && self.raid_cooldown_ticks % 5 == 0;
                    if let Some(pos) = self.wave_spawn_pos
                        && !crate::world::entity::living_entity::util::is_position_entity_ticking(level, pos)
                    {
                        should_try_to_find_spawn_pos = true;
                    }
                    if should_try_to_find_spawn_pos {
                        self.wave_spawn_pos = self.get_valid_spawn_pos(level);
                    }
                    if self.raid_cooldown_ticks == DEFAULT_PRE_RAID_TICKS || self.raid_cooldown_ticks % 20 == 0 {
                        self.update_players(level, view);
                    }
                    self.raid_cooldown_ticks -= 1;
                    self.raid_event.set_progress(mth::clamp_f32((DEFAULT_PRE_RAID_TICKS - self.raid_cooldown_ticks) as f32 / 300.0, 0.0, 1.0));
                }
            }
            if self.ticks_active % 20 == 0 {
                self.update_players(level, view);
                self.update_raiders(level);
                if raiders_alive > 0 {
                    if raiders_alive <= 2 {
                        self.raid_event.set_name(raid_name().append_str(" - ").append(Component::translatable(RAIDERS_REMAINING, vec![raiders_alive.into()])));
                    } else {
                        self.raid_event.set_name(raid_name());
                    }
                } else {
                    self.raid_event.set_name(raid_name());
                }
            }
            let mut sound_played = false;
            let mut attempt = 0;
            while self.should_spawn_group() {
                let spawn_pos = match self.wave_spawn_pos {
                    Some(p) => Some(p),
                    None => self.find_random_spawn_pos(level, 20),
                };
                if let Some(spawn_pos) = spawn_pos {
                    self.started = true;
                    self.spawn_group(level, spawn_pos);
                    if !sound_played {
                        self.play_sound(level, spawn_pos);
                        sound_played = true;
                    }
                } else {
                    attempt += 1;
                }
                if attempt > 5 {
                    self.stop();
                    break;
                }
            }
            if self.is_started() && !self.has_more_waves() && raiders_alive == 0 {
                if self.post_raid_ticks < POST_RAID_TICK_LIMIT {
                    self.post_raid_ticks += 1;
                } else {
                    self.status = RaidStatus::Victory;
                    let heroes = self.ordered_heroes();
                    for hero_uuid in heroes {
                        let Some(hero_id) = level.get_entity_by_uuid(hero_uuid).filter(|e| e.living().is_some() && !e.is_spectator()).map(|e| e.id()) else {
                            continue;
                        };
                        let omen_level = self.raid_omen_level;
                        level.with_entity(hero_id, |level, hero| {
                            hero.add_effect(level, MobEffectInstance::new(mc_data::MobEffect::HERO_OF_THE_VILLAGE, HERO_OF_THE_VILLAGE_DURATION, omen_level.wrapping_sub(1), false, false, true, None), None);
                            if let Some(player) = hero.as_any_mut().downcast_mut::<ServerPlayer>() {
                                player.award_stat_once(level, Stats::RAID_WIN);
                                // CriteriaTriggers::RAID_WIN.trigger(level, player);
                            }
                        });
                    }
                }
            }
            self.set_dirty(level);
        } else if self.is_over() {
            self.celebration_ticks += 1;
            if self.celebration_ticks >= MAX_CELEBRATION_TICKS {
                self.stop();
                return;
            }
            if self.celebration_ticks % 20 == 0 {
                self.update_players(level, view);
                self.raid_event.set_visible(true);
                if self.is_victory() {
                    self.raid_event.set_progress(0.0);
                    self.raid_event.set_name(raid_bar_victory());
                } else {
                    self.raid_event.set_name(raid_bar_defeat());
                }
            }
        }
    }

    /// `moveRaidCenterToNearbyVillageSection(level)`: the village section of the 5x5x5 cube around the center section
    /// whose center is closest to the raid center (the first one wins a tie, in `SectionPos.cube` order).
    fn move_raid_center_to_nearby_village_section(&mut self, level: &ServerLevel) {
        let origin = mc_core::pos::SectionPos::of_block_pos(&self.center);
        let mut best: Option<(f64, BlockPos)> = None;
        for section in mc_core::pos::SectionPos::cube(&origin, 2) {
            if !raid_world::is_village_section(level, section) {
                continue;
            }
            let center = section.center();
            let dist = center.dist_sqr(&self.center);
            if best.is_none_or(|(d, _)| dist < d) {
                best = Some((dist, center));
            }
        }
        if let Some((_, center)) = best {
            self.center = center;
        }
    }

    fn get_valid_spawn_pos(&mut self, level: &ServerLevel) -> Option<BlockPos> {
        self.find_random_spawn_pos(level, 8)
    }

    fn has_more_waves(&self) -> bool {
        if self.has_bonus_wave() { !self.has_spawned_bonus_wave() } else { !self.is_final_wave() }
    }

    fn is_final_wave(&self) -> bool {
        self.get_groups_spawned() == self.num_groups
    }

    fn has_bonus_wave(&self) -> bool {
        self.raid_omen_level > 1
    }

    fn has_spawned_bonus_wave(&self) -> bool {
        self.get_groups_spawned() > self.num_groups
    }

    fn should_spawn_bonus_group(&self) -> bool {
        self.is_final_wave() && self.get_total_raiders_alive() == 0 && self.has_bonus_wave()
    }

    /// `updateRaiders(level)`.
    fn update_raiders(&mut self, level: &mut ServerLevel) {
        let mut to_remove: Vec<(RaiderRef, i32)> = Vec::new();
        let waves: Vec<i32> = self.group_raider_map.keys().copied().collect();
        for wave in waves {
            let set = self.group_raider_map[&wave].ordered();
            for member in set {
                let live = level.get_entity(member.id).filter(|e| !e.is_removed());
                let Some(raider) = live else {
                    // removed (unloaded or discarded)
                    to_remove.push((member, wave));
                    continue;
                };
                let raider_pos = raider.block_position();
                if self.center.dist_sqr(&raider_pos) >= f64::from(RAID_REMOVAL_THRESHOLD_SQR) {
                    to_remove.push((member, wave));
                } else if raider.base().tick_count > 600 {
                    if level.get_entity_by_uuid(member.uuid).is_none() {
                        to_remove.push((member, wave));
                    }
                    let no_action_time = raider.living().map_or(0, |l| l.no_action_time);
                    if !raid_world::is_village(level, raider_pos) && no_action_time > MAX_NO_ACTION_TIME {
                        let ticks = raider::ticks_outside_raid(raider);
                        level.with_entity(member.id, |_, r| raider::set_ticks_outside_raid(r, ticks + 1));
                    }
                    let outside = level.get_entity(member.id).map_or(0, raider::ticks_outside_raid);
                    if outside >= OUTSIDE_RAID_BOUNDS_TIMEOUT && !to_remove.iter().any(|(r, _)| r.id == member.id) {
                        to_remove.push((member, wave));
                    }
                }
            }
        }
        for (member, wave) in to_remove {
            let was_leader = level.get_entity(member.id).is_some_and(raider::is_patrol_leader);
            let token = RaiderToken { id: member.id, wave, health: level.get_entity(member.id).map_or(member.health, |e| e.get_health()) };
            self.remove_from_raid(level, token, true, None);
            if was_leader {
                self.remove_leader(wave);
            }
        }
    }

    /// `playSound(level, soundOrigin)`.
    fn play_sound(&mut self, level: &ServerLevel, sound_origin: BlockPos) {
        let dist_away = 13.0f32;
        let range = 64;
        let players_in_raid: Vec<PlayerHandle> = self.raid_event.get_players().to_vec();
        let seed = self.random.next_long();
        for &pid in level.players() {
            let Some(player) = level.get_entity(pid) else { continue };
            let player_loc = player.position();
            let raid_loc = Vec3::at_center_of(&sound_origin);
            let dist_btwn = ((raid_loc.x - player_loc.x) * (raid_loc.x - player_loc.x) + (raid_loc.z - player_loc.z) * (raid_loc.z - player_loc.z)).sqrt();
            let x3 = player_loc.x + f64::from(dist_away) / dist_btwn * (raid_loc.x - player_loc.x);
            let z3 = player_loc.z + f64::from(dist_away) / dist_btwn * (raid_loc.z - player_loc.z);
            let in_raid = player.as_any().downcast_ref::<ServerPlayer>().and_then(|p| p.handle.as_ref()).is_some_and(|h| players_in_raid.iter().any(|p| p.same_player(h)));
            if dist_btwn <= f64::from(range) || in_raid {
                if let Some(connection) = player.as_any().downcast_ref::<ServerPlayer>().and_then(|p| p.connection.as_ref()) {
                    connection.send(&ClientboundSoundPacket::new(SoundEventHolder::Reference(SoundEvent::EVENT_RAID_HORN), SoundSource::Neutral, x3, player.base().y(), z3, 64.0, 1.0, seed));
                }
            }
        }
    }

    /// `spawnGroup(level, pos)`.
    fn spawn_group(&mut self, level: &mut ServerLevel, pos: BlockPos) {
        let mut leader_set = false;
        let group_number = self.groups_spawned + 1;
        self.total_health = 0.0;
        let difficulty = crate::world::level::server_level_accessor::ServerLevelAccessor::get_current_difficulty_at(&*level, pos);
        let is_bonus_group = self.should_spawn_bonus_group();

        for raider_type in RaiderType::VALUES {
            let num_spawns = self.get_default_num_spawns(raider_type, group_number, is_bonus_group) + self.get_potential_bonus_spawns(raider_type, group_number, difficulty.get_difficulty(), is_bonus_group);
            let mut ravagers_spawned = 0;

            for _ in 0..num_spawns {
                let Some(mut raider) = factory::create(raider_type.entity_type(), level, EntitySpawnRequest { reason: EntitySpawnReason::Event, ignore_checks: false }) else {
                    break;
                };
                if !leader_set && raider::can_be_leader(&*raider) {
                    raider::set_patrol_leader(&mut *raider, true);
                    self.set_leader(level, group_number, &mut *raider);
                    leader_set = true;
                }
                let Some(raider_id) = self.join_raid_fresh(level, group_number, raider, pos) else {
                    continue;
                };
                if raider_type.entity_type() == EntityType::RAVAGER {
                    let mut riding_raider: Option<Box<dyn Entity>> = None;
                    let request = EntitySpawnRequest { reason: EntitySpawnReason::Event, ignore_checks: false };
                    if group_number == Raid::get_num_groups_for(Difficulty::Normal) {
                        riding_raider = factory::create(EntityType::PILLAGER, level, request);
                    } else if group_number >= Raid::get_num_groups_for(Difficulty::Hard) {
                        if ravagers_spawned == 0 {
                            riding_raider = factory::create(EntityType::EVOKER, level, request);
                        } else {
                            riding_raider = factory::create(EntityType::VINDICATOR, level, request);
                        }
                    }
                    ravagers_spawned += 1;
                    if let Some(riding_raider) = riding_raider
                        && let Some(rider_id) = self.join_raid_fresh(level, group_number, riding_raider, pos)
                    {
                        level.with_entity(rider_id, |level, rider| {
                            rider.snap_to(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), 0.0, 0.0);
                            level.start_riding(rider, raider_id, false, false);
                        });
                    }
                }
            }
        }

        self.wave_spawn_pos = None;
        self.groups_spawned += 1;
        self.update_bossbar(level, None);
        self.set_dirty(level);
    }

    /// `joinRaid(level, groupNumber, raider, pos, false)` for a raider that is not in the level yet: sets it up, adds it
    /// (with passengers) and returns its id.
    pub fn join_raid_fresh(&mut self, level: &mut ServerLevel, group_number: i32, mut raider: Box<dyn Entity>, pos: BlockPos) -> Option<EntityId> {
        self.add_wave_mob(level, group_number, &*raider, true);
        raider::set_current_raid(&mut *raider, Some(self.id));
        raider::set_wave(&mut *raider, group_number);
        raider::set_can_join_raid(&mut *raider, true);
        raider::set_ticks_outside_raid(&mut *raider, 0);
        raider.set_pos(f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 1.0, f64::from(pos.z()) + 0.5);
        let difficulty = crate::world::level::server_level_accessor::ServerLevelAccessor::get_current_difficulty_at(&*level, pos);
        raider.finalize_spawn(level, difficulty, EntitySpawnReason::Event, None);
        raider.apply_raid_buffs(level, group_number, false, self);
        raider.base_mut().set_on_ground_raw(true);
        let id = raider.id();
        level.add_fresh_entity_with_passengers(vec![raider]);
        level.get_entity(id).map(|_| id)
    }

    /// `joinRaid(level, groupNumber, raider, null, true)`: an existing, ticking raider (taken out of the level) joins.
    pub fn join_raid_existing(&mut self, level: &mut ServerLevel, group_number: i32, raider: &mut dyn Entity) {
        self.add_wave_mob(level, group_number, raider, true);
        raider::set_current_raid(raider, Some(self.id));
        raider::set_wave(raider, group_number);
        raider::set_can_join_raid(raider, true);
        raider::set_ticks_outside_raid(raider, 0);
    }

    /// `updateBossbar()`; `current` is a raider that is taken out of the level right now.
    pub fn update_bossbar(&mut self, level: &ServerLevel, current: Option<&dyn Entity>) {
        let health = self.get_health_of_living_raiders(level, current);
        self.raid_event.set_progress(mth::clamp_f32(health / self.total_health, 0.0, 1.0));
    }

    /// `getHealthOfLivingRaiders()`.
    pub fn get_health_of_living_raiders(&mut self, level: &ServerLevel, current: Option<&dyn Entity>) -> f32 {
        let mut health = 0.0f32;
        for set in self.group_raider_map.values_mut() {
            for member in set.ordered() {
                let live: Option<&dyn Entity> = match current {
                    Some(c) if c.id() == member.id => Some(c),
                    _ => level.get_entity(member.id),
                };
                let h = match live {
                    Some(e) => {
                        let h = e.get_health();
                        if let Some(m) = set.members.iter_mut().find(|m| m.id == member.id) {
                            m.health = h;
                        }
                        h
                    }
                    None => member.health,
                };
                health += h;
            }
        }
        health
    }

    fn should_spawn_group(&self) -> bool {
        self.raid_cooldown_ticks == 0 && (self.groups_spawned < self.num_groups || self.should_spawn_bonus_group()) && self.get_total_raiders_alive() == 0
    }

    /// `getTotalRaidersAlive()`.
    pub fn get_total_raiders_alive(&self) -> i32 {
        self.group_raider_map.values().map(|s| s.len() as i32).sum()
    }

    /// `removeFromRaid(level, raider, removeFromTotalHealth)`; `current` is the raider if it is taken out of the level.
    pub fn remove_from_raid(&mut self, level: &mut ServerLevel, raider: RaiderToken, remove_from_total_health: bool, current: Option<&dyn Entity>) {
        let Some(raiders) = self.group_raider_map.get_mut(&raider.wave) else {
            return;
        };
        if raiders.remove(raider.id) {
            if remove_from_total_health {
                self.total_health -= raider.health;
            }
            // `raider.setCurrentRaid(null)`: the callers that hold the entity clear it; a raider in the level is cleared here
            level.with_entity(raider.id, |_, r| raider::set_current_raid(r, None));
            self.update_bossbar(level, current);
            self.set_dirty(level);
        }
    }

    fn set_dirty(&self, level: &mut ServerLevel) {
        super::raids::set_dirty(level);
    }

    /// `getOminousBannerInstance(registries)`.
    pub fn get_ominous_banner_instance(registries: &crate::registry::RegistryAccess) -> ItemStack {
        ItemStack::with_patch(Item::WHITE_BANNER, 1, &Raid::get_banner_component_patch(registries))
    }

    /// `getBannerComponentPatch(patternGetter)`.
    pub fn get_banner_component_patch(registries: &crate::registry::RegistryAccess) -> DataComponentPatch {
        let registered = |name: &str| registries.registry(crate::registry::keys::BANNER_PATTERN).is_some_and(|r| Identifier::parse(name).is_ok_and(|id| r.id_of(&id).is_some()));
        let mut layers: Vec<BannerPatternLayer> = Vec::new();
        for (name, color) in [
            ("rhombus", DyeColor::Cyan),
            ("stripe_bottom", DyeColor::LightGray),
            ("stripe_center", DyeColor::Gray),
            ("border", DyeColor::LightGray),
            ("stripe_middle", DyeColor::Black),
            ("half_horizontal", DyeColor::LightGray),
            ("circle", DyeColor::LightGray),
            ("border", DyeColor::Black),
        ] {
            if registered(&format!("minecraft:{name}")) {
                layers.push(BannerPatternLayer { pattern: Holder::reference(name), color });
            }
        }
        DataComponentPatch::builder()
            .set(BANNER_PATTERNS, BannerPatternLayers { layers })
            .set(TOOLTIP_DISPLAY, TooltipDisplay::DEFAULT.with_hidden(DataComponentType::BANNER_PATTERNS, true))
            .set(ITEM_NAME, Component::translatable("block.minecraft.ominous_banner", vec![]))
            .set(RARITY, Rarity::Uncommon)
            .build()
    }

    /// `getLeader(wave)` (the leader's id).
    pub fn get_leader(&self, wave: i32) -> Option<EntityId> {
        self.group_to_leader_map.get(&wave).map(|r| r.id)
    }

    /// `findRandomSpawnPos(level, maxTries)`.
    fn find_random_spawn_pos(&mut self, level: &ServerLevel, max_tries: i32) -> Option<BlockPos> {
        let seconds_remaining = self.raid_cooldown_ticks / 20;
        let how_far = 0.22f32 * seconds_remaining as f32 - 0.24f32;
        let start_angle = self.random.next_float() * (std::f32::consts::PI * 2.0);

        for i in 0..max_tries {
            let angle = start_angle + std::f32::consts::PI * i as f32 / 8.0;
            let spawn_x = self.center.x().wrapping_add(mth::floor_f32(mth::cos(f64::from(angle)) * 32.0 * how_far)).wrapping_add(self.random.next_int_bounded(3).wrapping_mul(mth::floor_f32(how_far)));
            let spawn_z = self.center.z().wrapping_add(mth::floor_f32(mth::sin(f64::from(angle)) * 32.0 * how_far)).wrapping_add(self.random.next_int_bounded(3).wrapping_mul(mth::floor_f32(how_far)));
            let spawn_y = level.get_height_at(HeightmapType::WorldSurface, spawn_x, spawn_z);
            if mth::abs_i32(spawn_y.wrapping_sub(self.center.y())) <= 96 {
                let spawn_pos = BlockPos::new(spawn_x, spawn_y, spawn_z);
                if !raid_world::is_village(level, spawn_pos) || seconds_remaining <= 7 {
                    let delta = 10;
                    if movement_has_chunks_at(level, spawn_pos.x() - delta, spawn_pos.z() - delta, spawn_pos.x() + delta, spawn_pos.z() + delta)
                        && crate::world::entity::living_entity::util::is_position_entity_ticking(level, spawn_pos)
                        && (spawn_placements::is_spawn_position_ok(EntityType::RAVAGER, level, spawn_pos)
                            || (level.get_block_state(spawn_pos.below()).is_block(Block::SNOW) && level.get_block_state(spawn_pos).is_air()))
                    {
                        return Some(spawn_pos);
                    }
                }
            }
        }
        None
    }

    /// `addWaveMob(level, wave, raider, updateHealth)`.
    pub fn add_wave_mob(&mut self, level: &mut ServerLevel, wave: i32, raider: &dyn Entity, update_health: bool) -> bool {
        let set = self.group_raider_map.entry(wave).or_default();
        let uuid = raider.uuid();
        // an older copy of the same raider (reloaded from a chunk) is replaced
        if let Some(existing) = set.members.iter().find(|r| r.uuid == uuid && r.id != raider.id()).map(|r| r.id) {
            set.remove(existing);
        }
        set.add(RaiderRef { id: raider.id(), uuid, health: raider.get_health() });
        if update_health {
            self.total_health += raider.get_health();
        }
        self.update_bossbar(level, Some(raider));
        self.set_dirty(level);
        true
    }

    /// `setLeader(wave, raider)`.
    pub fn set_leader(&mut self, level: &mut ServerLevel, wave: i32, raider: &mut dyn Entity) {
        self.group_to_leader_map.insert(wave, RaiderRef { id: raider.id(), uuid: raider.uuid(), health: raider.get_health() });
        let banner = Raid::get_ominous_banner_instance(&level.registries);
        crate::world::entity::mob::equipment::set_item_slot(raider, level, EquipmentSlot::Head, banner);
        raider.set_drop_chance(EquipmentSlot::Head, 2.0);
    }

    /// `removeLeader(wave)`.
    pub fn remove_leader(&mut self, wave: i32) {
        self.group_to_leader_map.remove(&wave);
    }

    fn get_default_num_spawns(&self, ty: RaiderType, wav: i32, is_bonus_wave: bool) -> i32 {
        let table = ty.spawns_per_wave_before_bonus();
        if is_bonus_wave { table[self.num_groups as usize] } else { table[wav as usize] }
    }

    /// `getPotentialBonusSpawns(type, random, wav, difficultyInstance, isBonusWave)`.
    fn get_potential_bonus_spawns(&mut self, ty: RaiderType, wav: i32, difficulty: Difficulty, is_bonus_wave: bool) -> i32 {
        let is_easy = difficulty == Difficulty::Easy;
        let is_normal = difficulty == Difficulty::Normal;
        let bonus_spawns = match ty {
            RaiderType::Vindicator | RaiderType::Pillager => {
                if is_easy {
                    self.random.next_int_bounded(2)
                } else if is_normal {
                    1
                } else {
                    2
                }
            }
            RaiderType::Evoker => return 0,
            RaiderType::Witch => {
                if is_easy || wav <= 2 || wav == 4 {
                    return 0;
                }
                1
            }
            RaiderType::Ravager => i32::from(!is_easy && is_bonus_wave),
        };
        if bonus_spawns > 0 { self.random.next_int_bounded(bonus_spawns + 1) } else { 0 }
    }

    /// `getNumGroups(difficulty)`.
    pub fn get_num_groups(&self, difficulty: Difficulty) -> i32 {
        Raid::get_num_groups_for(difficulty)
    }

    fn get_num_groups_for(difficulty: Difficulty) -> i32 {
        match difficulty {
            Difficulty::Peaceful => 0,
            Difficulty::Easy => 3,
            Difficulty::Normal => 5,
            Difficulty::Hard => 7,
        }
    }

    pub fn get_num_groups_field(&self) -> i32 {
        self.num_groups
    }

    /// `getEnchantOdds()`.
    pub fn get_enchant_odds(&self) -> f32 {
        match self.get_raid_omen_level() {
            2 => 0.1,
            3 => 0.25,
            4 => 0.5,
            5 => 0.75,
            _ => 0.0,
        }
    }

    /// `addHeroOfTheVillage(killer)`.
    pub fn add_hero_of_the_village(&mut self, uuid: Uuid) {
        if !self.heroes_of_the_village.contains(&uuid) {
            self.heroes_of_the_village.push(uuid);
            self.heroes_capacity.grow_to_fit(self.heroes_of_the_village.len());
        }
    }

    fn ordered_heroes(&self) -> Vec<Uuid> {
        self.heroes_capacity.iteration_order(self.heroes_of_the_village.iter().copied(), java_hash_code)
    }

    // ---- saved data (`Raid.MAP_CODEC`) -------------------------------------------------------------------------

    /// `Raid.MAP_CODEC` decode (the fields of the raid are the record's, flat in `map`).
    pub fn decode(map: &CompoundTag) -> DecodeResult<Raid> {
        let started = codec::boolean(codec::field(map, "started")?)?;
        let active = codec::boolean(codec::field(map, "active")?)?;
        let ticks_active = codec::long(codec::field(map, "ticks_active")?)?;
        let raid_omen_level = codec::int(codec::field(map, "raid_omen_level")?)?;
        let groups_spawned = codec::int(codec::field(map, "groups_spawned")?)?;
        let raid_cooldown_ticks = codec::int(codec::field(map, "cooldown_ticks")?)?;
        let post_raid_ticks = codec::int(codec::field(map, "post_raid_ticks")?)?;
        let total_health = codec::float(codec::field(map, "total_health")?)?;
        let num_groups = codec::int(codec::field(map, "group_count")?)?;
        let status_name = codec::string(codec::field(map, "status")?)?;
        let status = RaidStatus::by_name(status_name).ok_or_else(|| format!("Unknown element name:{status_name}"))?;
        let center = decode_block_pos(codec::field(map, "center")?)?;
        let mut heroes: Vec<Uuid> = Vec::new();
        for tag in codec::list(codec::field(map, "heroes_of_the_village")?)? {
            heroes.push(decode_uuid(&tag)?);
        }
        let mut random = random::create_new();
        let event_id = mth::create_insecure_uuid(&mut random);
        let mut raid = Raid::blank(center, event_id, random);
        raid.started = started;
        raid.active = active;
        raid.ticks_active = ticks_active;
        raid.raid_omen_level = raid_omen_level;
        raid.groups_spawned = groups_spawned;
        raid.raid_cooldown_ticks = raid_cooldown_ticks;
        raid.post_raid_ticks = post_raid_ticks;
        raid.total_health = total_health;
        raid.num_groups = num_groups;
        raid.status = status;
        for h in heroes {
            raid.add_hero_of_the_village_uuid(h);
        }
        Ok(raid)
    }

    fn add_hero_of_the_village_uuid(&mut self, uuid: Uuid) {
        if !self.heroes_of_the_village.contains(&uuid) {
            self.heroes_of_the_village.push(uuid);
            self.heroes_capacity.grow_to_fit(self.heroes_of_the_village.len());
        }
    }

    /// `Raid.MAP_CODEC` encode into `out`.
    pub fn encode_into(&self, out: &mut CompoundTag) {
        out.put_boolean("started", self.started);
        out.put_boolean("active", self.active);
        out.put_long("ticks_active", self.ticks_active);
        out.put_int("raid_omen_level", self.raid_omen_level);
        out.put_int("groups_spawned", self.groups_spawned);
        out.put_int("cooldown_ticks", self.raid_cooldown_ticks);
        out.put_int("post_raid_ticks", self.post_raid_ticks);
        out.put_float("total_health", self.total_health);
        out.put_int("group_count", self.num_groups);
        out.put_string("status", self.status.get_serialized_name());
        out.put_int_array("center", vec![self.center.x(), self.center.y(), self.center.z()]);
        let mut heroes = ListTag::new();
        for h in self.ordered_heroes() {
            heroes.push(Tag::IntArray(uuid_to_int_array(&h).to_vec()));
        }
        out.put("heroes_of_the_village", heroes);
    }
}

/// A raider as `removeFromRaid` needs it: its entity id, `getWave()` and `getHealth()`.
#[derive(Clone, Copy, Debug)]
pub struct RaiderToken {
    pub id: EntityId,
    pub wave: i32,
    pub health: f32,
}

fn movement_has_chunks_at(level: &ServerLevel, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    crate::world::entity::movement::MovementLevel::has_chunks_at(level, x0, z0, x1, z1)
}

/// `BlockPos.CODEC` (an int array of 3).
pub(super) fn decode_block_pos(tag: &Tag) -> DataResult<BlockPos> {
    match tag {
        Tag::IntArray(v) if v.len() == 3 => Ok(BlockPos::new(v[0], v[1], v[2])),
        _ => Err("Failed to decode BlockPos".to_string()),
    }
}

/// `UUIDUtil.CODEC` (an int array of 4).
fn decode_uuid(tag: &Tag) -> DataResult<Uuid> {
    match tag {
        Tag::IntArray(v) if v.len() == 4 => Ok(uuid_from_int_array([v[0], v[1], v[2], v[3]])),
        _ => Err("Expected an int array of 4".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ominous_banners_match() {
        let registries = crate::registry::RegistryAccess::default();
        let a = Raid::get_ominous_banner_instance(&registries);
        let b = Raid::get_ominous_banner_instance(&registries);
        assert!(!a.is_empty());
        assert!(ItemStack::matches(&a, &b));
    }
}
