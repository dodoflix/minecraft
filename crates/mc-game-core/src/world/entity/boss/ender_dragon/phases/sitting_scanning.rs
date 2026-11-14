//! `DragonSittingScanningPhase`: sit and turn towards a player within twenty blocks; none for five seconds and the dragon takes
//! off, charging a player in sight.

use super::{EnderDragonPhase, charge_player};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::targeting::TargetingConditions;
use crate::world::entity::boss::ender_dragon::{EnderDragon, HEAD};
use crate::world::entity::mob::ext::get_nearest_player;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use std::sync::Arc;

/// `SITTING_SCANNING_IDLE_TICKS`.
const SITTING_SCANNING_IDLE_TICKS: i32 = 100;
/// `SITTING_ATTACK_Y_VIEW_RANGE`.
const SITTING_ATTACK_Y_VIEW_RANGE: f64 = 10.0;
/// `SITTING_ATTACK_VIEW_RANGE`.
const SITTING_ATTACK_VIEW_RANGE: f64 = 20.0;
/// `SITTING_CHARGE_VIEW_RANGE`.
const SITTING_CHARGE_VIEW_RANGE: f64 = 150.0;
/// `DEG_TO_RAD`.
const DEG_TO_RAD: f32 = 0.017453292;

#[derive(Default)]
pub struct State {
    /// `scanningTime`.
    pub scanning_time: i32,
}

/// `begin()`.
pub fn begin(dragon: &mut EnderDragon) {
    dragon.phases.sitting_scanning.scanning_time = 0;
}

/// `scanTargeting`: `forCombat().range(20.0).selector((target, level) -> Math.abs(target.getY() - dragon.getY()) <= 10.0)`.
fn scan_targeting() -> TargetingConditions {
    TargetingConditions::for_combat()
        .range(SITTING_ATTACK_VIEW_RANGE)
        .source_selector(Some(Arc::new(|dragon, target, _level| {
            (target.base().y() - dragon.base().y()).abs() <= SITTING_ATTACK_Y_VIEW_RANGE
        })))
}

/// `doServerTick(level)`.
pub fn do_server_tick(dragon: &mut EnderDragon, level: &mut ServerLevel) {
    dragon.phases.sitting_scanning.scanning_time += 1;
    let pos = dragon.base().position();
    let attack_target = get_nearest_player(level, &scan_targeting(), &*dragon, pos.x, pos.y, pos.z);
    if let Some(attack_target) = attack_target {
        if dragon.phases.sitting_scanning.scanning_time > 25 {
            dragon.set_phase(level, EnderDragonPhase::SittingAttacking);
            return;
        }
        let Some(target) = level.get_entity(attack_target) else {
            return;
        };
        let (target_x, target_z) = (target.base().x(), target.base().z());
        let aim = Vec3::new(target_x - pos.x, 0.0, target_z - pos.z).normalize();
        let y_rot = dragon.base().y_rot();
        let dir = Vec3::new(
            f64::from(mth::sin(f64::from(y_rot * DEG_TO_RAD))),
            0.0,
            f64::from(-mth::cos(f64::from(y_rot * DEG_TO_RAD))),
        )
        .normalize();
        let dot = dir.dot(aim) as f32;
        let angle = (mth::java_acos(f64::from(dot)) * 57.2957763671875) as f32 + 0.5;
        if angle < 0.0 || angle > 10.0 {
            let head = level
                .dragon_part(dragon.part_ids()[HEAD])
                .map_or(pos, |head| head.base().position());
            let x_attack_dist = target_x - head.x;
            let z_attack_dist = target_z - head.z;
            // `Mth.atan2(..) * 180.0F / (float)Math.PI` compiles to one multiplication by 57.2957763671875
            let y_rot_delta = mth::clamp_f64(
                mth::wrap_degrees_f64(
                    180.0
                        - mth::atan2(x_attack_dist, z_attack_dist) * 57.2957763671875
                        - f64::from(y_rot),
                ),
                -100.0,
                100.0,
            );
            dragon.y_rot_a *= 0.8;
            let mut dist =
                (x_attack_dist * x_attack_dist + z_attack_dist * z_attack_dist).sqrt() as f32 + 1.0;
            let rot_speed = dist;
            if dist > 40.0 {
                dist = 40.0;
            }
            dragon.y_rot_a += y_rot_delta as f32 * (0.7 / dist / rot_speed);
            let new_y_rot = dragon.base().y_rot() + dragon.y_rot_a;
            dragon.base_mut().set_y_rot(new_y_rot);
        }
    } else if dragon.phases.sitting_scanning.scanning_time >= SITTING_SCANNING_IDLE_TICKS {
        let charge_targeting = TargetingConditions::for_combat().range(SITTING_CHARGE_VIEW_RANGE);
        let attack_target =
            get_nearest_player(level, &charge_targeting, &*dragon, pos.x, pos.y, pos.z);
        dragon.set_phase(level, EnderDragonPhase::Takeoff);
        if let Some(attack_target) = attack_target {
            dragon.set_phase(level, EnderDragonPhase::ChargingPlayer);
            if let Some(target) = level.get_entity(attack_target) {
                let target_pos = target.base().position();
                charge_player::set_target(dragon, target_pos);
            }
        }
    }
}
