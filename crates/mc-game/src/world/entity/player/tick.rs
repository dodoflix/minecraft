//! `ServerPlayer.tick` / `doTick` and the `Player.tick` bases (health/food/xp sync, menu broadcast, level-driven
//! counters).

use super::food_data::FoodTickRules;
use super::menu_ctx::{PlayerCtx, ServerPlayerMenuInit};
use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::movement;
use crate::world::entity::{Entity, EntityId};
use crate::world::inventory::PlayerMenus;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::{ClientboundSetExperiencePacket, ClientboundSetHealthPacket};

impl ServerPlayer {
    /// `initInventoryMenu()` for this player's connection (no-op without one).
    pub fn init_inventory_menu(&mut self, level: &mut ServerLevel) {
        let Some(connection) = self.connection.clone() else { return };
        let init = ServerPlayerMenuInit { connection };
        let mut ctx = PlayerCtx { player: self, level };
        let mut menus = std::mem::replace(&mut ctx.player.player.menus, PlayerMenus::new());
        menus.init_inventory_menu(&init, &mut ctx);
        ctx.player.player.menus = menus;
    }

    /// `containerMenu.broadcastChanges()`.
    pub fn broadcast_menu_changes(&mut self, level: &mut ServerLevel) {
        let mut ctx = PlayerCtx { player: self, level };
        PlayerMenus::with_current(&mut ctx, |c| &mut c.player.player.menus, |menu, c| menu.base_mut().broadcast_changes(c));
    }

    /// `isWithinBlockInteractionRange(pos, buffer)`: `BLOCK_INTERACTION_RANGE` attribute + `buffer`.
    pub fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool {
        let range = self.player.living.get_attribute_value(mc_data::Attribute::BLOCK_INTERACTION_RANGE) + buffer;
        let bb = mc_core::phys::AABB::from_block_pos(&pos);
        bb.distance_to_sqr(self.base().get_eye_position()) < range * range
    }

    /// `isWithinEntityInteractionRange(bb, buffer)`.
    pub fn is_within_entity_interaction_range(&self, target: &mc_core::phys::AABB, buffer: f64) -> bool {
        let range = self.player.living.get_attribute_value(mc_data::Attribute::ENTITY_INTERACTION_RANGE) + buffer;
        target.distance_to_sqr(self.base().get_eye_position()) < range * range
    }

    /// `ServerPlayer.tick()`: game mode tick, damage cooldown, menu broadcast (camera/advancement parts omitted).
    pub fn tick_server_player(&mut self, level: &mut ServerLevel) {
        self.game_mode.game_ticks += 1;
        let ws = &mut self.warden_spawn_tracker;
        ws.ticks_since_last_warning = ws.ticks_since_last_warning.saturating_add(1);
        if self.player.living.damage_cooldown_time > 0 {
            self.player.living.damage_cooldown_time -= 1;
        }
        self.broadcast_menu_changes(level);
    }

    /// `Player.tick()` + `LivingEntity.tick()`/`Entity.tick()` bases that run on the server for players.
    fn player_base_tick(&mut self, level: &mut ServerLevel) {
        let spectator = self.is_spectator();
        self.base_mut().no_physics = spectator;
        if spectator || self.base().is_passenger() {
            movement::set_on_ground(self, level, false);
        }
        if self.player.take_xp_delay > 0 {
            self.player.take_xp_delay -= 1;
        }
        if self.player.living.is_sleeping() {
            self.player.sleep_counter = (self.player.sleep_counter + 1).min(100);
        } else if self.player.sleep_counter > 0 {
            self.player.sleep_counter += 1;
            if self.player.sleep_counter >= 110 {
                self.player.sleep_counter = 0;
            }
        }
        self.player.was_underwater = self.base().eye_in_fluid[0];
        // Entity.baseTick
        self.base_tick(level);
        // LivingEntity.baseTick counters
        let l = &mut self.player.living;
        if l.hurt_time > 0 {
            l.hurt_time -= 1;
        }
        if l.last_hurt_by_player_memory_time > 0 {
            l.last_hurt_by_player_memory_time -= 1;
        } else {
            l.last_hurt_by_player = None;
        }
        l.y_head_rot_o = l.y_head_rot;
        l.y_body_rot_o = l.y_body_rot;
        // Player.tick tail
        let (x, z) = (self.base().x(), self.base().z());
        let (nx, nz) = (mth::clamp_f64(x, -2.9999999E7, 2.9999999E7), mth::clamp_f64(z, -2.9999999E7, 2.9999999E7));
        if nx != x || nz != z {
            let y = self.base().y();
            self.set_pos(nx, y, nz);
        }
        self.player.living.attack_strength_ticker += 1;
        self.player.living.item_swap_ticker += 1;
    }

    /// `ServerPlayer.doTick()`.
    pub fn do_tick(&mut self, level: &mut ServerLevel) {
        if !self.is_spectator() || !movement::touching_unloaded_chunk(self, level) {
            self.player_base_tick(level);
            let rules = level.player_food_rules();
            let health = self.player.living.get_health();
            let max = self.player.living.get_max_health();
            let r = self.player.food_data.tick(rules, health, max);
            if r.heal > 0.0 {
                let h = self.player.living.get_health();
                self.player.living.set_health(h + r.heal);
            }
            // `starve` damage goes through the damage layer (`hurtServer`), which does not exist yet.
            let _ = r.starve;
        }
        let health = self.player.living.get_health();
        let food = self.player.food_data.food_level;
        let saturation = self.player.food_data.saturation_level;
        if health != self.last_sent_health || self.last_sent_food != food || (saturation == 0.0) != self.last_food_saturation_zero {
            self.send(&ClientboundSetHealthPacket { health, food, saturation });
            self.last_sent_health = health;
            self.last_sent_food = food;
            self.last_food_saturation_zero = saturation == 0.0;
        }
        let hp_abs = health + self.player.get_absorption_amount();
        if hp_abs != self.last_recorded_health_and_absorption {
            self.last_recorded_health_and_absorption = hp_abs;
        }
        self.last_recorded_food_level = food;
        self.last_recorded_air_level = self.base().get_air_supply();
        self.last_recorded_level = self.player.experience_level;
        if self.player.total_experience != self.last_recorded_experience {
            self.last_recorded_experience = self.player.total_experience;
        }
        if self.player.total_experience != self.last_sent_exp {
            self.last_sent_exp = self.player.total_experience;
            self.send(&ClientboundSetExperiencePacket {
                experience_progress: self.player.experience_progress,
                experience_level: self.player.experience_level,
                total_experience: self.player.total_experience,
            });
        }
        // the player list's view of this player (tab list, broadcasts by range/dimension)
        self.sync_handle();
    }
}

