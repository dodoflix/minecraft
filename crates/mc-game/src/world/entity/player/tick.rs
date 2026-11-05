//! `ServerPlayer.tick` / `doTick` and the `Player.tick` bases (health/food/xp sync, menu broadcast, level-driven
//! counters).

use crate::stats::stats::Stats;
use crate::world::scores::ObjectiveCriteria;
use super::menu_ctx::{PlayerCtx, ServerPlayerMenuInit};
use crate::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_core::ident::Identifier;
use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::{living_entity, movement};
use crate::world::entity::{Entity, EntityId};
use crate::world::inventory::PlayerMenus;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
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

    /// `ServerPlayer.updatePlayerAttributes()`: creative interaction range bonuses and the crouching waypoint modifier.
    /// Calling `getAttribute` also instantiates the three attributes in the map (they are saved with the player).
    pub fn update_player_attributes(&mut self) {
        use mc_data::Attribute;
        use mc_data::enums::Pose;
        let creative = self.is_creative();
        let crouching = self.base().get_pose() == Pose::Crouching;
        let modifier = |path: &str, amount: f64, operation: AttributeModifierOperation| AttributeModifier {
            id: Identifier::new("minecraft", path).expect("valid id"),
            amount,
            operation,
        };
        let attributes = &mut self.player.living.attributes;
        let mut update = |attribute: Attribute, modifier: AttributeModifier, active: bool| {
            if let Some(instance) = attributes.get_instance(attribute) {
                if active {
                    instance.add_or_update_transient_modifier(modifier);
                } else {
                    instance.remove_modifier(&modifier.id);
                }
            }
        };
        update(Attribute::BLOCK_INTERACTION_RANGE, modifier("creative_mode_block_range", 0.5, AttributeModifierOperation::AddValue), creative);
        update(Attribute::ENTITY_INTERACTION_RANGE, modifier("creative_mode_entity_range", 2.0, AttributeModifierOperation::AddValue), creative);
        update(
            Attribute::WAYPOINT_TRANSMIT_RANGE,
            modifier("waypoint_transmit_range_crouch", -1.0, AttributeModifierOperation::AddMultipliedTotal),
            crouching,
        );
    }

    /// `ServerPlayer.tick()`: game mode tick, damage cooldown, menu broadcast + validity, falling/lava tracking, attributes,
    /// post effects. The camera follow-up belongs to the level slice's spectator camera; the criteria triggers and
    /// `advancements.flushDirty` are `// SEAM(advancements)`.
    pub fn tick_server_player(&mut self, level: &mut ServerLevel) {
        self.tick_game_mode(level);
        let ws = &mut self.warden_spawn_tracker;
        ws.ticks_since_last_warning = ws.ticks_since_last_warning.saturating_add(1);
        if self.player.living.damage_cooldown_time > 0 {
            self.player.living.damage_cooldown_time -= 1;
        }
        self.broadcast_menu_changes(level);
<<<<<<< HEAD
        self.close_container_if_invalid(level);
        // SEAM(advancements): CriteriaTriggers.TICK / LEVITATION
        self.track_start_falling_position();
        self.track_entered_or_exited_lava_on_vehicle(level);
=======
        self.tick_camera(level);
>>>>>>> cbfecbb (feat(game): spectator camera, game mode tick wired into the player tick, entity event broadcast helpers)
        self.update_player_attributes();
        // SEAM(advancements): advancements.flushDirty(this, true)
        if self.post_effects_dirty {
            crate::server::players::join::send_post_effects(self);
        }
    }

    /// `if (!containerMenu.stillValid(this)) { closeContainer(); containerMenu = inventoryMenu; }` (`ServerPlayer.closeContainer`
    /// = `ClientboundContainerClosePacket` + `doCloseContainer`).
    fn close_container_if_invalid(&mut self, level: &mut ServerLevel) {
        let mut ctx = PlayerCtx { player: self, level };
        let valid = PlayerMenus::with_current(&mut ctx, |c| &mut c.player.player.menus, |menu, c| menu.still_valid(c));
        if !valid {
            let id = self.player.menus.current_container_id();
            self.send(&mc_protocol::packets::play::ClientboundContainerClosePacket { container_id: id });
            self.do_close_container(level);
        }
    }

    /// `trackStartFallingPosition()`.
    pub fn track_start_falling_position(&mut self) {
        if self.base().fall_distance > 0.0 && self.starting_to_fall_position.is_none() {
            let position = self.base().position();
            self.starting_to_fall_position = Some(position);
            // SEAM(advancements): FALL_AFTER_EXPLOSION when `currentExplosionImpactPos.y <= position.y`
            self.current_explosion_impact_pos = None;
        }
    }

    /// `trackEnteredOrExitedLavaOnVehicle()`.
    pub fn track_entered_or_exited_lava_on_vehicle(&mut self, level: &ServerLevel) {
        let in_lava = |id| level.get_entity(id).is_some_and(|v| v.is_in_lava());
        let vehicle = self.base().vehicle;
        if vehicle.is_some_and(in_lava) && self.entered_lava_on_vehicle_position.is_none() {
            self.entered_lava_on_vehicle_position = Some(self.base().position());
        }
        // SEAM(advancements): RIDE_ENTITY_IN_LAVA_TRIGGER while already entered
        if self.entered_lava_on_vehicle_position.is_some() && !vehicle.is_some_and(in_lava) {
            self.entered_lava_on_vehicle_position = None;
        }
    }

    /// `Player.tick()`; `super.tick()` is the generic `LivingEntity.tick` (which runs `aiStep` = `Player.aiStep` below).
    pub(super) fn player_tick(&mut self, level: &mut ServerLevel) {
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
        // `updateIsUnderwater()`
        self.player.was_underwater = self.base().eye_in_fluid[0];
        living_entity::tick::tick(self, level);
        let (x, z) = (self.base().x(), self.base().z());
        let (nx, nz) = (mth::clamp_f64(x, -2.9999999E7, 2.9999999E7), mth::clamp_f64(z, -2.9999999E7, 2.9999999E7));
        if nx != x || nz != z {
            let y = self.base().y();
            self.set_pos(nx, y, nz);
        }
        self.player.living.attack_strength_ticker += 1;
        self.player.living.item_swap_ticker += 1;
        self.player.cooldowns.tick();
        self.update_player_pose(level);
        self.flush_item_cooldown_packets();
        self.flush_pending_drops(level);
    }

    /// `Player.aiStep()`: `super.aiStep()` is the generic `LivingEntity.aiStep`.
    pub(super) fn player_ai_step(&mut self, level: &mut ServerLevel) {
        if self.player.jump_trigger_time > 0 {
            self.player.jump_trigger_time -= 1;
        }
        self.tick_regeneration(level);
        self.tick_inventory_items(level);
        if self.player.abilities.flying && !self.base().is_passenger() {
            self.reset_fall_distance();
        }
        living_entity::ai_step::ai_step(self, level);
        let y_rot = self.base().y_rot();
        self.player.living.y_head_rot = y_rot;
        let speed = self.player.living.get_attribute_value(mc_data::Attribute::MOVEMENT_SPEED) as f32;
        self.set_speed(speed);
        self.touch_nearby_entities(level);
    }

    /// `Player.aiStep` pickup part: `playerTouch` for every entity around the player (experience orbs: one random orb).
    fn touch_nearby_entities(&mut self, level: &mut ServerLevel) {
        use mc_core::random::RandomSource;
        if self.player.living.get_health() <= 0.0 || self.is_spectator() {
            return;
        }
        let bb = self.base().bounding_box();
        let area = match self.base().vehicle.and_then(|v| level.get_entity(v)) {
            Some(vehicle) if !vehicle.is_removed() => bb.minmax(&vehicle.get_bounding_box()).inflate(1.0, 0.0, 1.0),
            _ => bb.inflate(1.0, 0.5, 1.0),
        };
        let ids = level.get_entities(Some(self.base_id()), &area, |_| true);
        let mut orbs = Vec::new();
        for id in ids {
            if level.get_entity(id).is_some_and(|e| e.get_type() == mc_data::EntityType::EXPERIENCE_ORB) {
                orbs.push(id);
            } else {
                level.with_entity(id, |level, e| {
                    if !e.is_removed() {
                        e.player_touch(level, self);
                    }
                });
            }
        }
        if !orbs.is_empty() {
            let index = self.base_mut().random.next_int_bounded(orbs.len() as i32) as usize;
            level.with_entity(orbs[index], |level, e| e.player_touch(level, self));
        }
    }

    /// `Player.updatePlayerPose()`.
    fn update_player_pose(&mut self, level: &mut ServerLevel) {
        use mc_data::enums::Pose;
        if self.can_player_fit_within_blocks_and_entities_when(level, Pose::Swimming) {
            let desired_pose = self.get_desired_pose();
            let actual_pose = if self.is_spectator() || self.base().is_passenger() || self.can_player_fit_within_blocks_and_entities_when(level, desired_pose) {
                desired_pose
            } else if self.can_player_fit_within_blocks_and_entities_when(level, Pose::Crouching) {
                Pose::Crouching
            } else {
                Pose::Swimming
            };
            self.set_pose(actual_pose);
        }
    }

    /// `Player.getDesiredPose()`.
    fn get_desired_pose(&self) -> mc_data::enums::Pose {
        use mc_data::enums::Pose;
        if self.player.living.is_sleeping() {
            Pose::Sleeping
        } else if self.is_swimming() {
            Pose::Swimming
        } else if self.player.living.is_fall_flying() {
            Pose::FallFlying
        } else if self.player.living.get_living_entity_flag(crate::world::entity::living::LIVING_ENTITY_FLAG_SPIN_ATTACK) {
            Pose::SpinAttack
        } else if self.is_shift_key_down() && !self.player.abilities.flying {
            Pose::Crouching
        } else {
            Pose::Standing
        }
    }

    /// `Player.isSwimming()`.
    pub fn is_swimming(&self) -> bool {
        !self.player.abilities.flying && !self.is_spectator() && self.base().get_shared_flag(crate::world::entity::entity::FLAG_SWIMMING)
    }

    /// `canPlayerFitWithinBlocksAndEntitiesWhen(pose)`.
    fn can_player_fit_within_blocks_and_entities_when(&self, level: &ServerLevel, pose: mc_data::enums::Pose) -> bool {
        let aabb = self.get_dimensions(pose).make_bounding_box_vec(self.base().position()).deflate_scalar(1.0E-7);
        movement::collect_all_colliders(self, level, &aabb).iter().all(|shape| shape.is_empty())
    }

    /// `ServerPlayer.doTick()`.
    pub fn do_tick(&mut self, level: &mut ServerLevel) {
        if !self.is_spectator() || !movement::touching_unloaded_chunk(self, level) {
            self.player_tick(level);
            self.close_container_if_invalid(level);
            self.tick_food(level);
            self.award_stat_once(level, Stats::PLAY_TIME);
            self.award_stat_once(level, Stats::TOTAL_WORLD_TIME);
            if self.is_alive() {
                self.award_stat_once(level, Stats::TIME_SINCE_DEATH);
            }
            if self.is_shift_key_down() {
                self.award_stat_once(level, Stats::CROUCH_TIME);
            }
            if !self.player.living.is_sleeping() {
                self.award_stat_once(level, Stats::TIME_SINCE_REST);
            }
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
        let health_and_absorption = health + self.player.get_absorption_amount();
        if health_and_absorption != self.last_recorded_health_and_absorption {
            self.last_recorded_health_and_absorption = health_and_absorption;
            self.update_score_for_criteria(level, &ObjectiveCriteria::HEALTH, mth::ceil_f32(health_and_absorption));
        }
        if food != self.last_recorded_food_level {
            self.last_recorded_food_level = food;
            self.update_score_for_criteria(level, &ObjectiveCriteria::FOOD, mth::ceil_f32(food as f32));
        }
        let air = self.base().get_air_supply();
        if air != self.last_recorded_air_level {
            self.last_recorded_air_level = air;
            self.update_score_for_criteria(level, &ObjectiveCriteria::AIR, mth::ceil_f32(air as f32));
        }
        let armor = mth::floor_f64(self.player.living.get_attribute_value(mc_data::Attribute::ARMOR));
        if armor != self.last_recorded_armor {
            self.last_recorded_armor = armor;
            self.update_score_for_criteria(level, &ObjectiveCriteria::ARMOR, mth::ceil_f32(armor as f32));
        }
        let total_experience = self.player.total_experience;
        if total_experience != self.last_recorded_experience {
            self.last_recorded_experience = total_experience;
            self.update_score_for_criteria(level, &ObjectiveCriteria::EXPERIENCE, mth::ceil_f32(total_experience as f32));
        }
        let experience_level = self.player.experience_level;
        if experience_level != self.last_recorded_level {
            self.last_recorded_level = experience_level;
            self.update_score_for_criteria(level, &ObjectiveCriteria::LEVEL, mth::ceil_f32(experience_level as f32));
        }
        if total_experience != self.last_sent_exp {
            self.last_sent_exp = total_experience;
            self.send(&ClientboundSetExperiencePacket {
                experience_progress: self.player.experience_progress,
                experience_level,
                total_experience,
            });
        }
        // SEAM(advancements): `if (tickCount % 20 == 0) CriteriaTriggers.LOCATION.trigger(this)`
        // the player list's view of this player (tab list, broadcasts by range/dimension)
        self.sync_handle();
    }
}
