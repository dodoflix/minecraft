//! `net.minecraft.world.entity.projectile.ShulkerBullet`: the homing bullet of the shulker (levitation on hit).

use super::{EntityHitResult, HitResult, Projectile, ProjectileData, ProjectileDeflection};
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::{Entity, EntityData, EntityId, entity};
use crate::world::item::enchantment::actors::Actors;
use crate::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, MobEffect, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::{ParticleOptions, SoundSource};
use mc_world::level_storage::Difficulty;
use std::any::Any;
use uuid::Uuid;

const SPEED: f64 = 0.15;

/// `ShulkerBullet`.
pub struct ShulkerBullet {
    base: EntityData,
    projectile: ProjectileData,
    /// `finalTarget` (`EntityReference<Entity>`).
    final_target: Option<Uuid>,
    current_move_direction: Option<Direction>,
    flight_steps: i32,
    target_delta_x: f64,
    target_delta_y: f64,
    target_delta_z: f64,
}

impl ShulkerBullet {
    /// `ShulkerBullet(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mut base = EntityData::new(ty, id, entity::TOTAL_AIR_SUPPLY, |_| {});
        base.no_physics = true;
        Self {
            base,
            projectile: ProjectileData::default(),
            final_target: None,
            current_move_direction: None,
            flight_steps: 0,
            target_delta_x: 0.0,
            target_delta_y: 0.0,
            target_delta_z: 0.0,
        }
    }

    /// `ShulkerBullet(level, owner, target, invalidStartAxis)`.
    pub fn from_owner(level: &ServerLevel, owner: &dyn Entity, target: &dyn Entity, invalid_start_axis: Axis) -> Self {
        let mut bullet = Self::new(EntityType::SHULKER_BULLET, level.next_entity_id());
        bullet.set_owner(Some(owner));
        let position = owner.get_bounding_box().get_center();
        let (y_rot, x_rot) = (bullet.base.y_rot(), bullet.base.x_rot());
        bullet.snap_to(position.x, position.y, position.z, y_rot, x_rot);
        bullet.final_target = Some(target.uuid());
        bullet.current_move_direction = Some(Direction::Up);
        bullet.select_next_move_direction(level, Some(invalid_start_axis), Some(target));
        bullet
    }

    /// `selectNextMoveDirection(avoidAxis, target)`.
    fn select_next_move_direction(&mut self, level: &ServerLevel, avoid_axis: Option<Axis>, target: Option<&dyn Entity>) {
        let mut y_offset = 0.5;
        let target_pos = match target {
            None => self.block_position().below(),
            Some(t) => {
                y_offset = f64::from(t.base().get_bb_height()) * 0.5;
                BlockPos::containing(t.base().x(), t.base().y() + y_offset, t.base().z())
            }
        };
        let mut target_x = f64::from(target_pos.x()) + 0.5;
        let mut target_y = f64::from(target_pos.y()) + y_offset;
        let mut target_z = f64::from(target_pos.z()) + 0.5;
        let mut selection = None;
        if !target_pos.closer_to_center_than(&self.position(), 2.0) {
            let current = self.block_position();
            let mut options = Vec::with_capacity(3);
            if avoid_axis != Some(Axis::X) {
                if current.x() < target_pos.x() && level.is_empty_block(current.east()) {
                    options.push(Direction::East);
                } else if current.x() > target_pos.x() && level.is_empty_block(current.west()) {
                    options.push(Direction::West);
                }
            }
            if avoid_axis != Some(Axis::Y) {
                if current.y() < target_pos.y() && level.is_empty_block(current.above()) {
                    options.push(Direction::Up);
                } else if current.y() > target_pos.y() && level.is_empty_block(current.below()) {
                    options.push(Direction::Down);
                }
            }
            if avoid_axis != Some(Axis::Z) {
                if current.z() < target_pos.z() && level.is_empty_block(current.south()) {
                    options.push(Direction::South);
                } else if current.z() > target_pos.z() && level.is_empty_block(current.north()) {
                    options.push(Direction::North);
                }
            }
            let mut dir = Direction::get_random(&mut self.base.random);
            if options.is_empty() {
                let mut attempts = 5;
                while !level.is_empty_block(current.relative(dir)) && attempts > 0 {
                    dir = Direction::get_random(&mut self.base.random);
                    attempts -= 1;
                }
            } else {
                dir = options[self.base.random.next_int_bounded(options.len() as i32) as usize];
            }
            selection = Some(dir);
            target_x = self.base.x() + f64::from(dir.get_step_x());
            target_y = self.base.y() + f64::from(dir.get_step_y());
            target_z = self.base.z() + f64::from(dir.get_step_z());
        }
        self.current_move_direction = selection;
        let xa = target_x - self.base.x();
        let ya = target_y - self.base.y();
        let za = target_z - self.base.z();
        let distance = (xa * xa + ya * ya + za * za).sqrt();
        if distance == 0.0 {
            self.target_delta_x = 0.0;
            self.target_delta_y = 0.0;
            self.target_delta_z = 0.0;
        } else {
            self.target_delta_x = xa / distance * SPEED;
            self.target_delta_y = ya / distance * SPEED;
            self.target_delta_z = za / distance * SPEED;
        }
        self.base.needs_sync = true;
        self.flight_steps = 10 + self.base.random.next_int_bounded(5) * 10;
    }

    /// `destroy()`.
    fn destroy(&mut self, level: &mut ServerLevel) {
        self.discard();
        self.game_event(level, GameEvent::ENTITY_DAMAGE);
    }
}

impl Projectile for ShulkerBullet {
    fn projectile_data(&self) -> &ProjectileData {
        &self.projectile
    }

    fn projectile_data_mut(&mut self) -> &mut ProjectileData {
        &mut self.projectile
    }

    fn can_hit_entity(&self, level: &ServerLevel, entity: &dyn Entity) -> bool {
        super::hurting::default_can_hit_entity(self, level, entity) && !entity.base().no_physics
    }

    fn on_hit_entity(&mut self, level: &mut ServerLevel, hit: &EntityHitResult) {
        let victim = hit.entity;
        let owner_id = self.get_owner(level).map(|o| o.id());
        let owner_living_id = owner_id.filter(|id| level.get_entity(*id).is_some_and(|o| o.living().is_some()));
        let source = level.damage_sources().mob_projectile(self.as_dyn(), owner_living_id.and_then(|id| level.get_entity(id)));
        let effect_source = owner_id.unwrap_or(self.id());
        level.with_entity(victim, |level, target| {
            if target.hurt_server(level, &source, 4.0) {
                crate::world::item::enchantment_helper::do_post_attack_effects(level, &mut Actors::of(&mut *target), victim, &source);
                if target.living().is_some() {
                    target.add_effect(level, MobEffectInstance::of(MobEffect::LEVITATION, 200, 0), Some(effect_source));
                }
            }
        });
    }

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        super::on_hit_block_base(self, level, hit);
        let p = self.position();
        level.send_particles(ParticleOptions::Simple(ParticleType::EXPLOSION), p.x, p.y, p.z, 2, 0.2, 0.2, 0.2, 0.0);
        self.play_sound(level, SoundEvent::ENTITY_SHULKER_BULLET_HIT, 1.0, 1.0);
    }

    fn on_redirect_projectile(&mut self, level: &mut ServerLevel, hit_projectile: EntityId) {
        let (self_id, owner) = (self.id(), self.projectile.owner);
        level.with_entity(hit_projectile, |level, e| {
            if let Some(p) = e.as_projectile_mut() {
                p.deflect(level, ProjectileDeflection::MomentumDeflect, Some(self_id), owner, true, Vec3::new(1.0, 1.0, 1.0));
            }
        });
    }

    fn on_hit(&mut self, level: &mut ServerLevel, hit: &HitResult) {
        super::on_hit_base(self, level, hit);
        self.destroy(level);
    }
}

impl Entity for ShulkerBullet {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    crate::impl_projectile_entity!(@base);

    fn on_above_bubble_column(&mut self, level: &mut ServerLevel, drag_down: bool, pos: BlockPos) {
        super::on_above_bubble_column(self, level, drag_down, pos);
    }

    fn on_inside_bubble_column(&mut self, _level: &mut ServerLevel, drag_down: bool) {
        super::on_inside_bubble_column(self, drag_down);
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn check_despawn(&mut self, level: &mut ServerLevel) {
        if level.difficulty() == Difficulty::Peaceful {
            self.discard();
        }
    }

    fn get_default_gravity(&self) -> f64 {
        0.04
    }

    fn is_affected_by_blocks(&self) -> bool {
        !self.is_removed()
    }

    fn is_on_fire(&self) -> bool {
        false
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, _source: &DamageSource, _amount: f32) -> bool {
        self.play_sound(level, SoundEvent::ENTITY_SHULKER_BULLET_HURT, 1.0, 1.0);
        let p = self.position();
        level.send_particles(ParticleOptions::Simple(ParticleType::CRIT), p.x, p.y, p.z, 15, 0.2, 0.2, 0.2, 0.0);
        self.destroy(level);
        true
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
        let final_target_id = self.final_target.and_then(|u| level.get_entity_by_uuid(u)).map(|e| e.id());
        if final_target_id.is_none() {
            self.final_target = None;
        }
        let target_alive = final_target_id.and_then(|id| level.get_entity(id)).is_some_and(|t| t.is_alive() && !(t.is_player() && t.is_spectator()));
        if !target_alive {
            self.apply_gravity();
        } else {
            self.target_delta_x = mth::clamp_f64(self.target_delta_x * 1.025, -1.0, 1.0);
            self.target_delta_y = mth::clamp_f64(self.target_delta_y * 1.025, -1.0, 1.0);
            self.target_delta_z = mth::clamp_f64(self.target_delta_z * 1.025, -1.0, 1.0);
            let movement = self.base.delta_movement();
            self.base.set_delta_movement(movement.add_xyz(
                (self.target_delta_x - movement.x) * 0.2,
                (self.target_delta_y - movement.y) * 0.2,
                (self.target_delta_z - movement.z) * 0.2,
            ));
        }
        let hit_result = super::util::get_hit_result_on_move_vector(level, self, &|e| self.can_hit_entity(level, e));
        let movement = self.base.delta_movement();
        self.set_pos_vec(self.position().add(movement));
        self.apply_effects_from_blocks(level);
        if self.base.portal_process.as_ref().is_some_and(|p| p.is_inside_portal_this_tick()) {
            self.handle_portal(level);
        }
        if self.is_alive() && hit_result.get_type() != super::HitType::Miss {
            super::hit_target_or_deflect_self(self, level, &hit_result);
        }
        super::util::rotate_towards_movement(self, 0.5);
        if let Some(target_id) = final_target_id {
            if self.flight_steps > 0 {
                self.flight_steps -= 1;
                if self.flight_steps == 0 {
                    let axis = self.current_move_direction.map(|d| d.get_axis());
                    if let Some(t) = level.get_entity(target_id) {
                        self.select_next_move_direction(level, axis, Some(t));
                    }
                }
            }
            if let Some(dir) = self.current_move_direction {
                let current = self.block_position();
                let axis = dir.get_axis();
                let rel = current.relative(dir);
                let can_stand = level.is_in_valid_bounds(rel)
                    && level.has_chunk(rel.x() >> 4, rel.z() >> 4)
                    && crate::world::entity::ai::navigation::entity_can_stand_on(level.get_block_state(rel), &*level, rel, self);
                if can_stand {
                    if let Some(t) = level.get_entity(target_id) {
                        self.select_next_move_direction(level, Some(axis), Some(t));
                    }
                } else if let Some(t) = level.get_entity(target_id) {
                    let target_pos = t.block_position();
                    if (axis == Axis::X && current.x() == target_pos.x()) || (axis == Axis::Z && current.z() == target_pos.z()) || (axis == Axis::Y && current.y() == target_pos.y()) {
                        self.select_next_move_direction(level, Some(axis), Some(t));
                    }
                }
            }
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        if let Some(target) = self.final_target {
            out.put_int_array("Target", mc_core::uuid_util::uuid_to_int_array(&target).to_vec());
        }
        if let Some(dir) = self.current_move_direction {
            out.put_int("Dir", dir.get_3d_data_value() as i32);
        }
        out.put_int("Steps", self.flight_steps);
        out.put_double("TXD", self.target_delta_x);
        out.put_double("TYD", self.target_delta_y);
        out.put_double("TZD", self.target_delta_z);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.flight_steps = input.get_int_or("Steps", 0);
        self.target_delta_x = input.get_double_or("TXD", 0.0);
        self.target_delta_y = input.get_double_or("TYD", 0.0);
        self.target_delta_z = input.get_double_or("TZD", 0.0);
        self.current_move_direction = input.get("Dir").and_then(|t| t.as_number()).map(|n| Direction::from_3d_data_value(n.int_value()));
        self.final_target = entity::read_uuid(input.get("Target"));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SHULKER_BULLET_FACTORY: EntityRegistration = EntityRegistration { entity_type: EntityType::SHULKER_BULLET, create: |ty, id, _registries| Box::new(ShulkerBullet::new(ty, id)) };
