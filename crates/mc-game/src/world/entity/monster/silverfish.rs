//! `net.minecraft.world.entity.monster.Silverfish`.

use crate::register_spawn_placement;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::goal::target::{HurtByTargetGoal, NearestAttackableTargetGoal, TargetType};
use crate::world::entity::ai::goal::{ClimbOnTopOfPowderSnowGoal, FloatGoal, Flag, Goal, GoalFlags, MeleeAttackGoal, RandomStrollGoal, reduced_tick_delay};
use crate::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::entity::MovementEmission;
use crate::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration};
use crate::world::entity::mob::{self, MobData, MobKind, navigation};
use crate::world::entity::spawn_predicates::{check_any_light_monster_spawn_rules, is_spawner};
use crate::world::entity::{Entity, EntityId};
use crate::world::level::block::helpers::is_instance_of;
use crate::world::level::block::impls::infested_block;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, SoundEvent};

/// `Silverfish`. The `SilverfishWakeUpFriendsGoal`'s `lookForFriends` counter lives here (the goal is the only user and
/// `hurtServer` reaches it through `friendsGoal.notifyHurt()` in vanilla).
pub struct Silverfish {
    pub mob: MobData,
    /// `friendsGoal.lookForFriends`.
    look_for_friends: i32,
}

impl Silverfish {
    /// `new Silverfish(type, level)`.
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Monster, |_| {});
        let mut silverfish = Silverfish { mob, look_for_friends: 0 };
        let navigation = StandardPathNavigation::ground(&silverfish);
        silverfish.mob.set_navigation(Box::new(navigation));
        silverfish.register_goals();
        silverfish
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal::new()));
        goals.add_goal(3, Box::new(SilverfishWakeUpFriendsGoal));
        goals.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, false)));
        goals.add_goal(5, Box::new(SilverfishMergeWithStoneGoal::new()));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(HurtByTargetGoal::new(Vec::new()).set_alert_others(Vec::new())));
        targets.add_goal(2, Box::new(NearestAttackableTargetGoal::new(TargetType::Player, true)));
    }

    /// `SilverfishWakeUpFriendsGoal.notifyHurt()`.
    fn notify_hurt(&mut self) {
        if self.look_for_friends == 0 {
            self.look_for_friends = reduced_tick_delay(20);
        }
    }
}

impl Entity for Silverfish {
    crate::mob_entity_accessors!(mob);

    fn get_movement_emission(&self) -> MovementEmission {
        MovementEmission::Events
    }

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SILVERFISH_AMBIENT)
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SILVERFISH_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_SILVERFISH_DEATH)
    }

    fn play_step_sound(&mut self, level: &mut ServerLevel, _pos: BlockPos, _state: mc_data::block::BlockState) {
        self.play_sound(level, SoundEvent::ENTITY_SILVERFISH_STEP, 0.15, 1.0);
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        if source.get_entity().is_some() || source.is_tag(level, "always_triggers_silverfish") {
            self.notify_hurt();
        }
        crate::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        self.mob.living.y_body_rot = self.base().y_rot();
        mob::tick::tick(self, level);
    }

    fn set_y_body_rot(&mut self, y_body_rot: f32) {
        self.base_mut().set_y_rot(y_body_rot);
        self.mob.living.y_body_rot = y_body_rot;
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if infested_block::is_compatible_host_block(level.get_block_state(pos.below())) { 10.0 } else { mob::pathfinder::get_walk_target_value(self, pos, level) }
    }
}

/// `Silverfish.checkSilverfishSpawnRules`.
fn check_silverfish_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    if !check_any_light_monster_spawn_rules(ty, level, reason, pos, random) {
        false
    } else if is_spawner(reason) {
        true
    } else {
        level.get_nearest_player_ignoring_creative(f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.5, f64::from(pos.z()) + 0.5, 5.0).is_none()
    }
}

register_spawn_placement!(EntityType::SILVERFISH, check_silverfish_spawn_rules);

/// `Silverfish.SilverfishMergeWithStoneGoal` (`RandomStrollGoal(silverfish, 1.0, 10)`).
struct SilverfishMergeWithStoneGoal {
    stroll: RandomStrollGoal,
    selected_direction: Option<Direction>,
    do_merge: bool,
}

impl SilverfishMergeWithStoneGoal {
    fn new() -> Self {
        Self { stroll: RandomStrollGoal::with_interval(1.0, 10), selected_direction: None, do_merge: false }
    }
}

impl Goal for SilverfishMergeWithStoneGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob::tick::get_target_checked(mob, level).is_some() {
            return false;
        }
        if !navigation::is_done(mob, level) {
            return false;
        }
        if level.mob_griefing() && mob.base_mut().random.next_int_bounded(reduced_tick_delay(10)) == 0 {
            let direction = Direction::get_random(&mut mob.base_mut().random);
            self.selected_direction = Some(direction);
            let b = mob.base();
            let pos = BlockPos::containing(b.x(), b.y() + 0.5, b.z()).relative(direction);
            if infested_block::is_compatible_host_block(level.get_block_state(pos)) {
                self.do_merge = true;
                return true;
            }
        }
        self.do_merge = false;
        self.stroll.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.do_merge { false } else { self.stroll.can_continue_to_use(mob, level) }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !self.do_merge {
            self.stroll.start(mob, level);
        } else if let Some(direction) = self.selected_direction {
            let b = mob.base();
            let pos = BlockPos::containing(b.x(), b.y() + 0.5, b.z()).relative(direction);
            let state = level.get_block_state(pos);
            if infested_block::is_compatible_host_block(state) {
                level.set_block_and_update(pos, infested_block::infested_state_by_host(state));
                // `Mob.spawnAnim()`
                level.broadcast_entity_event(mob.id(), 20);
                mob.discard();
            }
        }
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stroll.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Silverfish.SilverfishWakeUpFriendsGoal`; its state is `Silverfish::look_for_friends`.
struct SilverfishWakeUpFriendsGoal;

impl Goal for SilverfishWakeUpFriendsGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.as_any().downcast_ref::<Silverfish>().is_some_and(|s| s.look_for_friends > 0)
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(silverfish) = mob.as_any_mut().downcast_mut::<Silverfish>() else { return };
        silverfish.look_for_friends -= 1;
        if silverfish.look_for_friends > 0 {
            return;
        }
        let base_pos = mob.block_position();
        let mut y_off = 0;
        while (-5..=5).contains(&y_off) {
            let mut x_off = 0;
            while (-10..=10).contains(&x_off) {
                let mut z_off = 0;
                while (-10..=10).contains(&z_off) {
                    let test_pos = base_pos.offset(x_off, y_off, z_off);
                    let state = level.get_block_state(test_pos);
                    if is_instance_of(state, "InfestedBlock") {
                        if level.mob_griefing() {
                            level.destroy_block_impl(test_pos, true, Some(mob.id()), 512);
                        } else {
                            level.set_block_and_update(test_pos, infested_block::host_state_by_infested(level.get_block_state(test_pos)));
                        }
                        if mob.base_mut().random.next_boolean() {
                            return;
                        }
                    }
                    z_off = (if z_off <= 0 { 1 } else { 0 }) - z_off;
                }
                x_off = (if x_off <= 0 { 1 } else { 0 }) - x_off;
            }
            y_off = (if y_off <= 0 { 1 } else { 0 }) - y_off;
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static SILVERFISH: EntityRegistration = EntityRegistration { entity_type: EntityType::SILVERFISH, create: |ty, id, registries| Box::new(Silverfish::new(ty, id, registries)) };
