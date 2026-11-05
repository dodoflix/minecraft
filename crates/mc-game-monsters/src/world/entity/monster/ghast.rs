//! `net.minecraft.world.entity.monster.Ghast` (+ the shared `GhastMoveControl`, `RandomFloatAroundGoal`, `GhastLookGoal` and
//! `faceMovementDirection` the happy ghast reuses).

use mc_game_core::define_accessor;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::control::move_control::{MoveControl, MoveControlBase, Operation};
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::inside_block_effects::{collided_with_fluid, collided_with_shape_moving_from};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::projectile::hurting::large_fireball::LargeFireball;
use mc_game_core::world::entity::spawn_predicates::check_mob_spawn_rules;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::collision::for_each_block_intersected_between;
use mc_game_core::world::level::material::FluidStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter, LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
use mc_world::heightmap::HeightmapType;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

define_accessor!(pub DATA_IS_CHARGING: Boolean = Ghast);

const DEFAULT_EXPLOSION_POWER: i32 = 1;

/// `Ghast`.
pub struct Ghast {
    pub mob: MobData,
    explosion_power: i32,
}

impl Ghast {
    /// `new Ghast(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mut mob = MobData::new(ty, id, MobKind::Mob, |b| {
            b.define(DATA_IS_CHARGING, false);
        });
        mob.enemy = true;
        let mut ghast = Ghast { mob, explosion_power: DEFAULT_EXPLOSION_POWER };
        let navigation = StandardPathNavigation::ground(&ghast);
        ghast.mob.set_navigation(Box::new(navigation));
        ghast.mob.xp_reward = 5;
        ghast.mob.move_control = Some(Box::new(GhastMoveControl::new(false, Arc::new(|_| false))));
        ghast.register_goals();
        ghast
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(5, Box::new(RandomFloatAroundGoal::new(0)));
        goals.add_goal(7, Box::new(GhastLookGoal));
        goals.add_goal(7, Box::new(GhastShootFireballGoal { charge_time: 0 }));
        self.mob.target_selector.add_goal(1, Box::new(GhastTargetGoal::new()));
    }

    /// `isCharging()`.
    pub fn is_charging(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_IS_CHARGING)
    }

    /// `setCharging(onOff)`.
    pub fn set_charging(&mut self, on_off: bool) {
        self.mob.living.entity.entity_data.set(DATA_IS_CHARGING, on_off);
    }

    /// `getExplosionPower()`.
    pub fn get_explosion_power(&self) -> i32 {
        self.explosion_power
    }
}

/// `Ghast.isReflectedFireball(source)`.
fn is_reflected_fireball(source: &DamageSource) -> bool {
    source.direct_info().is_some_and(|d| d.entity_type == EntityType::FIREBALL) && source.causing_is_player()
}

impl Entity for Ghast {
    mc_game_core::mob_entity_accessors!(mob);

    fn is_invulnerable_to(&mut self, level: &mut ServerLevel, source: &DamageSource) -> bool {
        (self.is_invulnerable() && !source.is_tag(level, "bypasses_invulnerability"))
            || (!is_reflected_fireball(source) && mc_game_core::world::entity::living_entity::damage::is_invulnerable_to(self, level, source))
    }

    fn check_fall_damage(&mut self, _level: &mut ServerLevel, _ya: f64, _on_ground: bool, _on_state: mc_data::block::BlockState, _pos: BlockPos) {}

    fn on_climbable(&self, _level: &ServerLevel) -> bool {
        false
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        mc_game_core::world::entity::living_entity::travel::travel_flying(self, level, input, 0.02);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if is_reflected_fireball(source) {
            mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, 1000.0);
            true
        } else if self.is_invulnerable_to(level, source) {
            false
        } else {
            mc_game_core::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
        }
    }

    fn get_sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GHAST_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GHAST_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_GHAST_DEATH)
    }

    fn get_sound_volume(&self) -> f32 {
        5.0
    }

    fn get_max_spawn_cluster_size(&self) -> i32 {
        1
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_byte("ExplosionPower", self.explosion_power as i8);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.explosion_power = i32::from(input.get_byte_or("ExplosionPower", DEFAULT_EXPLOSION_POWER as i8));
    }

    fn support_quad_leash_as_holder(&self) -> bool {
        true
    }

    fn leash_elastic_distance(&self) -> f64 {
        10.0
    }

    fn leash_snap_distance(&self) -> f64 {
        16.0
    }
}

/// `Ghast.checkGhastSpawnRules`.
fn check_ghast_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    level.get_difficulty() != Difficulty::Peaceful && random.next_int_bounded(20) == 0 && check_mob_spawn_rules(ty, level, reason, pos, random)
}

register_spawn_placement!(EntityType::GHAST, check_ghast_spawn_rules);

/// `Ghast.faceMovementDirection(ghast)`.
pub fn face_movement_direction(ghast: &mut dyn Entity, level: &ServerLevel) {
    let pi = std::f32::consts::PI;
    match mob::tick::get_target_checked(ghast, level).and_then(|t| level.get_entity(t)) {
        None => {
            let movement = ghast.base().delta_movement();
            ghast.base_mut().set_y_rot(-(mth::atan2(movement.x, movement.z) as f32) * (180.0 / pi));
            let y_rot = ghast.base().y_rot();
            ghast.living_mut().expect("living").y_body_rot = y_rot;
        }
        Some(target) => {
            if target.base().distance_to_sqr_vec(ghast.position()) < 4096.0 {
                let xdd = target.base().x() - ghast.base().x();
                let zdd = target.base().z() - ghast.base().z();
                ghast.base_mut().set_y_rot(-(mth::atan2(xdd, zdd) as f32) * (180.0 / pi));
                let y_rot = ghast.base().y_rot();
                ghast.living_mut().expect("living").y_body_rot = y_rot;
            }
        }
    }
}

/// `Ghast.GhastLookGoal`.
pub struct GhastLookGoal;

impl Goal for GhastLookGoal {
    fn can_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        face_movement_direction(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look])
    }
}

/// `Ghast.GhastMoveControl`; `should_be_stopped` is the `BooleanSupplier` (reads the controlled mob).
pub struct GhastMoveControl {
    base: MoveControlBase,
    float_duration: i32,
    careful: bool,
    should_be_stopped: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>,
}

impl GhastMoveControl {
    pub fn new(careful: bool, should_be_stopped: Arc<dyn Fn(&dyn Entity) -> bool + Send + Sync>) -> Self {
        Self { base: MoveControlBase::default(), float_duration: 0, careful, should_be_stopped }
    }

    /// `canReach(travel)`.
    fn can_reach(&self, mob: &dyn Entity, level: &ServerLevel, travel: Vec3) -> bool {
        let aabb = mob.base().bounding_box();
        let aabb_at_destination = aabb.move_vec(travel);
        if self.careful {
            for pos in BlockPos::between_closed_aabb(&aabb_at_destination.inflate_scalar(1.0)) {
                if !self.block_traversal_possible(mob, level, None, None, pos, false, false) {
                    return false;
                }
            }
        }
        let is_in_water = mob.is_in_water();
        let is_in_lava = mob.is_in_lava();
        let start = mob.position();
        let end = start.add(travel);
        for_each_block_intersected_between(start, end, &aabb_at_destination, &mut |block_pos: BlockPos, _i: i32| {
            if aabb.intersects_block_pos(&block_pos) { true } else { self.block_traversal_possible(mob, level, Some(start), Some(end), block_pos, is_in_water, is_in_lava) }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn block_traversal_possible(&self, mob: &dyn Entity, level: &ServerLevel, start: Option<Vec3>, end: Option<Vec3>, pos: BlockPos, can_path_through_water: bool, can_path_through_lava: bool) -> bool {
        let state = level.get_block_state(pos);
        if state.is_air() {
            return true;
        }
        let precise_block_collisions = start.is_some() && end.is_some();
        let path_no_collisions = if let (Some(start), Some(end)) = (start, end) {
            !collided_with_shape_moving_from(mob, start, end, &state.get_collision_shape(level, pos).move_vec(Vec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()))).to_aabbs())
        } else {
            state.get_collision_shape(level, pos).is_empty()
        };
        if !self.careful {
            return path_no_collisions;
        }
        if state.is_tag("happy_ghast_avoids") {
            return false;
        }
        let fluid_state = level.get_fluid_state(pos);
        if !fluid_state.is_empty() && (!precise_block_collisions || collided_with_fluid(mob, level, fluid_state, pos, start.expect("start"), end.expect("end"))) {
            if fluid_state.is_tag("water") {
                return can_path_through_water;
            }
            if fluid_state.is_tag("lava") {
                return can_path_through_lava;
            }
        }
        path_no_collisions
    }
}

impl MoveControl for GhastMoveControl {
    fn base(&self) -> &MoveControlBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MoveControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if (self.should_be_stopped)(mob) {
            self.base.operation = Operation::Wait;
            mob::tick::stop_in_place(mob, level);
        }
        if self.base.operation == Operation::MoveTo {
            let previous = self.float_duration;
            self.float_duration -= 1;
            if previous <= 0 {
                self.float_duration = self.float_duration + mob.base_mut().random.next_int_bounded(5) + 2;
                let travel = Vec3::new(self.base.wanted_x - mob.base().x(), self.base.wanted_y - mob.base().y(), self.base.wanted_z - mob.base().z());
                if self.can_reach(mob, level, travel) {
                    let flying_speed = mob::attribute_value(mob, Attribute::FLYING_SPEED);
                    let movement = mob.base().delta_movement();
                    mob.base_mut().set_delta_movement(movement.add(travel.normalize().scale(flying_speed * 5.0 / 3.0)));
                } else {
                    self.base.operation = Operation::Wait;
                }
            }
        }
    }
}

/// `Ghast`'s `NearestAttackableTargetGoal<>(this, Player.class, 10, true, false, (target, level) -> Math.abs(target.getY() - this.getY()) <= 4.0)`:
/// the selector reads the ghast's current Y, so it is rebuilt from the mob before each use.
struct GhastTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl GhastTargetGoal {
    fn new() -> Self {
        Self { inner: NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None) }
    }

    fn refresh_selector(&mut self, mob: &dyn Entity) {
        let y = mob.base().y();
        self.inner.target_conditions = TargetingConditions::for_combat().selector(Some(Arc::new(move |target: &dyn Entity, _level: &ServerLevel| (target.base().y() - y).abs() <= 4.0)));
    }
}

impl Goal for GhastTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.refresh_selector(mob);
        self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Ghast.GhastShootFireballGoal`.
struct GhastShootFireballGoal {
    charge_time: i32,
}

impl Goal for GhastShootFireballGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level).is_some()
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.charge_time = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(ghast) = mob.as_any_mut().downcast_mut::<Ghast>() {
            ghast.set_charging(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = mob::tick::get_target_checked(mob, level) else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        let in_sight = target.base().distance_to_sqr_vec(mob.position()) < 4096.0 && mob::ext::has_line_of_sight(level, mob, target);
        let (target_x, target_y_half, target_z) = (target.base().x(), target.base().get_y_progress(0.5), target.base().z());
        if in_sight {
            self.charge_time += 1;
            if self.charge_time == 10 && !mob.base().is_silent() {
                level.level_event(None, 1015, mob.block_position(), 0);
            }
            if self.charge_time == 20 {
                let view_vector = mob.get_view_vector(1.0);
                let xdd = target_x - (mob.base().x() + view_vector.x * 4.0);
                let ydd = target_y_half - (0.5 + mob.base().get_y_progress(0.5));
                let zdd = target_z - (mob.base().z() + view_vector.z * 4.0);
                let direction = Vec3::new(xdd, ydd, zdd);
                if !mob.base().is_silent() {
                    level.level_event(None, 1016, mob.block_position(), 0);
                }
                let power = mob.as_any().downcast_ref::<Ghast>().map_or(DEFAULT_EXPLOSION_POWER, Ghast::get_explosion_power);
                let mut fireball = LargeFireball::from_mob(level, mob, direction.normalize(), power);
                let fz = fireball.base().z();
                fireball.set_pos(mob.base().x() + view_vector.x * 4.0, mob.base().get_y_progress(0.5) + 0.5, fz + view_vector.z * 4.0);
                level.add_fresh_entity(Box::new(fireball));
                self.charge_time = -40;
            }
        } else if self.charge_time > 0 {
            self.charge_time -= 1;
        }
        let charging = self.charge_time > 10;
        if let Some(ghast) = mob.as_any_mut().downcast_mut::<Ghast>() {
            ghast.set_charging(charging);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Ghast.RandomFloatAroundGoal`.
pub struct RandomFloatAroundGoal {
    distance_to_blocks: i32,
}

impl RandomFloatAroundGoal {
    const MAX_ATTEMPTS: i32 = 64;

    /// `RandomFloatAroundGoal(ghast, distanceToBlocks)`.
    pub fn new(distance_to_blocks: i32) -> Self {
        Self { distance_to_blocks }
    }

    /// `getSuitableFlyToPosition(mob, distanceToBlocks)`.
    pub fn get_suitable_fly_to_position(mob: &mut dyn Entity, level: &ServerLevel, distance_to_blocks: i32) -> Vec3 {
        let center = mob.position();
        let mut result: Option<Vec3> = None;
        for _ in 0..Self::MAX_ATTEMPTS {
            result = Self::choose_random_position_with_restriction(mob, center);
            if let Some(r) = result
                && Self::is_good_target(level, r, distance_to_blocks)
            {
                return r;
            }
        }
        let mut result = result.unwrap_or_else(|| Self::choose_random_position(mob, center));
        let pos = BlockPos::containing(result.x, result.y, result.z);
        let height_y = level.get_height_at(HeightmapType::MotionBlocking, pos.x(), pos.z());
        if height_y < pos.y() && height_y > level.get_min_y() {
            result = Vec3::new(result.x, mob.base().y() - (mob.base().y() - result.y).abs(), result.z);
        }
        result
    }

    fn is_good_target(level: &ServerLevel, target: Vec3, distance_to_blocks: i32) -> bool {
        if distance_to_blocks <= 0 {
            return true;
        }
        let pos = BlockPos::containing(target.x, target.y, target.z);
        if !level.get_block_state(pos).is_air() {
            return false;
        }
        for dir in Direction::VALUES {
            for i in 1..distance_to_blocks {
                let offset = pos.relative_n(dir, i);
                if !level.get_block_state(offset).is_air() {
                    return true;
                }
            }
        }
        false
    }

    fn choose_random_position(mob: &mut dyn Entity, center: Vec3) -> Vec3 {
        let random = &mut mob.base_mut().random;
        let x_target = center.x + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        let y_target = center.y + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        let z_target = center.z + f64::from((random.next_float() * 2.0 - 1.0) * 16.0);
        Vec3::new(x_target, y_target, z_target)
    }

    fn choose_random_position_with_restriction(mob: &mut dyn Entity, center: Vec3) -> Option<Vec3> {
        let target = Self::choose_random_position(mob, center);
        let m = mob.mob().expect("mob");
        if m.has_home() && !m.is_within_home_vec(target) { None } else { Some(target) }
    }
}

impl Goal for RandomFloatAroundGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        let control = mob.mob().and_then(|m| m.move_control.as_deref()).expect("move control");
        if !control.has_wanted() {
            true
        } else {
            let xd = control.get_wanted_x() - mob.base().x();
            let yd = control.get_wanted_y() - mob.base().y();
            let zd = control.get_wanted_z() - mob.base().z();
            let dd = xd * xd + yd * yd + zd * zd;
            dd < 1.0 || dd > 3600.0
        }
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let result = Self::get_suitable_fly_to_position(mob, level, self.distance_to_blocks);
        mob::set_wanted_position(mob, result.x, result.y, result.z, 1.0);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static GHAST: EntityRegistration = EntityRegistration { entity_type: EntityType::GHAST, create: |ty, id, registries| Box::new(Ghast::new(ty, id, registries)) };
