//! `net.minecraft.world.entity.projectile.FireworkRocketEntity`.

use super::{EntityHitResult, HitResult, HitType, Projectile, ProjectileData, hit_target_or_deflect_self, update_rotation, util};
use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::entity::entity::{TOTAL_AIR_SUPPLY, calculate_view_vector};
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::inside_block_effects::NoopApplier;
use crate::world::entity::movement::collision_context;
use crate::world::entity::{Entity, EntityData, EntityId, MoverType};
use crate::world::item::{ItemStack, load_item_stack};
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::{BlockGetter, LevelAccessor};
use crate::{impl_projectile_entity, impl_throwable_projectile};
use linkme::distributed_slice;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::random::RandomSource;
use mc_data::{EntityType, GameEvent, Item, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{FIREWORKS, FireworkExplosion};
use mc_protocol::packets::common::HumanoidArm;
use mc_protocol::packets::play::SoundSource;

define_accessor!(pub DATA_ID_FIREWORKS_ITEM: ItemStack = FireworkRocketEntity);
define_accessor!(pub DATA_ATTACHED_TO_TARGET: OptionalUnsignedInt = FireworkRocketEntity);
define_accessor!(pub DATA_SHOT_AT_ANGLE: Boolean = FireworkRocketEntity);

pub struct FireworkRocketEntity {
    base: EntityData,
    projectile: ProjectileData,
    life: i32,
    lifetime: i32,
    /// `attachedToEntity` (a living entity of the level).
    attached_to_entity: Option<EntityId>,
}

impl FireworkRocketEntity {
    /// `FireworkRocketEntity(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let base = EntityData::new(ty, id, TOTAL_AIR_SUPPLY, |b| {
            b.define(DATA_ID_FIREWORKS_ITEM, ItemStack::new(Item::FIREWORK_ROCKET));
            b.define(DATA_ATTACHED_TO_TARGET, None);
            b.define(DATA_SHOT_AT_ANGLE, false);
        });
        Self { base, projectile: ProjectileData::default(), life: 0, lifetime: 0, attached_to_entity: None }
    }

    /// `FireworkRocketEntity(level, x, y, z, sourceItemStack)`.
    pub fn at(level: &ServerLevel, x: f64, y: f64, z: f64, source_item_stack: &ItemStack) -> Self {
        let mut r = Self::new(EntityType::FIREWORK_ROCKET, level.next_entity_id());
        r.life = 0;
        r.set_pos(x, y, z);
        r.base.entity_data.set(DATA_ID_FIREWORKS_ITEM, source_item_stack.copy());
        let mut flight_count = 1;
        if let Some(fireworks) = source_item_stack.get(&FIREWORKS) {
            flight_count += fireworks.flight_duration;
        }
        let dx = r.base.random.triangle(0.0, 0.002297);
        let dz = r.base.random.triangle(0.0, 0.002297);
        r.base.set_delta_movement_xyz(dx, 0.05, dz);
        r.lifetime = 10 * flight_count + r.base.random.next_int_bounded(6) + r.base.random.next_int_bounded(7);
        r
    }

    /// `FireworkRocketEntity(level, owner, x, y, z, sourceItemStack)`.
    pub fn with_owner_at(level: &ServerLevel, owner: Option<&dyn Entity>, x: f64, y: f64, z: f64, source_item_stack: &ItemStack) -> Self {
        let mut r = Self::at(level, x, y, z, source_item_stack);
        r.set_owner(owner);
        r
    }

    /// `FireworkRocketEntity(level, sourceItemStack, stuckTo)`.
    pub fn stuck_to(level: &ServerLevel, source_item_stack: &ItemStack, stuck_to: &dyn Entity) -> Self {
        let p = stuck_to.base().position();
        let mut r = Self::with_owner_at(level, Some(stuck_to), p.x, p.y, p.z, source_item_stack);
        r.base.entity_data.set(DATA_ATTACHED_TO_TARGET, Some(stuck_to.id()));
        r.attached_to_entity = Some(stuck_to.id());
        r
    }

    /// `FireworkRocketEntity(level, sourceItemStack, x, y, z, shotAtAngle)`.
    pub fn shot_at_angle(level: &ServerLevel, source_item_stack: &ItemStack, x: f64, y: f64, z: f64, shot_at_angle: bool) -> Self {
        let mut r = Self::at(level, x, y, z, source_item_stack);
        r.base.entity_data.set(DATA_SHOT_AT_ANGLE, shot_at_angle);
        r
    }

    /// `FireworkRocketEntity(level, sourceItemStack, owner, x, y, z, shotAtAngle)`: what crossbows shoot.
    pub fn new_shot_by(level: &mut ServerLevel, source_item_stack: &ItemStack, owner: &dyn Entity, x: f64, y: f64, z: f64, shot_at_angle: bool) -> Box<dyn Entity> {
        let mut r = Self::shot_at_angle(level, source_item_stack, x, y, z, shot_at_angle);
        r.set_owner(Some(owner));
        Box::new(r)
    }

    fn is_attached_to_entity(&self) -> bool {
        self.base.entity_data.get(DATA_ATTACHED_TO_TARGET).is_some()
    }

    pub fn is_shot_at_angle(&self) -> bool {
        self.base.entity_data.get(DATA_SHOT_AT_ANGLE)
    }

    /// `getItem()`.
    pub fn get_item(&self) -> ItemStack {
        self.base.entity_data.get(DATA_ID_FIREWORKS_ITEM)
    }

    /// `getExplosions()`.
    fn get_explosions(&self) -> Vec<FireworkExplosion> {
        self.get_item().get(&FIREWORKS).map(|f| f.explosions.clone()).unwrap_or_default()
    }

    /// `explode(level)`.
    fn explode(&mut self, level: &mut ServerLevel) {
        level.broadcast_entity_event(self.base.id, 17);
        let owner = self.get_owner(level).map(|o| o.id());
        level.game_event_from_entity(owner, GameEvent::EXPLODE, self.base.position());
        self.deal_explosion_damage(level);
        self.discard();
    }

    /// `dealExplosionDamage(level)`.
    fn deal_explosion_damage(&self, level: &mut ServerLevel) {
        let explosions = self.get_explosions();
        let damage_amount = if explosions.is_empty() { 0.0f32 } else { 5.0 + (explosions.len() * 2) as f32 };
        if damage_amount <= 0.0 {
            return;
        }
        if let Some(attached) = self.attached_to_entity {
            let source = level.damage_sources().fireworks(self.as_dyn(), self.get_owner(level));
            level.with_entity(attached, |level, e| e.hurt_server(level, &source, 5.0 + (explosions.len() * 2) as f32));
        }
        let rocket_pos = self.base.position();
        let targets = level.get_entities_of_class(&self.base.bounding_box().inflate_scalar(5.0), |e| e.living().is_some(), |_| true);
        for id in targets {
            if Some(id) == self.attached_to_entity {
                continue;
            }
            let Some(target) = level.get_entity(id) else { continue };
            if self.base.distance_to_sqr_vec(target.position()) > 25.0 {
                continue;
            }
            let mut can_see = false;
            for test_step in 0..2 {
                let to = Vec3::new(target.base().x(), target.base().get_y_progress(0.5 * f64::from(test_step)), target.base().z());
                let clip = level.clip(&ClipContext::new(rocket_pos, to, ClipBlock::Collider, ClipFluid::None, collision_context(self.as_dyn())));
                if clip.miss {
                    can_see = true;
                    break;
                }
            }
            if can_see {
                let (xd, yd, zd) = (
                    (self.base.x() - target.base().x()) as f32,
                    (self.base.y() - target.base().y()) as f32,
                    (self.base.z() - target.base().z()) as f32,
                );
                let distance = f64::from(xd * xd + yd * yd + zd * zd).sqrt() as f32;
                let damage = damage_amount * (((5.0 - f64::from(distance)) / 5.0).sqrt() as f32);
                let source = level.damage_sources().fireworks(self.as_dyn(), self.get_owner(level));
                level.with_entity(id, |level, e| e.hurt_server(level, &source, damage));
            }
        }
    }
}

/// `Entity.getHandHoldingItemAngle(item)`.
fn get_hand_holding_item_angle(entity: &dyn Entity, item: Item) -> Vec3 {
    let Some(player) = entity.player() else { return Vec3::ZERO };
    use crate::world::entity::equipment::EquipmentSlot;
    let item_only_in_offhand = entity.get_item_by_slot(EquipmentSlot::OffHand).is(item) && !entity.get_item_by_slot(EquipmentSlot::MainHand).is(item);
    let main_arm = player.get_main_arm();
    let item_arm = if item_only_in_offhand {
        match main_arm {
            HumanoidArm::Right => HumanoidArm::Left,
            HumanoidArm::Left => HumanoidArm::Right,
        }
    } else {
        main_arm
    };
    calculate_view_vector(0.0, entity.base().y_rot() + if item_arm == HumanoidArm::Right { 80.0 } else { -80.0 }).scale(0.5)
}

impl Projectile for FireworkRocketEntity {
    impl_throwable_projectile!();

    fn on_hit_entity(&mut self, level: &mut ServerLevel, _hit: &EntityHitResult) {
        self.explode(level);
    }

    fn on_hit_block(&mut self, level: &mut ServerLevel, hit: &BlockHitResult) {
        let pos = hit.block_pos;
        let state = level.get_block_state(pos);
        crate::world::level::block::behaviour(state.block()).entity_inside(state, level, pos, self.as_dyn_mut(), &mut NoopApplier, true);
        if !self.get_explosions().is_empty() {
            self.explode(level);
        }
        super::on_hit_block_base(self, level, hit);
    }
}

impl Entity for FireworkRocketEntity {
    fn base(&self) -> &EntityData {
        &self.base
    }
    fn base_mut(&mut self) -> &mut EntityData {
        &mut self.base
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_dyn(&self) -> &dyn Entity {
        self
    }
    fn as_dyn_mut(&mut self) -> &mut dyn Entity {
        self
    }
    impl_projectile_entity!(away);

    fn is_attackable(&self) -> bool {
        false
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        super::tick(self, level);
        let hit: HitResult;
        if self.is_attached_to_entity() {
            if self.attached_to_entity.is_none()
                && let Some(id) = self.base.entity_data.get(DATA_ATTACHED_TO_TARGET)
                && level.get_entity(id).is_some_and(|e| e.living().is_some())
            {
                self.attached_to_entity = Some(id);
            }
            if let Some(target_id) = self.attached_to_entity {
                let moved = level.with_entity(target_id, |_, target| {
                    let hand_angle = if target.living().is_some_and(|l| l.is_fall_flying()) {
                        let look_angle = target.get_look_angle();
                        let movement = target.base().delta_movement();
                        target.base_mut().set_delta_movement(movement.add_xyz(
                            look_angle.x * 0.1 + (look_angle.x * 1.5 - movement.x) * 0.5,
                            look_angle.y * 0.1 + (look_angle.y * 1.5 - movement.y) * 0.5,
                            look_angle.z * 0.1 + (look_angle.z * 1.5 - movement.z) * 0.5,
                        ));
                        get_hand_holding_item_angle(target, Item::FIREWORK_ROCKET)
                    } else {
                        Vec3::ZERO
                    };
                    (target.base().position(), hand_angle, target.base().delta_movement())
                });
                if let Some((pos, hand_angle, delta)) = moved {
                    self.set_pos(pos.x + hand_angle.x, pos.y + hand_angle.y, pos.z + hand_angle.z);
                    self.base.set_delta_movement(delta);
                }
            }
            hit = util::get_hit_result_on_move_vector(level, self, &|e| self.can_hit_entity(level, e));
        } else {
            if !self.is_shot_at_angle() {
                let horizontal_acceleration = if self.base.horizontal_collision { 1.0 } else { 1.15 };
                let m = self.base.delta_movement();
                self.base.set_delta_movement(m.multiply_xyz(horizontal_acceleration, 1.0, horizontal_acceleration).add_xyz(0.0, 0.04, 0.0));
            }
            let movement = self.base.delta_movement();
            hit = util::get_hit_result_on_move_vector(level, self, &|e| self.can_hit_entity(level, e));
            self.r#move(level, MoverType::SelfMover, movement);
            self.apply_effects_from_blocks(level);
            self.base.set_delta_movement(movement);
        }
        if !self.base.no_physics && self.is_alive() && hit.get_type() != HitType::Miss {
            hit_target_or_deflect_self(self, level, &hit);
            self.base.needs_sync = true;
        }
        update_rotation(self);
        if self.life == 0 && !self.base.is_silent() {
            let p = self.base.position();
            level.play_sound_at(None, p.x, p.y, p.z, SoundEvent::ENTITY_FIREWORK_ROCKET_LAUNCH, SoundSource::Ambient, 3.0, 1.0);
        }
        self.life += 1;
        if self.life > self.lifetime {
            self.explode(level);
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        super::add_additional_save_data(self, out);
        out.put_int("Life", self.life);
        out.put_int("LifeTime", self.lifetime);
        if let Ok(tag) = self.get_item().to_tag().into_result() {
            out.put("FireworksItem", tag);
        }
        out.put_boolean("ShotAtAngle", self.base.entity_data.get(DATA_SHOT_AT_ANGLE));
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        super::read_additional_save_data(self, input);
        self.life = input.get_int_or("Life", 0);
        self.lifetime = input.get_int_or("LifeTime", 0);
        let item = input.get_compound("FireworksItem").and_then(load_item_stack).unwrap_or_else(|| ItemStack::new(Item::FIREWORK_ROCKET));
        self.base.entity_data.set(DATA_ID_FIREWORKS_ITEM, item);
        self.base.entity_data.set(DATA_SHOT_AT_ANGLE, input.get_boolean_or("ShotAtAngle", false));
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static FIREWORK_ROCKET_FACTORY: EntityRegistration =
    EntityRegistration { entity_type: EntityType::FIREWORK_ROCKET, create: |ty, id, _| Box::new(FireworkRocketEntity::new(ty, id)) };
