//! `Ravager`: the raid beast (stun and roar timers, leaf breaking, a raider on its back).

use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_data::{Attribute, EntityType, GameEvent, SoundEvent};
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtType;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType, class_of_type, class_of_types,
};
use mc_game_core::world::entity::ai::goal::{
    Flag, FloatGoal, LookAtPlayerGoal, MeleeAttackGoal, WaterAvoidingRandomStrollGoal,
};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use mc_game_core::world::entity::living_entity::damage::broadcast_entity_event;
use mc_game_core::world::entity::mob::{self, MobData, MobKind};
use mc_game_core::world::entity::raid::raider::{self, RaiderData};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::helpers::is_instance_of;
use mc_game_core::world::level::pathfinder::PathType;
use mc_nbt::CompoundTag;
use std::sync::Arc;

const TAG_ATTACK_TICK: &str = "AttackTick";
const TAG_STUN_TICK: &str = "StunTick";
const TAG_ROAR_TICK: &str = "RoarTick";

const BASE_MOVEMENT_SPEED: f64 = 0.3;
const ATTACK_MOVEMENT_SPEED: f64 = 0.35;
const ATTACK_DURATION: i32 = 10;
const STUN_DURATION: i32 = 40;

pub struct Ravager {
    pub mob: MobData,
    raider: RaiderData,
    attack_tick: i32,
    stunned_tick: i32,
    roar_tick: i32,
}

/// `entity instanceof AbstractIllager`.
fn is_abstract_illager(e: &dyn Entity) -> bool {
    matches!(
        e.get_type(),
        EntityType::PILLAGER | EntityType::VINDICATOR | EntityType::EVOKER | EntityType::ILLUSIONER
    )
}

/// `strongKnockback(entity)` of a ravager standing at (`x`, `z`).
fn strong_knockback(x: f64, z: f64, entity: &mut dyn Entity) {
    let xd = entity.base().x() - x;
    let zd = entity.base().z() - z;
    let dd = (xd * xd + zd * zd).max(0.001);
    entity.push_xyz(xd / dd * 4.0, 0.2, zd / dd * 4.0);
}

impl Ravager {
    /// `new Ravager(type, level)`: `registerGoals()`, the ground navigation, the experience reward and the leaves malus.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, raider::define_synched_data);
        let mut ravager = Ravager {
            mob,
            raider: RaiderData::default(),
            attack_tick: 0,
            stunned_tick: 0,
            roar_tick: 0,
        };
        ravager.mob.xp_reward = 20;
        ravager.mob.set_pathfinding_malus(PathType::Leaves, 0.0);
        let navigation = StandardPathNavigation::ground(&ravager);
        ravager.mob.set_navigation(Box::new(navigation));
        ravager.register_goals();
        ravager
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        raider::register_goals(&mut self.mob);
        let float = FloatGoal::new(self);
        let look_at_player = LookAtPlayerGoal::new_player(self, 6.0);
        let look_at_mob = LookAtPlayerGoal::new(
            self,
            LookAtType::Class(Arc::new(|e: &dyn Entity| e.mob().is_some())),
            8.0,
        );
        let sel = &mut self.mob.goal_selector;
        sel.add_goal(0, Box::new(float));
        sel.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, true)));
        sel.add_goal(5, Box::new(WaterAvoidingRandomStrollGoal::new(0.4)));
        sel.add_goal(6, Box::new(look_at_player));
        sel.add_goal(10, Box::new(look_at_mob));
        let t = &mut self.mob.target_selector;
        t.add_goal(
            2,
            Box::new(
                HurtByTargetGoal::new(vec![Arc::new(|e: &dyn Entity| e.raider().is_some())])
                    .set_alert_others(Vec::new()),
            ),
        );
        t.add_goal(
            3,
            Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)),
        );
        t.add_goal(
            4,
            Box::new(NearestAttackableTargetGoal::with_selector(
                TargetType::Class(class_of_types(&[
                    EntityType::VILLAGER,
                    EntityType::WANDERING_TRADER,
                ])),
                true,
                Arc::new(|target, _level| !target.is_baby()),
            )),
        );
        t.add_goal(
            4,
            Box::new(NearestAttackableTargetGoal::new(
                TargetType::Class(class_of_type(EntityType::IRON_GOLEM)),
                true,
            )),
        );
    }

    /// `stunEffect()`: the particle is client side, the draws that place it are not.
    fn stun_effect(&mut self) {
        let random = &mut self.base_mut().random;
        if random.next_int_bounded(6) == 0 {
            random.next_double();
            random.next_double();
        }
    }

    /// `roar()`: hurts and throws back everything within 4 blocks except ravagers (and armor stands without `mobGriefing`).
    fn roar(&mut self, level: &mut ServerLevel) {
        if !self.is_alive() {
            return;
        }
        let griefing = level.mob_griefing();
        let area = self.get_bounding_box().inflate(4.0, 4.0, 4.0);
        let source = level.damage_sources().mob_attack(&*self);
        let (x, z) = (self.base().x(), self.base().z());
        level.lend(self.as_dyn_mut(), |level| {
            let victims = level.get_entities_of_class(
                &area,
                |e| e.living().is_some(),
                |e| {
                    e.get_type() != EntityType::RAVAGER
                        && e.is_alive()
                        && (griefing || e.get_type() != EntityType::ARMOR_STAND)
                },
            );
            for id in victims {
                level.with_entity(id, |level, entity| {
                    if !is_abstract_illager(entity) {
                        entity.hurt_server(level, &source, 6.0);
                    }
                    if !entity.is_player() {
                        strong_knockback(x, z, entity);
                    }
                });
            }
        });
        self.game_event(level, GameEvent::ENTITY_ACTION);
        broadcast_entity_event(level, self.id(), 69);
    }

    /// The leaves in and around the box of a ravager that ran into something: destroyed, or the ravager jumps.
    fn break_leaves(&mut self, level: &mut ServerLevel) {
        let bb = self.get_bounding_box().inflate(0.2, 0.2, 0.2);
        let positions: Vec<BlockPos> = BlockPos::between_closed_aabb(&bb).collect();
        let id = self.id();
        let destroyed = level.lend(self.as_dyn_mut(), |level| {
            let mut destroyed = false;
            for pos in positions {
                let state: BlockState = level.get_block_state(pos);
                if is_instance_of(state, "LeavesBlock") {
                    destroyed = level.destroy_block_impl(pos, true, Some(id), 512) || destroyed;
                }
            }
            destroyed
        });
        if !destroyed && self.base().on_ground() {
            self.jump_from_ground(level);
        }
    }

    pub fn get_attack_tick(&self) -> i32 {
        self.attack_tick
    }

    pub fn get_stunned_tick(&self) -> i32 {
        self.stunned_tick
    }

    pub fn get_roar_tick(&self) -> i32 {
        self.roar_tick
    }
}

impl Entity for Ravager {
    mc_game_core::mob_entity_accessors!(mob);
    mc_game_core::raider_entity!(
        raider;
        update_no_action_time,
        die,
        hurt_server,
        can_join_patrol,
        remove_when_far_away,
        requires_custom_persistence,
        finalize_spawn,
        pick_up_item,
        after_loaded
    );

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:ravager")
    }

    fn update_control_flags(&mut self, level: &mut ServerLevel) {
        let no_controller =
            mob::tick::update_control_flags_with(self.as_dyn_mut(), level, |controller| {
                controller.mob().is_some()
                    && !mc_data::tags::get::<EntityType>("raiders")
                        .is_some_and(|t| t.contains(controller.get_type()))
            });
        self.mob
            .goal_selector
            .set_control_flag(Flag::Target, no_controller);
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        raider::add_additional_save_data(self, out);
        out.put_int(TAG_ATTACK_TICK, self.attack_tick);
        out.put_int(TAG_STUN_TICK, self.stunned_tick);
        out.put_int(TAG_ROAR_TICK, self.roar_tick);
    }

    fn read_additional_save_data(&mut self, level: Option<&ServerLevel>, input: &CompoundTag) {
        raider::read_additional_save_data(self, level, input);
        self.attack_tick = input.get_int_or(TAG_ATTACK_TICK, 0);
        self.stunned_tick = input.get_int_or(TAG_STUN_TICK, 0);
        self.roar_tick = input.get_int_or(TAG_ROAR_TICK, 0);
    }

    fn get_celebrate_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RAVAGER_CELEBRATE)
    }

    fn get_max_head_y_rot(&self) -> i32 {
        45
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        raider::ai_step(self, level);
        if !self.is_alive() {
            return;
        }
        let immobile = self.is_immobile();
        let max_speed = if self.get_target().is_some() {
            ATTACK_MOVEMENT_SPEED
        } else {
            BASE_MOVEMENT_SPEED
        };
        if let Some(speed) = self
            .mob
            .living
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
        {
            if immobile {
                speed.set_base_value(0.0);
            } else {
                let base_value = speed.get_base_value();
                speed.set_base_value(mth::lerp_f64(0.1, base_value, max_speed));
            }
        }
        if self.base().horizontal_collision && level.mob_griefing() {
            self.break_leaves(level);
        }
        if self.roar_tick > 0 {
            self.roar_tick -= 1;
            if self.roar_tick == 10 {
                self.roar(level);
            }
        }
        if self.attack_tick > 0 {
            self.attack_tick -= 1;
        }
        if self.stunned_tick > 0 {
            self.stunned_tick -= 1;
            self.stun_effect();
            if self.stunned_tick == 0 {
                self.play_sound(level, SoundEvent::ENTITY_RAVAGER_ROAR, 1.0, 1.0);
                self.roar_tick = 20;
            }
        }
    }

    fn is_immobile(&self) -> bool {
        self.mob.living.is_dead_or_dying()
            || self.attack_tick > 0
            || self.stunned_tick > 0
            || self.roar_tick > 0
    }

    fn has_line_of_sight(&self, level: &ServerLevel, target: &dyn Entity) -> bool {
        if self.stunned_tick <= 0 && self.roar_tick <= 0 {
            mob::ext::has_line_of_sight(level, self, target)
        } else {
            false
        }
    }

    fn blocked_by_item(
        &mut self,
        level: &mut ServerLevel,
        defender: &mut dyn Entity,
        _source: &DamageSource,
        _damage: f32,
        _fully_blocked: bool,
    ) {
        if self.roar_tick == 0 {
            if self.base_mut().random.next_double() < 0.5 {
                self.stunned_tick = STUN_DURATION;
                self.play_sound(level, SoundEvent::ENTITY_RAVAGER_STUNNED, 1.0, 1.0);
                broadcast_entity_event(level, self.id(), 39);
                defender.push_entity(level, self.as_dyn_mut());
            } else {
                strong_knockback(self.base().x(), self.base().z(), defender);
            }
            defender.base_mut().sync_velocity = true;
        }
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        self.attack_tick = ATTACK_DURATION;
        broadcast_entity_event(level, self.id(), 4);
        self.play_sound(level, SoundEvent::ENTITY_RAVAGER_ATTACK, 1.0, 1.0);
        mc_game_core::world::entity::living_entity::combat::mob_do_hurt_target(
            self.as_dyn_mut(),
            level,
            target,
        )
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RAVAGER_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RAVAGER_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_RAVAGER_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_RAVAGER_STEP, 0.15, 1.0);
    }

    fn check_spawn_obstruction(
        &self,
        level: &dyn mc_game_core::world::level::server_level_accessor::ServerLevelAccessor,
    ) -> bool {
        !level.contains_any_liquid(&self.base().bounding_box())
    }

    fn get_attack_bounding_box(
        &self,
        level: &ServerLevel,
        horizontal_expansion: f64,
    ) -> mc_core::phys::AABB {
        mob::combat::get_attack_bounding_box(self, level, horizontal_expansion)
            .deflate(0.05, 0.0, 0.05)
    }

    fn can_be_leader(&self) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static RAVAGER: EntityRegistration = EntityRegistration {
    entity_type: EntityType::RAVAGER,
    create: |ty, id, registries| Box::new(Ravager::new(ty, id, registries)),
};

/// Scenario-harness fields of a ravager (order 41; the twin of the `Ravager` entry of `EntityExtras.java`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static RAVAGER_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 41,
    applies: |e| e.as_any().is::<Ravager>(),
    fields: |e, _level, _ctx| {
        let ravager = e.as_any().downcast_ref::<Ravager>().expect("ravager");
        vec![
            ("attack".into(), ravager.attack_tick.to_string()),
            ("stun".into(), ravager.stunned_tick.to_string()),
            ("roar".into(), ravager.roar_tick.to_string()),
        ]
    },
};
