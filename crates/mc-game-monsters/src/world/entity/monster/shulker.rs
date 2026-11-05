//! `net.minecraft.world.entity.monster.Shulker` (+ its goals, look and body-rotation controls).

use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_core::world::entity::ai::control::body_rotation_control::{BodyRotationControl, BodyRotationControlBase};
use mc_game_core::world::entity::ai::control::look_control::{self, LookControl, LookControlBase};
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, LookAtPlayerGoal, RandomLookAroundGoal, reduced_tick_delay};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::attributes::{AttributeModifier, AttributeModifierOperation};
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::entity::MovementEmission;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::projectile::shulker_bullet::ShulkerBullet;
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, MoverType};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::collision::{BlockGetterExt as _, CollisionGetter as _};
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::shapes::is_face_full;
use mc_game_core::world::level::level_access::LevelAccessor as _;
use mc_game_core::world::level::{BlockGetter, LevelReader as _};
use linkme::distributed_slice;
use mc_core::direction::{Axis, Direction};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, GameEvent, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::components::{DataComponentMap, SHULKER_COLOR};
use mc_protocol::packets::play::SoundSource;
use mc_world::level_storage::Difficulty;
use std::sync::{Arc, LazyLock};

define_accessor!(pub DATA_ATTACH_FACE_ID: Direction = Shulker);
define_accessor!(pub DATA_PEEK_ID: Byte = Shulker);
define_accessor!(pub DATA_COLOR_ID: Byte = Shulker);

static COVERED_ARMOR_MODIFIER_ID: LazyLock<Identifier> = LazyLock::new(|| Identifier::with_default_namespace("covered").expect("valid id"));
const NO_COLOR: i8 = 16;
const PEEK_PER_TICK: f32 = 0.05;
const DEFAULT_ATTACH_FACE: Direction = Direction::Down;

fn covered_armor_modifier() -> AttributeModifier {
    AttributeModifier { id: COVERED_ARMOR_MODIFIER_ID.clone(), amount: 20.0, operation: AttributeModifierOperation::AddValue }
}

/// `Shulker` (an `AbstractGolem` and `Enemy`).
pub struct Shulker {
    pub mob: MobData,
    current_peek_amount_o: f32,
    current_peek_amount: f32,
}

impl Shulker {
    /// `new Shulker(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::PathfinderMob, |b| {
            b.define(DATA_ATTACH_FACE_ID, DEFAULT_ATTACH_FACE);
            b.define(DATA_PEEK_ID, 0);
            b.define(DATA_COLOR_ID, 16);
        });
        let mut shulker = Shulker { mob, current_peek_amount_o: 0.0, current_peek_amount: 0.0 };
        shulker.mob.enemy = true;
        let navigation = StandardPathNavigation::ground(&shulker);
        shulker.mob.set_navigation(Box::new(navigation));
        shulker.mob.body_rotation_control = Some(Box::new(ShulkerBodyRotationControl::default()));
        shulker.register_goals();
        shulker.mob.xp_reward = 5;
        shulker.mob.look_control = Some(Box::new(ShulkerLookControl::default()));
        shulker.mob.living.entity.delta_movement_frozen = true;
        shulker
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::build(self, LookAtType::Player, 8.0, 0.02, true);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(look_at));
        goals.add_goal(4, Box::new(ShulkerAttackGoal { attack_time: 0 }));
        goals.add_goal(7, Box::new(ShulkerPeekGoal { peek_time: 0 }));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(HurtByTargetGoal::new(vec![class_of::<Shulker>()]).set_alert_others(Vec::new())));
        targets.add_goal(2, Box::new(ShulkerNearestAttackGoal::new()));
        targets.add_goal(3, Box::new(ShulkerDefenseAttackGoal::new()));
    }

    /// `getAttachFace()`.
    pub fn get_attach_face(&self) -> Direction {
        self.mob.living.entity.entity_data.get(DATA_ATTACH_FACE_ID)
    }

    /// `setAttachFace(attachmentDirection)`.
    fn set_attach_face(&mut self, attachment_direction: Direction) {
        if self.mob.living.entity.entity_data.set(DATA_ATTACH_FACE_ID, attachment_direction) {
            self.on_synced_data_updated(DATA_ATTACH_FACE_ID.id());
        }
    }

    /// `getRawPeekAmount()`.
    fn get_raw_peek_amount(&self) -> i32 {
        i32::from(self.mob.living.entity.entity_data.get(DATA_PEEK_ID))
    }

    /// `setRawPeekAmount(amount)` (server side).
    fn set_raw_peek_amount(&mut self, level: &mut ServerLevel, amount: i32) {
        if let Some(armor) = self.mob.living.attributes.get_instance(Attribute::ARMOR) {
            armor.remove_modifier(&COVERED_ARMOR_MODIFIER_ID);
            if amount == 0 {
                armor.add_permanent_modifier(covered_armor_modifier());
            }
        }
        if amount == 0 {
            self.play_sound(level, SoundEvent::ENTITY_SHULKER_CLOSE, 1.0, 1.0);
            self.game_event(level, GameEvent::CONTAINER_CLOSE);
        } else {
            self.play_sound(level, SoundEvent::ENTITY_SHULKER_OPEN, 1.0, 1.0);
            self.game_event(level, GameEvent::CONTAINER_OPEN);
        }
        self.mob.living.entity.entity_data.set(DATA_PEEK_ID, amount as i8);
    }

    /// `isClosed()`.
    fn is_closed(&self) -> bool {
        self.get_raw_peek_amount() == 0
    }

    /// `getScale()` (sanitized to `MAX_SCALE`).
    fn get_scale(&self) -> f32 {
        (self.mob.living.get_attribute_value(Attribute::SCALE) as f32).min(3.0)
    }

    /// `getColor()`.
    pub fn get_color(&self) -> Option<mc_protocol::components::DyeColor> {
        let color = self.mob.living.entity.entity_data.get(DATA_COLOR_ID);
        (color != NO_COLOR && color <= 15).then(|| mc_game_core::world::item::dye_color::by_id(i32::from(color)))
    }

    /// `setVariant(color)`.
    pub fn set_variant(&mut self, color: Option<mc_protocol::components::DyeColor>) {
        let value = color.map_or(NO_COLOR, |c| c as i8);
        self.mob.living.entity.entity_data.set(DATA_COLOR_ID, value);
    }

    /// `getPhysicalPeek(amount)`.
    fn get_physical_peek(amount: f32) -> f32 {
        0.5 - mth::sin(f64::from((0.5 + amount) * std::f32::consts::PI)) * 0.5
    }

    /// `getProgressAabb(size, direction, progressTo, position)`.
    pub fn get_progress_aabb(size: f32, direction: Direction, progress_to: f32, position: Vec3) -> AABB {
        Self::get_progress_delta_aabb(size, direction, -1.0, progress_to, position)
    }

    /// `getProgressDeltaAabb(size, direction, progressFrom, progressTo, position)`.
    pub fn get_progress_delta_aabb(size: f32, direction: Direction, progress_from: f32, progress_to: f32, position: Vec3) -> AABB {
        let size_d = f64::from(size);
        let bounds_at_bottom_center = AABB::new(f64::from(-size) * 0.5, 0.0, f64::from(-size) * 0.5, size_d * 0.5, size_d, size_d * 0.5);
        let max_movement = f64::from(progress_from.max(progress_to));
        let min_movement = f64::from(progress_from.min(progress_to));
        let aabb = bounds_at_bottom_center
            .expand_towards(f64::from(direction.get_step_x()) * max_movement * size_d, f64::from(direction.get_step_y()) * max_movement * size_d, f64::from(direction.get_step_z()) * max_movement * size_d)
            .contract(
                f64::from(-direction.get_step_x()) * (1.0 + min_movement) * size_d,
                f64::from(-direction.get_step_y()) * (1.0 + min_movement) * size_d,
                f64::from(-direction.get_step_z()) * (1.0 + min_movement) * size_d,
            );
        aabb.move_(position.x, position.y, position.z)
    }

    /// `updatePeekAmount()`.
    fn update_peek_amount(&mut self) -> bool {
        self.current_peek_amount_o = self.current_peek_amount;
        let target_peek_amount = self.get_raw_peek_amount() as f32 * 0.01;
        if self.current_peek_amount == target_peek_amount {
            false
        } else {
            if self.current_peek_amount > target_peek_amount {
                self.current_peek_amount = mth::clamp_f32(self.current_peek_amount - PEEK_PER_TICK, target_peek_amount, 1.0);
            } else {
                self.current_peek_amount = mth::clamp_f32(self.current_peek_amount + PEEK_PER_TICK, 0.0, target_peek_amount);
            }
            true
        }
    }

    /// `onPeekAmountChange()`.
    fn on_peek_amount_change(&mut self, level: &mut ServerLevel) {
        self.reapply_position();
        self.refresh_dimensions();
        let physical_peek = Self::get_physical_peek(self.current_peek_amount);
        let physical_peek_old = Self::get_physical_peek(self.current_peek_amount_o);
        let direction = self.get_attach_face().get_opposite();
        let push = (physical_peek - physical_peek_old) * self.get_scale();
        if !(push <= 0.0) {
            let area = Self::get_progress_delta_aabb(self.get_scale(), direction, physical_peek_old, physical_peek, self.position());
            let candidates = level.get_entities(Some(self.id()), &area, |e| !e.is_spectator());
            for id in candidates {
                let eligible = level.get_entity(id).is_some_and(|e| !self.is_passenger_of_same_vehicle(level, e) && e.get_type() != EntityType::SHULKER && !e.base().no_physics);
                if eligible {
                    let delta = Vec3::new(f64::from(push) * f64::from(direction.get_step_x()), f64::from(push) * f64::from(direction.get_step_y()), f64::from(push) * f64::from(direction.get_step_z()));
                    level.with_entity(id, |level, entity| mc_game_core::world::entity::movement::move_entity(entity, level, MoverType::Shulker, delta));
                }
            }
        }
    }

    /// `findNewAttachment()`.
    fn find_new_attachment(&mut self, level: &mut ServerLevel) {
        match self.find_attachable_surface(level, self.base().block_position()) {
            Some(direction) => self.set_attach_face(direction),
            None => {
                self.teleport_somewhere(level);
            }
        }
    }

    /// `findAttachableSurface(target)`.
    fn find_attachable_surface(&self, level: &ServerLevel, target: BlockPos) -> Option<Direction> {
        Direction::ALL.into_iter().find(|direction| self.can_stay_at(level, target, *direction))
    }

    /// `canStayAt(target, face)`.
    fn can_stay_at(&self, level: &ServerLevel, target: BlockPos, face: Direction) -> bool {
        if self.is_position_blocked(level, target) {
            return false;
        }
        let opposite_face = face.get_opposite();
        if !loaded_and_entity_can_stand_on_face(level, target.relative(face), self, opposite_face) {
            return false;
        }
        let fully_opened = Self::get_progress_aabb(self.get_scale(), opposite_face, 1.0, Vec3::at_bottom_center_of(&target)).deflate_scalar(1.0E-6);
        level.no_collision(Some(self.id()), Some(&mc_game_core::world::entity::movement::collision_context(self)), &fully_opened)
    }

    /// `isPositionBlocked(target)`.
    fn is_position_blocked(&self, level: &ServerLevel, target: BlockPos) -> bool {
        let state = level.get_block_state(target);
        if state.is_air() {
            false
        } else {
            let moving_piston_in_our_current_position = state.block() == Block::MOVING_PISTON && target == self.base().block_position();
            !moving_piston_in_our_current_position
        }
    }

    /// `teleportSomewhere()`.
    pub fn teleport_somewhere(&mut self, level: &mut ServerLevel) -> bool {
        if self.mob.is_no_ai() || !self.is_alive() {
            return false;
        }
        let old_pos = self.base().block_position();
        for _ in 0..5 {
            let random = &mut self.mob.living.entity.random;
            let (dx, dy, dz) = (random_between_inclusive(random, -8, 8), random_between_inclusive(random, -8, 8), random_between_inclusive(random, -8, 8));
            let target = old_pos.offset(dx, dy, dz);
            if target.y() > level.get_min_y()
                && level.is_empty_block(target)
                && level.world_border().is_within_bounds_block(&target)
                && level.no_collision(
                    Some(self.id()),
                    Some(&mc_game_core::world::entity::movement::collision_context(self)),
                    &AABB::unit_cube_from_lower_corner(Vec3::new(f64::from(target.x()), f64::from(target.y()), f64::from(target.z()))).deflate_scalar(1.0E-6),
                )
            {
                if let Some(attachment_direction) = self.find_attachable_surface(level, target)
                    && !level.get_block_state(target.relative(attachment_direction)).block().is_in_tag("shulker_does_not_teleport_to")
                {
                    level.un_ride(self);
                    self.set_attach_face(attachment_direction);
                    self.play_sound(level, SoundEvent::ENTITY_SHULKER_TELEPORT, 1.0, 1.0);
                    self.set_pos(f64::from(target.x()) + 0.5, f64::from(target.y()), f64::from(target.z()) + 0.5);
                    level.game_event_from_entity_ref(&*self, GameEvent::TELEPORT, Vec3::new(f64::from(old_pos.x()), f64::from(old_pos.y()), f64::from(old_pos.z())));
                    let packed_diff = mc_game_core::util::block_util::pack_difference_in_position(old_pos, self.base().block_position(), 8, 8, 8).unwrap_or(0);
                    level.level_event(None, 2016, old_pos, packed_diff);
                    self.mob.living.entity.entity_data.set(DATA_PEEK_ID, 0);
                    self.set_target(level, None);
                    return true;
                }
            }
        }
        false
    }

    /// `hitByShulkerBullet()`.
    fn hit_by_shulker_bullet(&mut self, level: &mut ServerLevel) {
        let old_position = self.position();
        let old_aabb = self.get_bounding_box();
        if !self.is_closed() && self.teleport_somewhere(level) {
            let query = old_aabb.inflate_scalar(8.0);
            // `this` is out of the level while it runs: it counts as one of the nearby shulkers when it is still in range
            let mut shulker_count = level.get_entities_of_class(&query, |e| e.get_type() == EntityType::SHULKER, |e| e.is_alive()).len() as i32;
            if self.is_alive() && self.get_bounding_box().intersects_aabb(&query) {
                shulker_count += 1;
            }
            let failure_chance = (shulker_count - 1) as f32 / 5.0;
            if !(level.tick_state.random.next_float() < failure_chance)
                && let Some(mut baby) = mc_game_core::world::entity::factory::create(EntityType::SHULKER, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
            {
                if let Some(shulker) = baby.as_any_mut().downcast_mut::<Shulker>() {
                    shulker.set_variant(self.get_color());
                }
                let (y_rot, x_rot) = (baby.base().y_rot(), baby.base().x_rot());
                baby.snap_to(old_position.x, old_position.y, old_position.z, y_rot, x_rot);
                level.add_fresh_entity(baby);
            }
        }
    }
}

/// `random.nextInt(max - min + 1) + min` (`Mth.randomBetweenInclusive`).
fn random_between_inclusive(random: &mut impl RandomSource, min: i32, max: i32) -> i32 {
    random.next_int_bounded(max - min + 1) + min
}

/// `Level.loadedAndEntityCanStandOnFace(pos, entity, faceDirection)`.
fn loaded_and_entity_can_stand_on_face(level: &ServerLevel, pos: BlockPos, entity: &dyn Entity, face_direction: Direction) -> bool {
    if !level.is_in_valid_bounds(pos) {
        return false;
    }
    if !mc_game_core::world::level::LevelReader::has_chunk(level, pos.x() >> 4, pos.z() >> 4) {
        return false;
    }
    let state = level.get_block_state(pos);
    // `BlockState.entityCanStandOnFace(level, pos, entity, direction)`
    is_face_full(&state.get_collision_shape_ctx(level, pos, &mc_game_core::world::entity::movement::collision_context(entity)), face_direction)
}

impl Entity for Shulker {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SHULKER_AMBIENT)
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        120
    }

    fn play_ambient_sound(&mut self, level: &mut ServerLevel) {
        if !self.is_closed() {
            mob::tick::play_ambient_sound(self, level);
        }
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SHULKER_DEATH)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(if self.is_closed() { SoundEvent::ENTITY_SHULKER_HURT_CLOSED } else { SoundEvent::ENTITY_SHULKER_HURT })
    }

    fn is_enemy(&self) -> bool {
        true
    }

    fn remove_when_far_away(&self, _distance_to_closest_player: f64) -> bool {
        false
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_byte("AttachFace", self.get_attach_face().get_3d_data_value() as i8);
        out.put_byte("Peek", self.mob.living.entity.entity_data.get(DATA_PEEK_ID));
        out.put_byte("Color", self.mob.living.entity.entity_data.get(DATA_COLOR_ID));
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        let face = input.get_byte("AttachFace").map_or(DEFAULT_ATTACH_FACE, |b| Direction::from_3d_data_value(i32::from(b)));
        self.set_attach_face(face);
        self.mob.living.entity.entity_data.set(DATA_PEEK_ID, input.get_byte_or("Peek", 0));
        self.mob.living.entity.entity_data.set(DATA_COLOR_ID, input.get_byte_or("Color", 16));
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if !self.base().is_passenger() && !self.can_stay_at(level, self.base().block_position(), self.get_attach_face()) {
            self.find_new_attachment(level);
        }
        if self.update_peek_amount() {
            self.on_peek_amount_change(level);
        }
    }

    fn make_bounding_box(&self, position: Vec3) -> AABB {
        let phys_peek = Self::get_physical_peek(self.current_peek_amount);
        let direction = self.get_attach_face().get_opposite();
        Self::get_progress_aabb(self.get_scale(), direction, phys_peek, position)
    }

    fn get_dimensions(&self, pose: Pose) -> EntityDimensions {
        let mut dimension = EntityDimensions::of_type(self.mob.living.entity.entity_type);
        if self.get_attach_face() == Direction::Down && self.current_peek_amount > 0.0 {
            let height_scale_factor = 1.0 + self.current_peek_amount;
            dimension = dimension.scale_xy(1.0, height_scale_factor);
        }
        mob::ext::living_dimensions(self, pose, dimension)
    }

    fn before_start_riding(&mut self) {
        self.set_attach_face(Direction::Down);
    }

    fn on_vehicle_removed(&mut self, _level: &mut ServerLevel, _old_vehicle: EntityId, _held: Option<&dyn Entity>) {
        self.mob.living.y_body_rot_o = 0.0;
        self.mob.living.y_body_rot = 0.0;
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        self.set_y_rot(0.0);
        let y_rot = self.base().y_rot();
        self.mob.living.y_head_rot = y_rot;
        self.base_mut().set_old_pos_and_rot();
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn on_moved_by_shulker_box(&mut self, level: &mut ServerLevel) -> bool {
        self.teleport_somewhere(level);
        true
    }

    fn set_pos(&mut self, x: f64, y: f64, z: f64) {
        let old_pos = self.base().block_position();
        if self.base().is_passenger() {
            self.base_mut().set_pos_raw(x, y, z);
        } else {
            self.base_mut().set_pos_raw(f64::from(mth::floor_f64(x)) + 0.5, f64::from(mth::floor_f64(y + 0.5)), f64::from(mth::floor_f64(z)) + 0.5);
        }
        let position = self.base().position();
        let bb = self.make_bounding_box(position);
        self.base_mut().set_bounding_box(bb);
        if self.base().tick_count != 0 {
            let pos = self.base().block_position();
            if pos != old_pos {
                self.mob.living.entity.entity_data.set(DATA_PEEK_ID, 0);
                self.base_mut().needs_sync = true;
            }
        }
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_closed()
            && source.direct_info().is_some_and(|d| matches!(d.entity_type, EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT))
        {
            return false;
        }
        if !mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage) {
            return false;
        }
        if f64::from(self.mob.living.get_health()) < f64::from(self.mob.living.get_max_health()) * 0.5 && self.base_mut().random.next_int_bounded(4) == 0 {
            self.teleport_somewhere(level);
        } else if source.is_tag(level, "is_projectile") && source.direct_info().is_some_and(|d| d.entity_type == EntityType::SHULKER_BULLET) {
            self.hit_by_shulker_bullet(level);
        }
        true
    }

    fn can_be_collided_with(&self, _other: Option<&dyn Entity>) -> bool {
        self.is_alive()
    }

    fn on_synced_data_updated(&mut self, accessor_id: u8) {
        if accessor_id == DATA_ATTACH_FACE_ID.id() {
            let position = self.base().position();
            let bb = self.make_bounding_box(position);
            self.base_mut().set_bounding_box(bb);
        }
        if accessor_id == mc_game_core::world::entity::entity::DATA_POSE.id() {
            self.refresh_dimensions();
        }
    }

    fn get_max_head_x_rot(&self) -> i32 {
        180
    }

    fn get_max_head_y_rot(&self) -> i32 {
        180
    }

    fn push_entity(&mut self, _level: &mut ServerLevel, _other: &mut dyn Entity) {}

    fn entity_components(&self) -> DataComponentMap {
        let base = mc_game_core::advancements::predicates::entity::support::base_entity_components(self.base());
        let builder = DataComponentMap::builder().add_all(&base);
        match self.get_color() {
            Some(color) => builder.set(SHULKER_COLOR, color).build(),
            None => builder.build(),
        }
    }

    fn apply_implicit_components(&mut self, stack: &ItemStack) {
        if let Some(color) = stack.get(&SHULKER_COLOR) {
            self.set_variant(Some(*color));
        }
        mc_game_core::world::entity::spawn_config::apply_base_implicit_components(self, stack);
    }
}

fn shulker_ref(mob: &dyn Entity) -> &Shulker {
    mob.as_any().downcast_ref::<Shulker>().expect("Shulker")
}

fn shulker_mut(mob: &mut dyn Entity) -> &mut Shulker {
    mob.as_any_mut().downcast_mut::<Shulker>().expect("Shulker")
}

/// `getTargetSearchArea(followDistance)` of `ShulkerNearestAttackGoal` / `ShulkerDefenseAttackGoal`.
fn shulker_target_search_area(mob: &dyn Entity, follow_distance: f64) -> AABB {
    let attach_face = shulker_ref(mob).get_attach_face();
    let bb = mob.base().bounding_box();
    if attach_face.get_axis() == Axis::X {
        bb.inflate(4.0, follow_distance, follow_distance)
    } else if attach_face.get_axis() == Axis::Z {
        bb.inflate(follow_distance, follow_distance, 4.0)
    } else {
        bb.inflate(follow_distance, 4.0, follow_distance)
    }
}

/// `Shulker.ShulkerAttackGoal`.
struct ShulkerAttackGoal {
    attack_time: i32,
}

impl Goal for ShulkerAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let target = mob.get_target().and_then(|id| level.get_entity(id));
        target.is_some_and(|t| t.is_alive()) && level.difficulty() != Difficulty::Peaceful
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack_time = 20;
        shulker_mut(mob).set_raw_peek_amount(level, 100);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        shulker_mut(mob).set_raw_peek_amount(level, 0);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if level.difficulty() == Difficulty::Peaceful {
            return;
        }
        self.attack_time -= 1;
        let Some(target_id) = mob.get_target() else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        mob::look_at_entity_full(mob, target, 180.0, 180.0);
        let distance = mob.base().distance_to_sqr_vec(target.position());
        if distance < 400.0 {
            if self.attack_time <= 0 {
                self.attack_time = 20 + mob.base_mut().random.next_int_bounded(10) * 20 / 2;
                let axis = shulker_ref(mob).get_attach_face().get_axis();
                let Some(target) = level.get_entity(target_id) else { return };
                let bullet = ShulkerBullet::from_owner(level, mob, target, axis);
                level.add_fresh_entity(Box::new(bullet));
                let random = &mut mob.base_mut().random;
                let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
                mob.play_sound(level, SoundEvent::ENTITY_SHULKER_SHOOT, 2.0, pitch);
            }
        } else {
            mob.set_target(level, None);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `Shulker.ShulkerBodyRotationControl`.
#[derive(Default)]
struct ShulkerBodyRotationControl {
    base: BodyRotationControlBase,
}

impl BodyRotationControl for ShulkerBodyRotationControl {
    fn base(&self) -> &BodyRotationControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BodyRotationControlBase {
        &mut self.base
    }
    fn client_tick(&mut self, _mob: &mut dyn Entity) {}
}

/// `Shulker.ShulkerDefenseAttackGoal` (`NearestAttackableTargetGoal<LivingEntity>` of enemies).
struct ShulkerDefenseAttackGoal {
    target: NearestAttackableTargetGoal,
}

impl ShulkerDefenseAttackGoal {
    fn new() -> Self {
        let mut target = NearestAttackableTargetGoal::build(
            TargetType::Class(Arc::new(|e: &dyn Entity| e.living().is_some())),
            10,
            true,
            false,
            Some(Arc::new(|input: &dyn Entity, _level: &ServerLevel| input.mob().is_some_and(|m| m.enemy))),
        );
        target.search_area = Some(Box::new(shulker_target_search_area));
        Self { target }
    }
}

impl Goal for ShulkerDefenseAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob::combat::get_team_name(level, mob).is_none() { false } else { self.target.can_use_impl(mob, level) }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.target.flags()
    }
}

/// `Shulker.ShulkerLookControl`.
#[derive(Default)]
struct ShulkerLookControl {
    base: LookControlBase,
}

/// A `Quaternionf` as `(x, y, z, w)`.
type Quat = (f32, f32, f32, f32);

/// `new Quaternionf().rotationXYZ(angleX, angleY, angleZ)` (JOML).
fn rotation_xyz(angle_x: f32, angle_y: f32, angle_z: f32) -> Quat {
    let half = |a: f32| {
        let h = a * 0.5;
        (f64::from(h).sin() as f32, f64::from(h).cos() as f32)
    };
    let ((sx, cx), (sy, cy), (sz, cz)) = (half(angle_x), half(angle_y), half(angle_z));
    let (cycz, sysz, sycz, cysz) = (cy * cz, sy * sz, sy * cz, cy * sz);
    (sx * cycz + cx * sysz, cx * sycz - sx * cysz, cx * cysz + sx * sycz, cx * cycz - sx * sysz)
}

/// `Direction.getRotation()`.
fn direction_rotation(direction: Direction) -> Quat {
    let pi = std::f32::consts::PI;
    match direction {
        Direction::Down => rotation_xyz(pi, 0.0, 0.0),
        Direction::Up => (0.0, 0.0, 0.0, 1.0),
        Direction::North => rotation_xyz(pi / 2.0, 0.0, pi),
        Direction::South => rotation_xyz(pi / 2.0, 0.0, 0.0),
        Direction::West => rotation_xyz(pi / 2.0, 0.0, pi / 2.0),
        Direction::East => rotation_xyz(pi / 2.0, 0.0, -pi / 2.0),
    }
}

/// `quaternion.transform(new Vector3f(0, 0, 1))` (JOML `Quaternionf.transform(Vector3f)`).
fn transform_forward((x, y, z, w): Quat) -> (f32, f32, f32) {
    let (w2, x2, y2, z2) = (w * w, x * x, y * y, z * z);
    let (zw, xy, xz, yw, yz, xw) = (z * w, x * y, x * z, y * w, y * z, x * w);
    let k = 1.0 / (w2 + x2 + y2 + z2);
    (2.0 * (xz + yw) * k, 2.0 * (yz - xw) * k, (z2 - x2 - y2 + w2) * k)
}

impl LookControl for ShulkerLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn clamp_head_rotation_to_body(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {}

    fn get_y_rot_d(&self, mob: &dyn Entity) -> Option<f32> {
        let attach_face = shulker_ref(mob).get_attach_face().get_opposite();
        let forward = transform_forward(direction_rotation(attach_face));
        let up_normal = attach_face.get_unit_vec3i();
        let right = (up_normal.x as f32, up_normal.y as f32, up_normal.z as f32);
        // `right.cross(forward)`
        let right = (right.1 * forward.2 - right.2 * forward.1, right.2 * forward.0 - right.0 * forward.2, right.0 * forward.1 - right.1 * forward.0);
        let b = self.base();
        let xd = b.wanted_x - mob.base().x();
        let yd = b.wanted_y - mob.base().get_eye_y();
        let zd = b.wanted_z - mob.base().z();
        let out = (xd as f32, yd as f32, zd as f32);
        let delta_right = right.0.mul_add(out.0, right.1.mul_add(out.1, right.2 * out.2));
        let delta_forward = forward.0.mul_add(out.0, forward.1.mul_add(out.1, forward.2 * out.2));
        if !(delta_right.abs() > 1.0E-5) && !(delta_forward.abs() > 1.0E-5) {
            None
        } else {
            Some((mth::atan2(f64::from(-delta_right), f64::from(delta_forward)) * f64::from(180.0f32) / f64::from(std::f32::consts::PI)) as f32)
        }
    }

    fn get_x_rot_d(&self, _mob: &dyn Entity) -> Option<f32> {
        Some(0.0)
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        look_control::default_tick(self, mob, level);
    }
}

/// `Shulker.ShulkerNearestAttackGoal` (`NearestAttackableTargetGoal<Player>`).
struct ShulkerNearestAttackGoal {
    target: NearestAttackableTargetGoal,
}

impl ShulkerNearestAttackGoal {
    fn new() -> Self {
        let mut target = NearestAttackableTargetGoal::new(TargetType::Player, true);
        target.search_area = Some(Box::new(shulker_target_search_area));
        Self { target }
    }
}

impl Goal for ShulkerNearestAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if level.difficulty() == Difficulty::Peaceful { false } else { self.target.can_use_impl(mob, level) }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.target.flags()
    }
}

/// `Shulker.ShulkerPeekGoal`.
struct ShulkerPeekGoal {
    peek_time: i32,
}

impl Goal for ShulkerPeekGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob.get_target().is_none()
            && mob.base_mut().random.next_int_bounded(reduced_tick_delay(40)) == 0
            && shulker_ref(mob).can_stay_at(level, mob.base().block_position(), shulker_ref(mob).get_attach_face())
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.get_target().is_none() && self.peek_time > 0
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.peek_time = self.adjusted_tick_delay(20 * (1 + mob.base_mut().random.next_int_bounded(3)));
        shulker_mut(mob).set_raw_peek_amount(level, 30);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if mob.get_target().is_none() {
            shulker_mut(mob).set_raw_peek_amount(level, 0);
        }
    }

    fn tick(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.peek_time -= 1;
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SHULKER: EntityRegistration = EntityRegistration { entity_type: EntityType::SHULKER, create: |ty, id, registries| Box::new(Shulker::new(ty, id, registries)) };
