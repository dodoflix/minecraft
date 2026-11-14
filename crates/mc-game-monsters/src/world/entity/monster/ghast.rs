//! `net.minecraft.world.entity.monster.Ghast` (the move control, the random float goal and `faceMovementDirection` it shares with the
//! happy ghast live in `mc_game_core::world::entity::monster::ghast_movement`).

use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};
use mc_game_core::define_accessor;
use mc_game_core::register_spawn_placement;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::monster::ghast_movement::{
    GhastMoveControl, RandomFloatAroundGoal, face_movement_direction,
};
use mc_game_core::world::entity::projectile::hurting::large_fireball::LargeFireball;
use mc_game_core::world::entity::spawn_predicates::check_mob_spawn_rules;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::SoundSource;
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
        let mut ghast = Ghast {
            mob,
            explosion_power: DEFAULT_EXPLOSION_POWER,
        };
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
        self.mob
            .target_selector
            .add_goal(1, Box::new(GhastTargetGoal::new()));
    }

    /// `isCharging()`.
    pub fn is_charging(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_IS_CHARGING)
    }

    /// `setCharging(onOff)`.
    pub fn set_charging(&mut self, on_off: bool) {
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_IS_CHARGING, on_off);
    }

    /// `getExplosionPower()`.
    pub fn get_explosion_power(&self) -> i32 {
        self.explosion_power
    }
}

/// `Ghast.isReflectedFireball(source)`.
fn is_reflected_fireball(source: &DamageSource) -> bool {
    source
        .direct_info()
        .is_some_and(|d| d.entity_type == EntityType::FIREBALL)
        && source.causing_is_player()
}

impl Entity for Ghast {
    mc_game_core::mob_entity_accessors!(mob);

    fn is_invulnerable_to(&mut self, level: &mut ServerLevel, source: &DamageSource) -> bool {
        (self.is_invulnerable() && !source.is_tag(level, "bypasses_invulnerability"))
            || (!is_reflected_fireball(source)
                && mc_game_core::world::entity::living_entity::damage::is_invulnerable_to(
                    self, level, source,
                ))
    }

    fn check_fall_damage(
        &mut self,
        _level: &mut ServerLevel,
        _ya: f64,
        _on_ground: bool,
        _on_state: mc_data::block::BlockState,
        _pos: BlockPos,
    ) {
    }

    fn on_climbable(&self, _level: &ServerLevel) -> bool {
        false
    }

    fn travel(&mut self, level: &mut ServerLevel, input: Vec3) {
        mc_game_core::world::entity::living_entity::travel::travel_flying(self, level, input, 0.02);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if is_reflected_fireball(source) {
            mc_game_core::world::entity::living_entity::damage::hurt_server(
                self, level, source, 1000.0,
            );
            true
        } else if self.is_invulnerable_to(level, source) {
            false
        } else {
            mc_game_core::world::entity::living_entity::damage::hurt_server(
                self, level, source, damage,
            )
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

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, level, input);
        self.explosion_power =
            i32::from(input.get_byte_or("ExplosionPower", DEFAULT_EXPLOSION_POWER as i8));
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
fn check_ghast_spawn_rules(
    ty: EntityType,
    level: &dyn ServerLevelAccessor,
    reason: EntitySpawnReason,
    pos: BlockPos,
    random: &mut dyn RandomSource,
) -> bool {
    level.get_difficulty() != Difficulty::Peaceful
        && random.next_int_bounded(20) == 0
        && check_mob_spawn_rules(ty, level, reason, pos, random)
}

register_spawn_placement!(EntityType::GHAST, check_ghast_spawn_rules);

/// `Ghast.GhastLookGoal`.
pub struct GhastLookGoal;

impl Goal for GhastLookGoal {
    fn name(&self) -> &'static str {
        "Ghast$GhastLookGoal"
    }

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

/// `Ghast`'s `NearestAttackableTargetGoal<>(this, Player.class, 10, true, false, (target, level) -> Math.abs(target.getY() - this.getY()) <= 4.0)`:
/// the selector reads the ghast's current Y, so it is rebuilt from the mob before each use.
struct GhastTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl GhastTargetGoal {
    fn new() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None),
        }
    }

    fn refresh_selector(&mut self, mob: &dyn Entity) {
        let y = mob.base().y();
        self.inner.target_conditions = TargetingConditions::for_combat().selector(Some(Arc::new(
            move |target: &dyn Entity, _level: &ServerLevel| (target.base().y() - y).abs() <= 4.0,
        )));
    }
}

impl Goal for GhastTargetGoal {
    fn name(&self) -> &'static str {
        "NearestAttackableTargetGoal"
    }

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
    fn name(&self) -> &'static str {
        "Ghast$GhastShootFireballGoal"
    }

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
        let Some(target_id) = mob::tick::get_target_checked(mob, level) else {
            return;
        };
        let Some(target) = level.get_entity(target_id) else {
            return;
        };
        let in_sight = target.base().distance_to_sqr_vec(mob.position()) < 4096.0
            && mob::ext::has_line_of_sight(level, mob, target);
        let (target_x, target_y_half, target_z) = (
            target.base().x(),
            target.base().get_y_progress(0.5),
            target.base().z(),
        );
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
                let power = mob
                    .as_any()
                    .downcast_ref::<Ghast>()
                    .map_or(DEFAULT_EXPLOSION_POWER, Ghast::get_explosion_power);
                let mut fireball =
                    LargeFireball::from_mob(level, mob, direction.normalize(), power);
                let fz = fireball.base().z();
                fireball.set_pos(
                    mob.base().x() + view_vector.x * 4.0,
                    mob.base().get_y_progress(0.5) + 0.5,
                    fz + view_vector.z * 4.0,
                );
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

#[distributed_slice(ENTITY_FACTORIES)]
static GHAST: EntityRegistration = EntityRegistration {
    entity_type: EntityType::GHAST,
    create: |ty, id, registries| Box::new(Ghast::new(ty, id, registries)),
};
