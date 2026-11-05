//! `ServerPlayer.startSleepInBed` / `stopSleepInBed` / `startSleeping` (+ `Player.startSleepInBed`, `BedSleepingProblem`).

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::stats::Stats;
use crate::world::attribute::BedRule;
use crate::world::entity::Entity;
use crate::world::entity::living_entity::sleeping;
use crate::world::entity::respawn::{RespawnConfig, respawn_data_of};
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::abstract_bed_block;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{EntityType, props};
use mc_text::Component;

/// `Player.BedSleepingProblem`.
#[derive(Clone, Debug, PartialEq)]
pub struct BedSleepingProblem {
    pub message: Option<Component>,
}

impl BedSleepingProblem {
    fn translatable(key: &str) -> BedSleepingProblem {
        BedSleepingProblem { message: Some(Component::translatable(key, Vec::new())) }
    }
    pub fn too_far_away() -> BedSleepingProblem {
        Self::translatable("block.minecraft.bed.too_far_away")
    }
    pub fn obstructed() -> BedSleepingProblem {
        Self::translatable("block.minecraft.bed.obstructed")
    }
    pub fn not_safe() -> BedSleepingProblem {
        Self::translatable("block.minecraft.bed.not_safe")
    }
    pub fn other_problem() -> BedSleepingProblem {
        BedSleepingProblem { message: None }
    }
}

/// `Monster` and its subclasses among the entity types (`EntityTypes` registration classes extending `Monster`).
const MONSTER_TYPES: [EntityType; 34] = [
    EntityType::BLAZE,
    EntityType::BOGGED,
    EntityType::BREEZE,
    EntityType::CAVE_SPIDER,
    EntityType::CREAKING,
    EntityType::CREEPER,
    EntityType::DROWNED,
    EntityType::ELDER_GUARDIAN,
    EntityType::ENDERMAN,
    EntityType::ENDERMITE,
    EntityType::EVOKER,
    EntityType::GIANT,
    EntityType::GUARDIAN,
    EntityType::HUSK,
    EntityType::ILLUSIONER,
    EntityType::PARCHED,
    EntityType::PIGLIN,
    EntityType::PIGLIN_BRUTE,
    EntityType::PILLAGER,
    EntityType::RAVAGER,
    EntityType::SILVERFISH,
    EntityType::SKELETON,
    EntityType::SPIDER,
    EntityType::STRAY,
    EntityType::VEX,
    EntityType::VINDICATOR,
    EntityType::WARDEN,
    EntityType::WITCH,
    EntityType::WITHER,
    EntityType::WITHER_SKELETON,
    EntityType::ZOGLIN,
    EntityType::ZOMBIE,
    EntityType::ZOMBIE_VILLAGER,
    EntityType::ZOMBIFIED_PIGLIN,
];

impl ServerPlayer {
    /// `ServerPlayer.startSleepInBed(bedBlock, bedBlockState, rule, pos)`.
    pub fn start_sleep_in_bed(&mut self, level: &mut ServerLevel, bed_state: BlockState, rule: &BedRule, pos: BlockPos) -> Result<(), BedSleepingProblem> {
        let direction = bed_state.get(props::HORIZONTAL_FACING);
        if self.player.living.is_sleeping() || !Entity::is_alive(self) {
            return Err(BedSleepingProblem::other_problem());
        }
        let can_sleep = rule.can_sleep(level);
        let can_set_spawn = rule.can_set_spawn(level);
        if !self.bed_in_range(pos, direction) {
            return Err(BedSleepingProblem::too_far_away());
        }
        if self.bed_blocked(level, pos, direction) {
            return Err(BedSleepingProblem::obstructed());
        }
        if can_set_spawn {
            let data = respawn_data_of(level.dimension.clone(), pos, self.base().y_rot(), self.base().x_rot());
            self.set_respawn_position(Some(RespawnConfig { respawn_data: data, forced: false }), true);
        }
        if !can_sleep {
            return Err(BedSleepingProblem { message: rule.error_message.clone() });
        }
        if !self.is_creative() {
            let bed_center = Vec3::at_bottom_center_of(&pos);
            let bb = AABB::new(bed_center.x - 8.0, bed_center.y - 5.0, bed_center.z - 8.0, bed_center.x + 8.0, bed_center.y + 5.0, bed_center.z + 8.0);
            let player: &dyn Entity = &*self;
            let monsters = level.get_entities_of_class(&bb, |e| MONSTER_TYPES.contains(&e.get_type()), |e| e.is_preventing_player_rest(level, player));
            if !monsters.is_empty() {
                return Err(BedSleepingProblem::not_safe());
            }
        }
        // `Player.startSleepInBed`
        if !Entity::start_sleeping(self, level, pos) {
            return Err(BedSleepingProblem::other_problem());
        }
        self.player.sleep_counter = 0;
        self.award_stat_once(level, abstract_bed_block::get_slept_in_bed_stat_type(bed_state));
        if can_set_spawn {
            // CriteriaTriggers::SLEPT_IN_BED.trigger(self) once the advancement triggers land
        }
        if !level.can_sleep_through_nights() {
            self.send_overlay_message(Component::translatable("sleep.not_possible", Vec::new()));
        }
        level.update_sleeping_player_list();
        Ok(())
    }

    /// `ServerPlayer.startSleeping` after `LivingEntity.startSleeping`: resets the rest timer and syncs the position.
    pub fn after_start_sleeping(&mut self, level: &mut ServerLevel) {
        self.reset_stat(level, Stats::TIME_SINCE_REST);
        if self.listener.is_some() {
            let (p, y_rot, x_rot) = (self.base().position(), self.base().y_rot(), self.base().x_rot());
            self.connection_teleport(p.x, p.y, p.z, y_rot, x_rot);
        }
    }

    /// `bedInRange(pos, direction)`.
    fn bed_in_range(&self, pos: BlockPos, direction: Direction) -> bool {
        self.is_reachable_bed_block(pos) || self.is_reachable_bed_block(pos.relative(direction.get_opposite()))
    }

    fn is_reachable_bed_block(&self, bed_block_pos: BlockPos) -> bool {
        let center = Vec3::at_bottom_center_of(&bed_block_pos);
        let p = self.base().position();
        (p.x - center.x).abs() <= 3.0 && (p.y - center.y).abs() <= 2.0 && (p.z - center.z).abs() <= 3.0
    }

    /// `bedBlocked(pos, direction)`.
    fn bed_blocked(&self, level: &ServerLevel, pos: BlockPos, direction: Direction) -> bool {
        let above = pos.above();
        !free_at(level, above) || !free_at(level, above.relative(direction.get_opposite()))
    }

    /// `ServerPlayer.stopSleepInBed(forcefulWakeUp, updateLevelList)`.
    pub fn stop_sleep_in_bed_impl(&mut self, level: &mut ServerLevel, forceful_wake_up: bool, update_level_list: bool) {
        if self.player.living.is_sleeping() {
            let packet = mc_protocol::packets::play::ClientboundAnimatePacket { id: self.base().id, action: 0 };
            level.entity_tracker.send_to_tracking_players_and_self(self.base().id, &crate::server::level::level_events::encode_packet(&packet));
        }
        // `Player.stopSleepInBed`
        sleeping::stop_sleeping(self.as_dyn_mut(), level);
        if update_level_list {
            level.update_sleeping_player_list();
        }
        self.player.sleep_counter = if forceful_wake_up { 0 } else { 100 };
        if self.listener.is_some() {
            let (p, y_rot, x_rot) = (self.base().position(), self.base().y_rot(), self.base().x_rot());
            self.connection_teleport(p.x, p.y, p.z, y_rot, x_rot);
        }
    }
}

/// `Player.freeAt(pos)`.
fn free_at(level: &ServerLevel, pos: BlockPos) -> bool {
    !level.get_block_state(pos).is_suffocating_at(level, pos)
}
