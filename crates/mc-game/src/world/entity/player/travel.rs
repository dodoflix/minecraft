//! The server-side `LivingEntity.aiStep` jump/travel section for a player: the server simulates every player each
//! tick with zero input (gravity, drag, collisions); the packet handler then snaps the player back to the position
//! the client reported. This is what keeps `onGround`/`verticalCollisionBelow` current on the server.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, MoverType, movement};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::{Attribute, MobEffect};

fn compute_modified_friction(friction: f32, modifier: f32) -> f32 {
    mth::clamp_f32(1.0 - (1.0 - friction) * modifier, 0.0, 1.0)
}

impl ServerPlayer {
    fn has_effect(&self, effect: MobEffect) -> bool {
        self.player.living.active_effects.contains_key(&effect)
    }

    /// `LivingEntity.getEffectiveGravity()`.
    pub fn living_effective_gravity(&self) -> f64 {
        let falling = self.base().delta_movement().y <= 0.0;
        if falling && self.has_effect(MobEffect::SLOW_FALLING) { self.get_gravity().min(0.01) } else { self.get_gravity() }
    }

    fn on_climbable(&self, level: &ServerLevel) -> bool {
        use crate::world::level::BlockGetter;
        if self.is_spectator() {
            return false;
        }
        level.get_block_state(self.base().block_position()).block().is_in_tag("climbable")
    }

    /// `aiStep` from `noJumpDelay` to `travel` with zero input (the AI/jump-input parts do not apply to players on
    /// the server: `jumping` stays false, `xxa/yya/zza` are zero).
    pub fn ai_step_travel(&mut self, level: &mut ServerLevel) {
        let l = &mut self.player.living;
        if l.no_jump_delay > 0 {
            l.no_jump_delay -= 1;
        }
        let m = self.base().delta_movement();
        let (mut dx, mut dy, mut dz) = (m.x, m.y, m.z);
        if m.horizontal_distance_sqr() < 9.0E-6 {
            dx = 0.0;
            dz = 0.0;
        }
        if m.y.abs() < 0.003 {
            dy = 0.0;
        }
        self.base_mut().set_delta_movement_xyz(dx, dy, dz);
        // `jumping` is false: `noJumpDelay = 0`
        self.player.living.no_jump_delay = 0;
        if self.has_effect(MobEffect::SLOW_FALLING) || self.has_effect(MobEffect::LEVITATION) {
            self.reset_fall_distance();
        }
        // `canSimulateMovement() && isEffectiveAi()`: a ServerPlayer simulates itself (AuthoritativeSideAndServer)
        self.travel(level);
        // applyEffectsFromBlocks: block `entityInside` effects are not dispatched yet
    }

    /// `Player.travel(Vec3.ZERO)` -> `LivingEntity.travel`.
    fn travel(&mut self, level: &mut ServerLevel) {
        if self.base().is_passenger() {
            return;
        }
        let flying = self.player.abilities.flying;
        if flying {
            let original_y = self.base().delta_movement().y;
            self.travel_base(level);
            let dm = self.base().delta_movement();
            self.base_mut().set_delta_movement(dm.with(mc_core::direction::Axis::Y, original_y * 0.6));
        } else {
            self.travel_base(level);
        }
    }

    fn travel_base(&mut self, level: &mut ServerLevel) {
        use crate::world::level::BlockGetter;
        let in_liquid = self.base().was_touching_water || (!self.base().first_tick && self.base().fluid_height[1] > 0.0);
        if in_liquid && !self.player.abilities.flying {
            self.travel_in_fluid(level);
        } else {
            let _ = level.get_fluid_state(self.base().block_position());
            self.travel_in_air(level);
        }
    }

    fn travel_in_air(&mut self, level: &mut ServerLevel) {
        use crate::world::level::BlockGetter;
        let pos_below = movement::get_block_pos_below_that_affects_my_movement(self, level);
        let friction_modifier = self.player.living.get_attribute_value(Attribute::FRICTION_MODIFIER) as f32;
        let block_friction = if self.base().on_ground() {
            compute_modified_friction(level.get_block_state(pos_below).block().data().friction, friction_modifier)
        } else {
            1.0
        };
        // handleRelativeFrictionAndCalculateMovement(ZERO)
        let mut dm = self.base().delta_movement();
        if self.on_climbable(level) {
            self.reset_fall_distance();
            let xd = mth::clamp_f64(dm.x, -f64::from(0.15f32), f64::from(0.15f32));
            let zd = mth::clamp_f64(dm.z, -f64::from(0.15f32), f64::from(0.15f32));
            let mut yd = dm.y.max(-f64::from(0.15f32));
            if yd < 0.0 && !level.get_block_state(self.base().block_position()).block().data().name.ends_with("scaffolding") && self.is_shift_key_down() {
                yd = 0.0;
            }
            dm = Vec3::new(xd, yd, zd);
            self.base_mut().set_delta_movement(dm);
        }
        let dm = self.base().delta_movement();
        let _ = self.r#move(level, MoverType::SelfMover, dm);
        let mut movement = self.base().delta_movement();
        if self.base().horizontal_collision && self.on_climbable(level) {
            movement = Vec3::new(movement.x, 0.2, movement.z);
        }
        let mut movement_y = movement.y;
        if let Some(e) = self.player.living.active_effects.get(&MobEffect::LEVITATION) {
            movement_y += (0.05 * f64::from(e.amplifier + 1) - movement.y) * 0.2;
        } else {
            movement_y -= self.living_effective_gravity();
        }
        let air_drag_mod = self.player.living.get_attribute_value(Attribute::AIR_DRAG_MODIFIER) as f32;
        let air_drag = compute_modified_friction(0.91, air_drag_mod);
        let friction = block_friction * air_drag;
        let vertical_friction = compute_modified_friction(0.98, air_drag_mod);
        self.base_mut().set_delta_movement_xyz(movement.x * f64::from(friction), movement_y * f64::from(vertical_friction), movement.z * f64::from(friction));
    }

    fn fluid_falling_adjusted_movement(&self, base_gravity: f64, is_falling: bool, movement: Vec3) -> Vec3 {
        if base_gravity != 0.0 && !self.base().get_shared_flag(crate::world::entity::entity::FLAG_SPRINTING) {
            let yd = if is_falling && (movement.y - 0.005).abs() >= 0.003 && (movement.y - base_gravity / 16.0).abs() < 0.003 { -0.003 } else { movement.y - base_gravity / 16.0 };
            Vec3::new(movement.x, yd, movement.z)
        } else {
            movement
        }
    }

    fn jump_out_of_fluid(&mut self, level: &ServerLevel, old_y: f64) {
        let m = self.base().delta_movement();
        let y = self.base().y();
        let area = self.base().bounding_box().move_(m.x, m.y + f64::from(0.6f32) - y + old_y, m.z);
        let free = crate::world::level::collision::CollisionGetter::no_collision(level, Some(self.base_id()), Some(&movement::collision_context(self)), &area) && !contains_any_liquid(level, &area);
        if self.base().horizontal_collision && free {
            self.base_mut().set_delta_movement_xyz(m.x, f64::from(0.3f32), m.z);
        }
    }

    fn travel_in_fluid(&mut self, level: &mut ServerLevel) {
        let is_falling = self.base().delta_movement().y <= 0.0;
        let old_y = self.base().y();
        let base_gravity = self.living_effective_gravity();
        let dm = self.base().delta_movement();
        let _ = self.r#move(level, MoverType::SelfMover, dm);
        if self.base().was_touching_water {
            let sprint = self.base().get_shared_flag(crate::world::entity::entity::FLAG_SPRINTING);
            let mut slow_down = if sprint { 0.9 } else { 0.8 };
            let mut water_walker = self.player.living.get_attribute_value(Attribute::WATER_MOVEMENT_EFFICIENCY) as f32;
            if !self.base().on_ground() {
                water_walker *= 0.5;
            }
            if water_walker > 0.0 {
                slow_down += (0.54600006 - slow_down) * water_walker;
            }
            if self.has_effect(MobEffect::DOLPHINS_GRACE) {
                slow_down = 0.96;
            }
            let mut movement = self.base().delta_movement();
            if self.base().horizontal_collision && self.on_climbable(level) {
                movement = Vec3::new(movement.x, 0.2, movement.z);
            }
            movement = movement.multiply_xyz(f64::from(slow_down), f64::from(0.8f32), f64::from(slow_down));
            let adjusted = self.fluid_falling_adjusted_movement(base_gravity, is_falling, movement);
            self.base_mut().set_delta_movement(adjusted);
        } else {
            let shallow = self.base().fluid_height[1] <= self.base().get_fluid_jump_threshold();
            if shallow {
                let m = self.base().delta_movement().multiply_xyz(0.5, f64::from(0.8f32), 0.5);
                let adjusted = self.fluid_falling_adjusted_movement(base_gravity, is_falling, m);
                self.base_mut().set_delta_movement(adjusted);
            } else {
                let m = self.base().delta_movement().scale(0.5);
                self.base_mut().set_delta_movement(m);
            }
            if base_gravity != 0.0 {
                let m = self.base().delta_movement();
                self.base_mut().set_delta_movement(m.add(Vec3::new(0.0, -base_gravity / 4.0, 0.0)));
            }
        }
        self.jump_out_of_fluid(level, old_y);
    }
}

/// `containsAnyLiquid(box)`.
fn contains_any_liquid(level: &ServerLevel, area: &mc_core::phys::AABB) -> bool {
    use crate::world::level::BlockGetter;
    let (x0, y0, z0) = (mth::floor_f64(area.min_x), mth::floor_f64(area.min_y), mth::floor_f64(area.min_z));
    let (x1, y1, z1) = (mth::ceil_f64(area.max_x), mth::ceil_f64(area.max_y), mth::ceil_f64(area.max_z));
    for x in x0..x1 {
        for y in y0..y1 {
            for z in z0..z1 {
                if !level.get_fluid_state(mc_core::pos::BlockPos::new(x, y, z)).is_empty() {
                    return true;
                }
            }
        }
    }
    false
}
