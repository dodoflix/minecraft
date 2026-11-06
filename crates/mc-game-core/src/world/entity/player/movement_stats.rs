//! `ServerPlayer.checkMovementStatistics` / `checkRidingStatistics` and the `jumpFromGround` override: distance
//! statistics and the food exhaustion of moving.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::stats::stats::Stats;
use crate::world::entity::Entity;
use mc_core::math::java;

/// `didNotMove(dx, dy, dz)`.
fn did_not_move(dx: f64, dy: f64, dz: f64) -> bool {
    dx == 0.0 && dy == 0.0 && dz == 0.0
}

/// `Math.round((float) Math.sqrt(..) * 100.0F)`.
fn distance_cm(squared: f64) -> i32 {
    java::round_f32(squared.sqrt() as f32 * 100.0)
}

impl ServerPlayer {
    /// `checkMovementStatistics(dx, dy, dz)`: swim/walk/sprint/climb/fly/aviate distance stats and the exhaustion of
    /// swimming and sprinting.
    pub fn check_movement_statistics(&mut self, level: &ServerLevel, dx: f64, dy: f64, dz: f64) {
        if self.base().is_passenger() || did_not_move(dx, dy, dz) {
            return;
        }
        if self.is_swimming() {
            let distance = distance_cm(dx * dx + dy * dy + dz * dz);
            if distance > 0 {
                self.award_stat(level, Stats::SWIM_ONE_CM, distance);
                self.cause_food_exhaustion(0.01 * distance as f32 * 0.01);
            }
        } else if self.is_eye_in_fluid("water") {
            let distance = distance_cm(dx * dx + dy * dy + dz * dz);
            if distance > 0 {
                self.award_stat(level, Stats::WALK_UNDER_WATER_ONE_CM, distance);
                self.cause_food_exhaustion(0.01 * distance as f32 * 0.01);
            }
        } else if self.is_in_water() {
            let horizontal = distance_cm(dx * dx + dz * dz);
            if horizontal > 0 {
                self.award_stat(level, Stats::WALK_ON_WATER_ONE_CM, horizontal);
                self.cause_food_exhaustion(0.01 * horizontal as f32 * 0.01);
            }
        } else if self.on_climbable(level) {
            if dy > 0.0 {
                self.award_stat(
                    level,
                    Stats::CLIMB_ONE_CM,
                    java::round_f64(dy * 100.0) as i32,
                );
            }
        } else if self.base().on_ground() {
            let horizontal = distance_cm(dx * dx + dz * dz);
            if horizontal > 0 {
                if self.is_sprinting() {
                    self.award_stat(level, Stats::SPRINT_ONE_CM, horizontal);
                    self.cause_food_exhaustion(0.1 * horizontal as f32 * 0.01);
                } else if self.is_crouching() {
                    self.award_stat(level, Stats::CROUCH_ONE_CM, horizontal);
                    self.cause_food_exhaustion(0.0 * horizontal as f32 * 0.01);
                } else {
                    self.award_stat(level, Stats::WALK_ONE_CM, horizontal);
                    self.cause_food_exhaustion(0.0 * horizontal as f32 * 0.01);
                }
            }
        } else if self.player.living.is_fall_flying() {
            let distance = distance_cm(dx * dx + dy * dy + dz * dz);
            self.award_stat(level, Stats::AVIATE_ONE_CM, distance);
        } else {
            let horizontal = distance_cm(dx * dx + dz * dz);
            if horizontal > 25 {
                self.award_stat(level, Stats::FLY_ONE_CM, horizontal);
            }
        }
    }

    /// `checkRidingStatistics(dx, dy, dz)` for a passenger: the distance stat of the vehicle's kind. Vehicle classes
    /// without an entity type in the registry yet are matched by their `EntityType` (minecart/boat/pig/horse/strider/
    /// happy ghast/nautilus families).
    pub fn check_riding_statistics(&mut self, level: &ServerLevel, dx: f64, dy: f64, dz: f64) {
        if !self.base().is_passenger() || did_not_move(dx, dy, dz) {
            return;
        }
        let distance = distance_cm(dx * dx + dy * dy + dz * dz);
        let Some(vehicle) = self.base().vehicle.and_then(|v| level.get_entity(v)) else {
            return;
        };
        let name = vehicle.get_type().name();
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        let stat = if path.ends_with("minecart") {
            Stats::MINECART_ONE_CM
        } else if path.ends_with("boat") || path.ends_with("raft") {
            Stats::BOAT_ONE_CM
        } else if path == "pig" {
            Stats::PIG_ONE_CM
        } else if matches!(
            path,
            "horse"
                | "donkey"
                | "mule"
                | "skeleton_horse"
                | "zombie_horse"
                | "camel"
                | "camel_husk"
                | "llama"
                | "trader_llama"
        ) {
            Stats::HORSE_ONE_CM
        } else if path == "strider" {
            Stats::STRIDER_ONE_CM
        } else if path == "happy_ghast" {
            Stats::HAPPY_GHAST_ONE_CM
        } else if matches!(path, "nautilus" | "zombie_nautilus") {
            Stats::NAUTILUS_ONE_CM
        } else {
            return;
        };
        self.award_stat(level, stat, distance);
    }

    /// `ServerPlayer.jumpFromGround()` after `LivingEntity.jumpFromGround`: the jump stat and the exhaustion.
    pub(super) fn after_jump_from_ground(&mut self, level: &ServerLevel) {
        self.award_stat(level, Stats::JUMP, 1);
        if self.is_sprinting() {
            self.cause_food_exhaustion(0.2);
        } else {
            self.cause_food_exhaustion(0.05);
        }
    }
}
