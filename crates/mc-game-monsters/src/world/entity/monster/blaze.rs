//! `net.minecraft.world.entity.monster.Blaze`.

use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{Attribute, EntityType, SoundEvent};
use mc_game_ai::world::entity::ai::goal::MoveTowardsRestrictionGoal;
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{
    Flag, Goal, GoalFlags, LookAtPlayerGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::projectile::hurting::small_fireball::SmallFireball;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::pathfinder::PathType;

define_accessor!(pub DATA_FLAGS_ID: Byte = Blaze);

/// `Blaze`.
pub struct Blaze {
    pub mob: MobData,
    allowed_height_offset: f32,
    next_height_offset_change_tick: i32,
}

impl Blaze {
    /// `new Blaze(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |b| {
            b.define(DATA_FLAGS_ID, 0);
        });
        let mut blaze = Blaze {
            mob,
            allowed_height_offset: 0.5,
            next_height_offset_change_tick: 0,
        };
        let navigation = StandardPathNavigation::ground(&blaze);
        blaze.mob.set_navigation(Box::new(navigation));
        blaze.register_goals();
        blaze.mob.set_pathfinding_malus(PathType::Water, -1.0);
        blaze.mob.set_pathfinding_malus(PathType::Lava, 8.0);
        blaze
            .mob
            .set_pathfinding_malus(PathType::FireInNeighbor, 0.0);
        blaze.mob.set_pathfinding_malus(PathType::Fire, 0.0);
        blaze.mob.xp_reward = 10;
        blaze
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let look_at = LookAtPlayerGoal::new_player(self, 8.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(4, Box::new(BlazeAttackGoal::new()));
        goals.add_goal(5, Box::new(MoveTowardsRestrictionGoal::new(1.0)));
        goals.add_goal(
            7,
            Box::new(WaterAvoidingRandomStrollGoal::with_probability(1.0, 0.0)),
        );
        goals.add_goal(8, Box::new(look_at));
        goals.add_goal(8, Box::new(RandomLookAroundGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(
            1,
            Box::new(HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new())),
        );
        targets.add_goal(
            2,
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );
    }

    fn is_charged(&self) -> bool {
        (self.mob.living.entity.entity_data.get(DATA_FLAGS_ID) & 1) != 0
    }

    fn set_charged(&mut self, value: bool) {
        let mut flags = self.mob.living.entity.entity_data.get(DATA_FLAGS_ID);
        if value {
            flags |= 1;
        } else {
            flags &= -2;
        }
        self.mob.living.entity.entity_data.set(DATA_FLAGS_ID, flags);
    }
}

impl Entity for Blaze {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BLAZE_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BLAZE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BLAZE_DEATH)
    }

    fn get_light_level_dependent_magic_value(&self, _level: &ServerLevel) -> f32 {
        1.0
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        if !self.base().on_ground() && self.base().delta_movement().y < 0.0 {
            let m = self.base().delta_movement();
            self.base_mut()
                .set_delta_movement(m.multiply_xyz(1.0, 0.6, 1.0));
        }
        mob::tick::ai_step(self, level);
    }

    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        self.next_height_offset_change_tick -= 1;
        if self.next_height_offset_change_tick <= 0 {
            self.next_height_offset_change_tick = 100;
            self.allowed_height_offset = self.base_mut().random.triangle(0.5, 6.891) as f32;
        }
        let target = mob::tick::get_target_checked(self, level).and_then(|t| level.get_entity(t));
        if target.is_some_and(|t| {
            t.base().get_eye_y() > self.base().get_eye_y() + f64::from(self.allowed_height_offset)
                && self.can_attack(level, t)
        }) {
            let movement = self.base().delta_movement();
            self.base_mut().set_delta_movement(movement.add_xyz(
                0.0,
                (f64::from(0.3f32) - movement.y) * f64::from(0.3f32),
                0.0,
            ));
            self.base_mut().needs_sync = true;
        }
    }

    fn is_on_fire(&self) -> bool {
        self.is_charged()
    }
}

/// `Blaze.BlazeAttackGoal`.
struct BlazeAttackGoal {
    attack_step: i32,
    attack_time: i32,
    last_seen: i32,
}

impl BlazeAttackGoal {
    fn new() -> Self {
        Self {
            attack_step: 0,
            attack_time: 0,
            last_seen: 0,
        }
    }
}

fn blaze_mut(mob: &mut dyn Entity) -> &mut Blaze {
    mob.as_any_mut().downcast_mut::<Blaze>().expect("Blaze")
}

impl Goal for BlazeAttackGoal {
    fn name(&self) -> &'static str {
        "Blaze$BlazeAttackGoal"
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob::tick::get_target_checked(mob, level)
            .and_then(|t| level.get_entity(t))
            .is_some_and(|t| t.is_alive() && mob.can_attack(level, t))
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.attack_step = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        blaze_mut(mob).set_charged(false);
        self.last_seen = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.attack_time -= 1;
        let Some(target_id) = mob::tick::get_target_checked(mob, level) else {
            return;
        };
        let (has_line_of_sight, distance) = {
            let Some(target) = level.get_entity(target_id) else {
                return;
            };
            (
                mob.mob()
                    .expect("mob")
                    .sensing
                    .has_line_of_sight(level, mob, target),
                mob.base().distance_to_sqr_vec(target.position()),
            )
        };
        if has_line_of_sight {
            self.last_seen = 0;
        } else {
            self.last_seen += 1;
        }
        let follow = mob::attribute_value(mob, Attribute::FOLLOW_RANGE);
        if distance < 4.0 {
            if !has_line_of_sight {
                return;
            }
            if self.attack_time <= 0 {
                self.attack_time = 20;
                mob::combat::do_hurt_target(mob, level, target_id);
            }
            let Some(t) = level.get_entity(target_id) else {
                return;
            };
            mob::set_wanted_position(mob, t.base().x(), t.base().y(), t.base().z(), 1.0);
        } else if distance < follow * follow && has_line_of_sight {
            let (xd, yd, zd) = {
                let Some(t) = level.get_entity(target_id) else {
                    return;
                };
                (
                    t.base().x() - mob.base().x(),
                    t.base().get_y_progress(0.5) - mob.base().get_y_progress(0.5),
                    t.base().z() - mob.base().z(),
                )
            };
            if self.attack_time <= 0 {
                self.attack_step += 1;
                if self.attack_step == 1 {
                    self.attack_time = 60;
                    blaze_mut(mob).set_charged(true);
                } else if self.attack_step <= 4 {
                    self.attack_time = 6;
                } else {
                    self.attack_time = 100;
                    self.attack_step = 0;
                    blaze_mut(mob).set_charged(false);
                }
                if self.attack_step > 1 {
                    let sqd = f64::sqrt(f64::sqrt(distance)) * 0.5;
                    if !mob.base().is_silent() {
                        let pos = mob.block_position();
                        level.level_event(None, 1018, pos, 0);
                    }
                    let random = &mut mob.base_mut().random;
                    let direction = Vec3::new(
                        random.triangle(xd, 2.297 * sqd),
                        yd,
                        random.triangle(zd, 2.297 * sqd),
                    );
                    let mut fireball = SmallFireball::from_mob(level, mob, direction.normalize());
                    let (fx, fz) = (fireball.base().x(), fireball.base().z());
                    fireball.set_pos(fx, mob.base().get_y_progress(0.5) + 0.5, fz);
                    level.add_fresh_entity(Box::new(fireball));
                }
            }
            if let Some(t) = level.get_entity(target_id) {
                mob::look_at_entity_full(mob, t, 10.0, 10.0);
            }
        } else if self.last_seen < 5 {
            let Some(t) = level.get_entity(target_id) else {
                return;
            };
            mob::set_wanted_position(mob, t.base().x(), t.base().y(), t.base().z(), 1.0);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static BLAZE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::BLAZE,
    create: |ty, id, registries| Box::new(Blaze::new(ty, id, registries)),
};
