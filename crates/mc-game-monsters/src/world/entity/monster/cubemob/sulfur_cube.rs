//! `SulfurCube`: a cube mob that swallows a block item (archetype: attribute modifiers, buoyancy, explosion, contact
//! damage, knockback/sound settings), can be bucketed, sheared and primed.

use mc_game_core::world::entity::monster::cubemob::abstract_cube_mob::{self as acm, CubeMob, CubeMobData, CubeMobFloatGoal, CubeMobKeepOnJumpingGoal, CubeMobMoveControl, CubeMobRandomDirectionGoal, ID_SIZE};
use mc_game_core::define_accessor;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::server::level::level_explode::Explode;
use mc_game_core::world::damagesource::{DamageSource, DamageTypeTags};
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::look_control::{LookControl, LookControlBase};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::bucketable::{self, Bucketable};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::item::primed_tnt::PrimedTnt;
use mc_game_core::world::entity::living_entity::util::{l, lm};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::sulfur_cube_archetype::{self as archetype, ContactDamage, ExplosionData, KnockbackModifiers, SoundSettings};
use mc_game_core::world::entity::{Entity, EntityAttachment, EntityId, RemovalReason, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack, ItemStackExt};
use mc_game_core::world::level::explosion::{ExplosionDamageCalculator, ExplosionInteraction};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::signal_getter::get_best_own_or_neighbour_signal;
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::{Vec2, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, GameEvent, Item, ParticleType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{BUCKET_ENTITY_DATA, CustomData, ItemStackTemplate, SULFUR_CUBE_CONTENT, SulfurCubeContent};
use mc_protocol::packets::play::SoundSource;
use std::sync::Arc;

define_accessor!(MAX_FUSE: Int = SulfurCube);
define_accessor!(FROM_BUCKET: Boolean = SulfurCube);

pub const SPLIT_COUNT: i32 = 2;
pub const MAX_SIZE: i32 = 2;
pub const MIN_SIZE: i32 = 1;
pub const PICKUP_TIMER_DURATION: i32 = 100;
pub const PUSH_DISTANCE_THRESHOLD: f64 = 1.3f32 as f64;

/// `SulfurCube`.
pub struct SulfurCube {
    pub mob: MobData,
    pub cube: CubeMobData,
    pickup_timer: i32,
    push_sound_cooldown: i32,
    floats_in_liquids: bool,
    explosion_data: Option<ExplosionData>,
    knockback_modifier: KnockbackModifiers,
    sound_settings: SoundSettings,
    fuse: i32,
    contact_damages: Vec<ContactDamage>,
    /// Sounds decided by hooks without a level (`knockback`), played at the next tick.
    pending_sounds: Vec<SoundEvent>,
}

/// `isSwallowableItem(itemStack)`.
pub fn is_swallowable_item(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("sulfur_cube_swallowable").is_some_and(|t| t.contains(stack.item()))
}

fn is_sulfur_cube_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("sulfur_cube_food").is_some_and(|t| t.contains(stack.item()))
}

impl SulfurCube {
    /// `new SulfurCube(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::AgeableMob, |b| {
            b.define(ID_SIZE, 1);
            b.define(FROM_BUCKET, false);
            b.define(MAX_FUSE, -1);
        });
        let mut cube = SulfurCube {
            mob,
            cube: CubeMobData::default(),
            pickup_timer: 0,
            push_sound_cooldown: 0,
            floats_in_liquids: false,
            explosion_data: None,
            knockback_modifier: archetype::DEFAULT_KNOCKBACK_MODIFIERS,
            sound_settings: archetype::DEFAULT_SOUND_SETTINGS,
            fuse: -1,
            contact_damages: Vec::new(),
            pending_sounds: Vec::new(),
        };
        let navigation = StandardPathNavigation::ground(&cube);
        cube.mob.set_navigation(Box::new(navigation));
        cube.register_goals();
        cube.fixup_dimensions();
        cube.mob.move_control = Some(Box::new(CubeMobMoveControl::new(cube.base().y_rot())));
        cube.mob.look_control = Some(Box::new(SulfurCubeLookControl::default()));
        cube
    }

    /// `registerGoals()`: the `AbstractCubeMob` goals, `addBehaviourGoals()`, no targeting goals.
    fn register_goals(&mut self) {
        let float = CubeMobFloatGoal::new(self);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(4, Box::new(CubeMobRandomDirectionGoal::new()));
        goals.add_goal(5, Box::new(CubeMobKeepOnJumpingGoal));
        goals.add_goal(2, Box::new(SulfurCubeTemptGoal::default()));
        goals.add_goal(3, Box::new(SulfurCubeSearchForItemsGoal::default()));
    }

    pub fn get_fuse(&self) -> i32 {
        self.fuse
    }

    pub fn is_primed(&self) -> bool {
        self.fuse >= 0
    }

    /// `hasBodyItem()`.
    pub fn has_body_item(&self) -> bool {
        !self.get_item_by_slot(EquipmentSlot::Body).is_empty()
    }

    /// `canExplode()`.
    pub fn can_explode(&self) -> bool {
        self.explosion_data.is_some() && self.is_alive() && !self.is_primed()
    }

    /// `setFuse(fuse)` + `entityData.set(MAX_FUSE, fuse)` (the synched update runs `setFuse`).
    fn set_max_fuse(&mut self, fuse: i32) {
        self.fuse = fuse;
        self.mob.living.entity.entity_data.set(MAX_FUSE, fuse);
    }

    /// `primeTime(imminent)`.
    pub fn prime_time(&mut self, level: &mut ServerLevel, imminent: bool) -> bool {
        let Some(data) = self.explosion_data else { return false };
        if !(self.is_alive() && level.tnt_explodes() && !self.is_primed()) {
            return false;
        }
        let fuse = data.fuse;
        let fuse_time = if imminent { PrimedTnt::get_random_short_fuse(fuse, &mut self.base_mut().random) } else { fuse };
        self.base_mut().permanently_invulnerable = true;
        self.set_max_fuse(fuse_time);
        mc_game_core::world::entity::living_entity::damage::make_sound(self, level, SoundEvent::ENTITY_TNT_PRIMED);
        self.game_event(level, GameEvent::PRIME_FUSE);
        true
    }

    fn tick_fuse(&mut self, level: &mut ServerLevel) {
        if self.fuse > 0 {
            self.fuse -= 1;
        }
        if let Some(data) = self.explosion_data {
            if self.fuse == 0 {
                mob::leashable::drop_leash(self, level);
                lm(self).dead = true;
                if level.tnt_explodes() {
                    let interaction = if level.mob_griefing() { ExplosionInteraction::Tnt } else { ExplosionInteraction::None };
                    let calculator = (self.base().portal_cooldown > 0).then_some(&mc_game_core::world::entity::item::primed_tnt::USED_PORTAL_DAMAGE_CALCULATOR as &dyn ExplosionDamageCalculator);
                    let y = self.base().get_y_progress(0.0625);
                    let mut params = Explode::new(Some(self), self.base().x(), y, self.base().z(), data.power as f32, interaction).with_damage_source(None, calculator);
                    params.fire = data.causes_fire;
                    level.explode(params, &mut []);
                }
                mc_game_core::world::entity::living_entity::effects::trigger_on_death_mob_effects(self, level, RemovalReason::Killed);
                self.discard();
            }
        }
    }

    fn prime_when_on_powered_position(&mut self, level: &mut ServerLevel) {
        if self.can_explode() {
            let here = BlockPos::containing(self.base().x(), self.base().y(), self.base().z());
            if get_best_own_or_neighbour_signal(&*level, here) != 0 {
                self.prime_time(level, false);
            }
        }
    }

    /// The `collectEquipmentChanges` override: goal and archetype bookkeeping when the body item changes.
    fn apply_body_item_change(&mut self, level: &mut ServerLevel, previous: &ItemStack, current: &ItemStack) {
        if ItemStack::matches(current, previous) {
            return;
        }
        if !current.is_empty() {
            let mut goals = std::mem::take(&mut self.mob.goal_selector);
            goals.remove_all_goals(self, level, |_| true);
            self.mob.goal_selector = goals;
            self.set_speed(0.0);
        } else {
            self.register_goals();
        }
        for archetype in archetype::matching_archetypes(&level.registries, previous.item()) {
            for entry in &archetype.attribute_modifiers {
                if let Some(attr) = lm(self).attributes.get_instance(entry.attribute) {
                    attr.remove_modifier(&entry.modifier.id);
                }
            }
        }
        self.floats_in_liquids = false;
        self.explosion_data = None;
        self.contact_damages.clear();
        self.knockback_modifier = archetype::DEFAULT_KNOCKBACK_MODIFIERS;
        self.sound_settings = archetype::DEFAULT_SOUND_SETTINGS;
        for archetype in archetype::matching_archetypes(&level.registries, current.item()) {
            if archetype.buoyant {
                self.floats_in_liquids = true;
            }
            if archetype.explosion.is_some() {
                self.explosion_data = archetype.explosion;
            }
            if let Some(contact) = &archetype.contact_damage {
                self.contact_damages.push(contact.clone());
            }
            self.knockback_modifier = archetype.knockback_modifiers;
            self.sound_settings = archetype.sound_settings;
            for entry in &archetype.attribute_modifiers {
                if let Some(attr) = lm(self).attributes.get_instance(entry.attribute) {
                    attr.add_or_update_transient_modifier(entry.modifier.clone());
                }
            }
        }
    }

    /// `equipItem(heldItem)`.
    pub fn equip_item(&mut self, level: &mut ServerLevel, held_item: &ItemStack) -> bool {
        if self.is_baby() {
            return false;
        }
        if self.has_body_item() {
            let swallowed = self.get_item_by_slot(EquipmentSlot::Body);
            if held_item.is(swallowed.item()) {
                return false;
            }
            let offset = self.get_attachments().get_average(EntityAttachment::Passenger);
            mc_game_core::world::entity::item::spawn_at_location(level, self, swallowed, offset);
        }
        let previous = self.get_item_by_slot(EquipmentSlot::Body);
        let mut one = held_item.clone();
        one.set_count(1);
        mob::equipment::set_item_slot_and_drop_when_killed(self, level, EquipmentSlot::Body, one);
        if !previous.is_empty() {
            let current = self.get_item_by_slot(EquipmentSlot::Body);
            self.apply_body_item_change(level, &previous, &current);
        }
        self.play_sound_default(level, SoundEvent::ENTITY_SULFUR_CUBE_ABSORB);
        true
    }

    /// `shear(level, soundSource, tool)`.
    fn shear(&mut self, level: &mut ServerLevel) {
        let offset = self.get_attachments().get_average(EntityAttachment::Passenger);
        let to_shear = self.get_item_by_slot(EquipmentSlot::Body);
        mob::equipment::set_item_slot(self, level, EquipmentSlot::Body, ItemStack::empty());
        mc_game_core::world::entity::item::spawn_at_location(level, self, to_shear, offset);
        self.play_sound_default(level, SoundEvent::ENTITY_SULFUR_CUBE_EJECT);
        self.pickup_timer = 100;
    }

    /// `applyContactDamage(entity)`.
    fn apply_contact_damage(&mut self, level: &mut ServerLevel, entity: &mut dyn Entity) {
        for damage in self.contact_damages.clone() {
            let source_entity = if !damage.attribute_to_source && !entity.is_player() { None } else { Some(self.as_dyn()) };
            let source = level.damage_sources().source(&damage.damage_type.to_string(), source_entity, source_entity);
            let amount = damage.amount.sample(&mut self.base_mut().random);
            entity.hurt_server(level, &source, amount);
        }
    }

    /// `playerPush(player)`.
    fn player_push(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        if !self.has_body_item() {
            return;
        }
        let passenger = player.base().is_passenger();
        let (pusher_pos, pusher_height) = if passenger {
            let root = player.get_root_vehicle(level);
            match level.get_entity(root) {
                Some(vehicle) => (vehicle.position(), f64::from(vehicle.base().get_bb_height())),
                None => (player.position(), f64::from(player.base().get_bb_height())),
            }
        } else {
            (player.position(), f64::from(player.base().get_bb_height()))
        };
        let cube_to_pusher = self.base().position().subtract(pusher_pos);
        let pusher_feet = pusher_pos.y;
        let cube_bottom = self.base().y();
        let cube_top = cube_bottom + f64::from(self.base().get_bb_height());
        let pusher_top = pusher_feet + pusher_height;
        if cube_to_pusher.horizontal_distance() < PUSH_DISTANCE_THRESHOLD && pusher_feet <= cube_top && pusher_top > cube_bottom {
            let knockback = (1.0 - l(self).get_attribute_value(Attribute::KNOCKBACK_RESISTANCE)).max(0.0);
            let push_direction = cube_to_pusher.horizontal().normalize().scale(knockback);
            let push_speed_scale = if passenger { 0.16f32 } else { 0.3f32 };
            let mut player_speed = player.get_known_speed(level).length() * 2.0 * f64::from(push_speed_scale);
            player_speed = mth::clamp_f64(player_speed, 0.0, 0.5);
            let y = if self.base().on_ground() { knockback * f64::from(0.3f32) } else { 0.0 };
            let push_velocity = Vec3::new(push_direction.x, y, push_direction.z).scale(player_speed);
            self.base_mut().needs_sync = true;
            let threshold = self.sound_settings.push_sound_impulse_threshold;
            if push_velocity.length_sqr() > f64::from(threshold * threshold) && self.push_sound_cooldown <= 0 {
                self.push_sound_cooldown = (self.sound_settings.push_sound_cooldown * 20.0) as i32;
                let sound = self.sound_settings.push_sound;
                self.play_sound_default(level, sound);
            }
            self.base_mut().add_delta_movement(push_velocity);
            self.apply_contact_damage(level, player);
        }
    }
}

// ---- knockback geometry ----------------------------------------------------------------------------------------------

fn apply_horizontal_hit_angle_scale(horizontal_angle_scale: f32, original_angle: Vec2, attacker_position: Vec3, attacker_aim: Vec3, target_center: Vec3) -> Vec2 {
    let attacker_to_target = target_center.subtract(attacker_position).normalize();
    let angle_diff = mth::atan2(attacker_aim.x * attacker_to_target.z - attacker_aim.z * attacker_to_target.x, attacker_aim.x * attacker_to_target.x + attacker_aim.z * attacker_to_target.z) as f32;
    original_angle.rotate(f64::from(angle_diff * horizontal_angle_scale))
}

fn apply_vertical_hit_angle_power_transfer(
    vertical_hit_angle_scale: f32,
    horizontal_power: f32,
    vertical_power: f32,
    attacker_position: Vec3,
    attacker_aim: Vec3,
    target_centered: Vec3,
    target_height: f32,
) -> Vec2 {
    let half = 0.5f32 * target_height;
    let top = target_centered.add_xyz(0.0, f64::from(half), 0.0);
    let bottom = target_centered.add_xyz(0.0, f64::from(-half), 0.0);
    let to_top = top.subtract(attacker_position).normalize();
    let to_bottom = bottom.subtract(attacker_position).normalize();
    let factor = mth::clamped_map_f64(attacker_aim.y, to_top.y, to_bottom.y, -1.0, 1.0) as f32;
    let mut ratio = (factor * vertical_hit_angle_scale).abs();
    if factor < 0.0 {
        ratio = -ratio;
    }
    Vec2::new(horizontal_power * (1.0 - ratio), vertical_power * (1.0 + ratio))
}

fn apply_vertical_position_angle_power_rotation(
    scale: f32,
    horizontal_power: f32,
    vertical_power: f32,
    original_horizontal_power: f32,
    original_vertical_power: f32,
    attacker_feet: Vec3,
    target_feet: Vec3,
) -> Vec2 {
    let delta = target_feet.subtract(attacker_feet);
    let angle = mth::atan2(-delta.y, delta.horizontal_distance()) as f32;
    let mut rotated = Vec2::new(horizontal_power, vertical_power).rotate(f64::from(-angle * scale));
    let h_ratio = if original_horizontal_power > 0.0 { rotated.x.abs() / original_horizontal_power } else { 0.0 };
    let v_ratio = if original_vertical_power > 0.0 { rotated.y.abs() / original_vertical_power } else { 0.0 };
    let max_ratio = h_ratio.max(v_ratio);
    if max_ratio > 1.0 {
        rotated = rotated.scale(1.0 / max_ratio);
    }
    rotated
}

// ---- look control ----------------------------------------------------------------------------------------------------

/// `SulfurCube.SulfurCubeLookControl`.
#[derive(Default)]
struct SulfurCubeLookControl {
    base: LookControlBase,
}

impl LookControl for SulfurCubeLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let has_body_item = !mob.get_item_by_slot(EquipmentSlot::Body).is_empty();
        if !has_body_item {
            mc_game_core::world::entity::ai::control::look_control::default_tick(self, mob, level);
        } else {
            let close_angle = mth::wrap_degrees90(mob.base().y_rot());
            let y_rot = mob.base().y_rot() - close_angle;
            mob.set_y_rot(y_rot);
            mob.set_y_head_rot(y_rot);
        }
    }
}

// ---- goals -----------------------------------------------------------------------------------------------------------

/// `SulfurCube.SulfurCubeTemptGoal` (`TemptGoal.ForNonPathfinders` with the move control as navigation, `canScare` false,
/// flags `LOOK`, stop distance 1.0).
#[derive(Default)]
struct SulfurCubeTemptGoal {
    player: Option<EntityId>,
    calm_down: i32,
}

impl Goal for SulfurCubeTemptGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.calm_down > 0 {
            self.calm_down -= 1;
            return false;
        }
        let baby = mob.is_baby();
        let selector = Arc::new(move |target: &dyn Entity, _level: &ServerLevel| {
            let follows = |stack: &ItemStack| if baby { is_sulfur_cube_food(stack) } else { is_swallowable_item(stack) };
            follows(&target.get_item_by_slot(EquipmentSlot::MainHand)) || follows(&target.get_item_by_slot(EquipmentSlot::OffHand))
        });
        let conditions = TargetingConditions::for_non_combat().ignore_line_of_sight().selector(Some(selector)).range(l(mob).get_attribute_value(Attribute::TEMPT_RANGE));
        let pos = mob.position();
        let players: Vec<EntityId> = level.players().to_vec();
        self.player = mob::ext::get_nearest_entity(level, &players, &conditions, mob, pos.x, pos.y, pos.z);
        self.player.is_some()
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.player = None;
        acm::set_wanted_movement(mob, 0.0);
        self.calm_down = reduced_tick_delay(100);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(player_id) = self.player else { return };
        let Some(player) = level.get_entity(player_id) else { return };
        let (y_max, x_max) = (mob.get_max_head_y_rot() + 20, mob.get_max_head_x_rot());
        mob::look_at_entity_full(mob, player, y_max as f32, x_max as f32);
        if mob.base().distance_to_sqr_vec(player.position()) < 1.0 {
            acm::set_wanted_movement(mob, 0.0);
        } else {
            mob::ext::mob_look_at(mob, player, 10.0, 10.0);
            let y_rot = mob.base().y_rot();
            acm::set_direction(mob, y_rot, true);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}

/// `SulfurCube.SulfurCubeSearchForItemsGoal`.
#[derive(Default)]
struct SulfurCubeSearchForItemsGoal {
    target_item: Option<EntityId>,
}

impl Goal for SulfurCubeSearchForItemsGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let pickup_timer = mob.as_any().downcast_ref::<SulfurCube>().map_or(0, |c| c.pickup_timer);
        if mob.is_baby() || pickup_timer > 0 {
            return false;
        }
        let bb = mob.base().bounding_box().inflate(8.0, 8.0, 8.0);
        let items = level.get_entities_of_class(&bb, |e| e.get_type() == EntityType::ITEM, |e| {
            e.as_any().downcast_ref::<ItemEntity>().is_some_and(|item| !item.has_pick_up_delay() && item.is_alive() && is_swallowable_item(&item.get_item()))
        });
        let pos = mob.position();
        let mut best = -1.0;
        self.target_item = None;
        for id in items {
            if let Some(item) = level.get_entity(id) {
                let dist = item.base().distance_to_sqr_xyz(pos.x, pos.y, pos.z);
                if best == -1.0 || dist < best {
                    best = dist;
                    self.target_item = Some(id);
                }
            }
        }
        self.target_item.is_some()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(item) = self.target_item.and_then(|id| level.get_entity(id)) {
            mob::ext::mob_look_at(mob, item, 10.0, 10.0);
        }
        let y_rot = mob.base().y_rot();
        acm::set_direction(mob, y_rot, true);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}

// ---- CubeMob / Bucketable --------------------------------------------------------------------------------------------

impl CubeMob for SulfurCube {
    fn cube(&self) -> &CubeMobData {
        &self.cube
    }
    fn cube_mut(&mut self) -> &mut CubeMobData {
        &mut self.cube
    }
    fn get_particle_type(&self) -> Option<ParticleType> {
        Some(ParticleType::SULFUR_CUBE_GOO)
    }
    fn get_squish_sound(&self) -> SoundEvent {
        if self.is_tiny() {
            SoundEvent::ENTITY_SMALL_SULFUR_CUBE_SQUISH
        } else if self.has_body_item() {
            SoundEvent::ENTITY_SULFUR_CUBE_BOUNCE
        } else {
            SoundEvent::ENTITY_SULFUR_CUBE_SQUISH
        }
    }
    fn get_jump_sound(&self) -> SoundEvent {
        if self.is_tiny() { SoundEvent::ENTITY_SMALL_SULFUR_CUBE_JUMP } else { SoundEvent::ENTITY_SULFUR_CUBE_JUMP }
    }
    fn can_deal_damage(&self) -> bool {
        false
    }
    fn get_split_count(&mut self) -> i32 {
        if self.is_primed() { 0 } else { 2 }
    }
    fn set_cube_mob_health(&mut self, actual_size: i32) {
        if let Some(health) = self.mob.living.attributes.get_instance(Attribute::MAX_HEALTH) {
            health.set_base_value(f64::from(4 * actual_size));
        }
    }
    fn set_spawn_size(&mut self, _level: &mut dyn ServerLevelAccessor, _difficulty: DifficultyInstance) {
        let size = if self.is_baby() { 1 } else { 2 };
        self.set_size(size, true);
    }
    fn set_size(&mut self, size: i32, update_health: bool) {
        acm::set_size(self, size, update_health);
        if update_health && size == 1 && !self.is_baby() {
            mob::ageable::set_baby(self, true);
        }
    }
    fn split_cubes_are_babies(&self) -> bool {
        true
    }
    fn cube_move_control_active(&self) -> bool {
        !self.has_body_item()
    }
}

impl Bucketable for SulfurCube {
    fn from_bucket(&self) -> bool {
        self.mob.living.entity.entity_data.get(FROM_BUCKET)
    }
    fn set_from_bucket(&mut self, from_bucket: bool) {
        self.mob.living.entity.entity_data.set(FROM_BUCKET, from_bucket);
    }
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        bucketable::save_default_data_to_bucket_tag(self, bucket);
        let body = self.get_item_by_slot(EquipmentSlot::Body);
        if !body.is_empty() {
            if let Ok(template) = ItemStackTemplate::from_stack(&body) {
                bucket.set(SULFUR_CUBE_CONTENT, SulfurCubeContent { absorbed_block_item_stack: template });
            }
        }
        let mut tag = bucket.get(&BUCKET_ENTITY_DATA).map(|d| d.copy_tag()).unwrap_or_default();
        tag.put_int("age", mob::ageable::get_age(self));
        tag.put_boolean("age_locked", mob::ageable::is_age_locked(self));
        bucket.set(BUCKET_ENTITY_DATA, CustomData::of(&tag));
    }
    fn load_from_bucket_tag(&mut self, tag: &CompoundTag) {
        bucketable::load_default_data_from_bucket_tag(self, tag);
        if mob::ageable::set_age(self, tag.get_int_or("age", 0)) && !self.is_baby() {
            self.set_size(2, true);
        }
        mob::ageable::set_age_locked(self, tag.get_boolean_or("age_locked", false));
    }
    fn get_bucket_item_stack(&self) -> ItemStack {
        ItemStack::new(Item::SULFUR_CUBE_BUCKET)
    }
    fn get_pickup_sound(&self) -> SoundEvent {
        SoundEvent::ITEM_BUCKET_FILL_SULFUR_CUBE
    }
    fn can_be_picked_up_with_bucket(&self, stack: &ItemStack) -> bool {
        stack.item() == Item::BUCKET
    }
}

// ---- Entity ----------------------------------------------------------------------------------------------------------

impl Entity for SulfurCube {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:sulfur_cube")
    }
    fn as_cube_mob(&self) -> Option<&dyn CubeMob> {
        Some(self)
    }
    fn as_cube_mob_mut(&mut self) -> Option<&mut dyn CubeMob> {
        Some(self)
    }
    fn as_bucketable(&self) -> Option<&dyn Bucketable> {
        Some(self)
    }
    fn as_bucketable_mut(&mut self) -> Option<&mut dyn Bucketable> {
        Some(self)
    }
    fn cube_mob_size(&self) -> Option<i32> {
        Some(self.get_size())
    }
    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        acm::get_dimensions(self, pose)
    }
    fn refresh_dimensions(&mut self) {
        acm::refresh_dimensions(self);
    }
    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == MAX_FUSE.id() {
            self.fuse = self.mob.living.entity.entity_data.get(MAX_FUSE);
        }
        acm::on_synced_data_updated(self, accessor_id);
    }
    fn tick(&mut self, level: &mut ServerLevel) {
        self.tick_fuse(level);
        self.prime_when_on_powered_position(level);
        for sound in std::mem::take(&mut self.pending_sounds) {
            self.play_sound_default(level, sound);
        }
        acm::tick(self, level);
    }
    fn before_remove(&mut self, level: &mut ServerLevel, reason: RemovalReason) {
        acm::before_remove(self, level, reason);
    }
    fn push_entity(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        acm::push_entity(self, level, other);
    }
    fn do_push(&mut self, level: &mut ServerLevel, other: &mut dyn Entity) {
        mc_game_core::world::entity::living_entity::push::do_push(level, self, other);
        self.apply_contact_damage(level, other);
    }
    fn player_touch(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        acm::player_touch(self, level, player);
        self.player_push(level, player);
    }
    fn jump_from_ground(&mut self, level: &ServerLevel) {
        acm::jump_from_ground(self, level, 0.0);
    }
    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }
    fn get_sound_volume(&self) -> f32 {
        0.4 * self.get_size() as f32
    }
    fn get_max_head_x_rot(&self) -> i32 {
        0
    }
    fn get_passenger_attachment_point(&self, _passenger: &dyn Entity, dimensions: &EntityDimensions, scale: f32) -> Vec3 {
        acm::get_passenger_attachment_point(self, dimensions, scale)
    }
    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_tiny() { SoundEvent::ENTITY_SMALL_SULFUR_CUBE_HURT } else { SoundEvent::ENTITY_SULFUR_CUBE_HURT })
    }
    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(if self.is_tiny() { SoundEvent::ENTITY_SMALL_SULFUR_CUBE_DEATH } else { SoundEvent::ENTITY_SULFUR_CUBE_DEATH })
    }
    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, block_state: mc_data::block::BlockState) {
        if !self.has_body_item() {
            let sound_type = block_state.get_sound_type();
            self.play_sound(level, sound_type.step_sound, sound_type.volume * 0.15, sound_type.pitch);
        }
    }
    fn get_fluid_jump_threshold(&self) -> f64 {
        f64::from(self.base().get_bb_height()) * 0.2
    }
    fn can_breathe_underwater(&self) -> bool {
        self.has_body_item() || mc_data::tags::get::<EntityType>("can_breathe_under_water").is_some_and(|t| t.contains(self.get_type()))
    }
    fn travel_in_fluid(&mut self, level: &mut ServerLevel, input: Vec3) {
        mc_game_core::world::entity::living_entity::travel::travel_in_fluid(self, level, input);
        if self.has_body_item() && self.floats_in_liquids {
            let vibe_amount = 0.2f32 * mth::sin(f64::from(self.base().tick_count as f32 * 0.4f32));
            let fluid = if self.is_in_water() { "water" } else { "lava" };
            let immersion = self.get_fluid_height(fluid) - self.get_fluid_jump_threshold() + f64::from(vibe_amount);
            if immersion > 0.0 {
                let dm = self.base().delta_movement();
                self.base_mut().set_delta_movement(dm.add_xyz(0.0, immersion.min(1.0) * f64::from(0.04f32), 0.0));
            }
        }
    }
    fn requires_custom_persistence(&self) -> bool {
        (self.mob.living.entity.is_passenger() || mob::leashable::is_leashed(self)) || self.has_body_item() || self.from_bucket()
    }
    fn can_be_leashed(&self) -> bool {
        self.has_body_item()
    }
    fn max_up_step(&self) -> f32 {
        if self.has_body_item() { 0.0 } else { self.mob.living.get_attribute_value(Attribute::STEP_HEIGHT) as f32 }
    }
    fn omnidirectional_air_mover(&self) -> bool {
        self.has_body_item()
    }
    fn can_freeze(&self) -> bool {
        !self.has_body_item()
    }
    fn is_invulnerable_to_piercing_weapon(&self) -> bool {
        self.is_invulnerable() && !self.is_primed()
    }
    fn can_be_picked_from_inside(&self) -> bool {
        !self.has_body_item()
    }
    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.has_body_item() {
            if self.can_explode() && !self.is_primed() {
                let direct_on_fire_arrow = source
                    .direct_entity
                    .and_then(|id| level.get_entity(id))
                    .is_some_and(|e| matches!(e.get_type(), EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT) && e.is_on_fire());
                if source.is_tag(level, DamageTypeTags::IS_FIRE) || direct_on_fire_arrow {
                    self.prime_time(level, false);
                } else if source.is_tag(level, DamageTypeTags::IS_EXPLOSION) {
                    self.prime_time(level, true);
                }
            }
            if source.is_tag(level, DamageTypeTags::SULFUR_CUBE_WITH_BLOCK_IMMUNE_TO) {
                if !source.is_tag(level, DamageTypeTags::NO_KNOCKBACK) {
                    mc_game_core::world::entity::living_entity::damage::deal_default_knockback(self, level, source, damage, true);
                }
                return true;
            }
        }
        mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }
    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        if self.pickup_timer > 0 {
            self.pickup_timer -= 1;
        }
        if self.push_sound_cooldown > 0 {
            self.push_sound_cooldown -= 1;
        }
    }
    fn collect_equipment_changes_pre(&mut self, level: &mut ServerLevel, last: &[ItemStack; 8]) {
        let previous = last[EquipmentSlot::Body as usize].clone();
        let current = self.get_item_by_slot(EquipmentSlot::Body);
        self.apply_body_item_change(level, &previous, &current);
    }
    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let held = player.get_item_in_hand(hand);
        if self.is_baby() {
            if is_sulfur_cube_food(&held) && self.can_age_up() {
                let age = mob::ageable::get_age(self);
                mob::interact::use_player_item(level, player, hand);
                mob::ageable::age_up(self, level, mob::ageable::get_speed_up_seconds_when_feeding(age.wrapping_neg()), true);
                mc_game_core::world::entity::living_entity::damage::make_sound(self, level, SoundEvent::ENTITY_SMALL_SULFUR_CUBE_EAT);
                InteractionResult::SUCCESS
            } else {
                mob::interact::mob_interact(self, level, player, hand)
            }
        } else if self.is_primed() {
            InteractionResult::PASS
        } else if !(self.can_explode() && (held.is(Item::FLINT_AND_STEEL) || held.is(Item::FIRE_CHARGE))) {
            if held.is(Item::SHEARS) && self.has_body_item() {
                let sheared = self.get_item_by_slot(EquipmentSlot::Body);
                self.shear(level);
                level.game_event_from_entity(Some(player.id()), GameEvent::SHEAR, self.position());
                mob::interact::with_hand_stack(player, hand, |s, p| mc_game_core::world::item::hurt_and_break(s, 1, level, p, hand.as_equipment_slot()));
                mob::ext::trigger_player_sheared_equipment(level, player.id(), &sheared, self.as_dyn());
                InteractionResult::SUCCESS
            } else if is_swallowable_item(&held) {
                let worked = self.equip_item(level, &held);
                if worked {
                    let infinite = player.has_infinite_materials();
                    mob::interact::with_hand_stack(player, hand, |s, _| s.consume(1, infinite));
                    self.game_event(level, GameEvent::ENTITY_INTERACT);
                }
                if worked { InteractionResult::SUCCESS_SERVER } else { InteractionResult::PASS }
            } else {
                match bucketable::bucket_mob_pickup_in_place(level, player, hand, self) {
                    Some(result) => result,
                    None => mob::interact::mob_interact(self, level, player, hand),
                }
            }
        } else if !level.tnt_explodes() {
            player.send_overlay_message(mc_text::Component::translatable("block.minecraft.tnt.disabled", Vec::new()));
            InteractionResult::PASS
        } else {
            self.prime_time(level, false);
            let infinite = player.has_infinite_materials();
            if held.is(Item::FLINT_AND_STEEL) {
                mob::interact::with_hand_stack(player, hand, |s, p| mc_game_core::world::item::hurt_and_break(s, 1, level, p, hand.as_equipment_slot()));
            } else {
                mob::interact::with_hand_stack(player, hand, |s, _| s.consume(1, infinite));
            }
            player.award_stat_once(level, mc_game_core::stats::Stat::Used(held.item()));
            InteractionResult::SUCCESS_SERVER
        }
    }
    fn get_equipment_slot_for_item(&self, stack: &ItemStack) -> EquipmentSlot {
        if is_swallowable_item(stack) { EquipmentSlot::Body } else { mob::equipment::get_equipment_slot_for_item(self, stack) }
    }
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool {
        if slot == EquipmentSlot::Body { is_swallowable_item(stack) } else { mob::equipment::is_equippable_in_slot(self, stack, slot) }
    }
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body || (self.is_alive() && !self.is_baby())
    }
    fn can_pick_up_loot(&self) -> bool {
        !self.has_body_item()
    }
    fn can_hold_item(&self, stack: &ItemStack) -> bool {
        !self.has_body_item() && is_swallowable_item(stack) && !self.is_baby()
    }
    fn pick_up_item(&mut self, level: &mut ServerLevel, item_id: EntityId) {
        let Some(stack) = level.get_entity(item_id).and_then(|e| e.as_any().downcast_ref::<ItemEntity>()).map(|e| e.get_item()) else { return };
        if self.can_hold_item(&stack) && self.pickup_timer <= 0 {
            mc_game_core::world::entity::player::pickup::on_item_pickup_by_id(level, &*self, item_id);
            let mut taken = ItemStack::empty();
            level.with_entity(item_id, |_, item| {
                if let Some(ie) = item.as_any_mut().downcast_mut::<ItemEntity>() {
                    let mut s = ie.get_item();
                    taken = s.split(1);
                    ie.set_item_in_place(s);
                }
            });
            mob::equipment::set_item_slot(self, level, EquipmentSlot::Body, taken);
            self.play_sound_default(level, SoundEvent::ENTITY_SULFUR_CUBE_ABSORB);
            mob::equipment::set_guaranteed_drop(self, EquipmentSlot::Body);
            let packet = mc_protocol::packets::play::ClientboundTakeItemEntityPacket { item_id, player_id: self.id(), amount: 1 };
            if !level.get_entity(item_id).is_some_and(|x| x.is_removed()) {
                level.entity_tracker.send_to_tracking_players(item_id, &mc_game_core::server::level::server_entity::encode_payload(&packet));
            }
        }
    }
    fn get_base_experience_reward(&mut self, _level: &ServerLevel) -> i32 {
        if self.is_baby() { 0 } else { 1 + self.base_mut().random.next_int_bounded(2) }
    }
    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        let mut baby = mc_game_core::world::entity::factory::create(EntityType::SULFUR_CUBE, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })?;
        baby.as_cube_mob_mut()?.set_size(1, true);
        Some(baby)
    }
    fn is_food(&self, stack: &ItemStack) -> bool {
        is_sulfur_cube_food(stack)
    }
    fn age_boundary_reached(&mut self, level: &mut ServerLevel) {
        mob::ageable::age_boundary_reached(self, level);
        if !self.is_baby() {
            acm::set_size_in_level(self, level, 2, true);
        }
    }
    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        acm::finalize_spawn(self, level, difficulty, reason, group_data)
    }
    fn knockback(&mut self, power: f64, xd: f64, zd: f64, source: &DamageSource, damage: f32, comes_from_effect: bool) {
        let attacker = source.causing_info().cloned();
        let Some(attacker) = attacker.filter(|_| self.has_body_item()) else {
            mc_game_core::world::entity::living_entity::damage::knockback(self, power, xd, zd, source, damage, comes_from_effect);
            return;
        };
        let mut horizontal_power = self.knockback_modifier.horizontal_power;
        let mut vertical_power = self.knockback_modifier.vertical_power;
        let hit_sound = self.sound_settings.hit_sound;
        let original_angle = Vec2::new(xd as f32, zd as f32);
        let aim = attacker.look_angle.normalize();
        let center = self.base().bounding_box().get_center();
        let new_angle = apply_horizontal_hit_angle_scale(1.6, original_angle, attacker.eye_position, aim, center);
        let mut new_power = apply_vertical_hit_angle_power_transfer(0.5, horizontal_power, vertical_power, attacker.eye_position, aim, center, self.base().get_bb_height());
        new_power = apply_vertical_position_angle_power_rotation(0.8, new_power.x, new_power.y, horizontal_power, vertical_power, attacker.position, self.base().position());
        horizontal_power = new_power.x;
        vertical_power = new_power.y;
        let (xd, zd) = (f64::from(new_angle.x), f64::from(new_angle.y));
        let power_multiplier = mth::sqrt(damage) * if comes_from_effect { power as f32 * 0.25 } else { 1.0 };
        horizontal_power *= power_multiplier;
        vertical_power *= power_multiplier;
        let knockback_resistance = l(self).get_attribute_value(Attribute::KNOCKBACK_RESISTANCE);
        horizontal_power *= (1.0 - knockback_resistance) as f32;
        vertical_power *= (1.0 - knockback_resistance) as f32;
        self.base_mut().needs_sync = true;
        let delta_movement = self.base().delta_movement();
        horizontal_power *= 0.4;
        horizontal_power = mth::clamp_f32(horizontal_power, -128.0, 128.0);
        vertical_power = mth::clamp_f32(vertical_power, -128.0, 128.0);
        let horizontal_knockback = Vec3::new(xd, 0.0, zd).normalize().scale(f64::from(horizontal_power));
        self.base_mut().set_delta_movement_xyz(delta_movement.x - horizontal_knockback.x, delta_movement.y + f64::from(vertical_power) * 1.2, delta_movement.z - horizontal_knockback.z);
        self.pending_sounds.push(hit_sound);
    }
    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
        if let Some(content) = stack.get(&SULFUR_CUBE_CONTENT) {
            let body = content.absorbed_block_item_stack.create();
            mob::equipment::set_item_slot_spawn(self, EquipmentSlot::Body, body);
            mob::equipment::set_guaranteed_drop(self, EquipmentSlot::Body);
        }
    }
    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        acm::add_additional_save_data(self, out);
        out.put_int("pickup_timer", self.pickup_timer);
        out.put_boolean("from_bucket", self.from_bucket());
        out.put_int("fuse", self.fuse);
    }
    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        self.pickup_timer = input.get_int_or("pickup_timer", 0);
        self.set_from_bucket(input.get_boolean_or("from_bucket", false));
        self.fuse = input.get_int_or("fuse", -1);
        let fuse = self.fuse;
        self.mob.living.entity.entity_data.set(MAX_FUSE, fuse);
        acm::read_additional_save_data(self, input);
    }
}

/// `SulfurCube.checkSulfurCubeSpawnRules`.
pub fn check_sulfur_cube_spawn_rules(_ty: EntityType, _level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
    true
}

mc_game_core::register_spawn_placement!(EntityType::SULFUR_CUBE, check_sulfur_cube_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static SULFUR_CUBE: EntityRegistration = EntityRegistration { entity_type: EntityType::SULFUR_CUBE, create: |ty, id, _registries| Box::new(SulfurCube::new(ty, id)) };

