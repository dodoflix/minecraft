//! `Ocelot`: the wild cat that learns to trust players fed with raw fish.

use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::difficulty_instance::DifficultyInstance;
use mc_game_ai::world::entity::ai::goal::OcelotAttackGoal;
use mc_game_ai::world::entity::ai::goal::{AvoidEntityGoal, LeapAtTargetGoal};
use mc_game_core::world::entity::ai::goal::{BreedGoal, FloatGoal, Goal, GoalFlags, LookAtPlayerGoal, TemptGoal, WaterAvoidingRandomStrollGoal};
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::navigation::StandardPathNavigation;
use crate::world::entity::animal::turtle;
use mc_game_core::world::entity::dimensions::{EntityAttachments, EntityDimensions};
use mc_game_core::world::entity::factory::{ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use mc_game_core::world::entity::living_entity::util::broadcast_entity_event;
use mc_game_core::world::entity::mob::spawn::AgeableMobGroupData;
use mc_game_core::world::entity::mob::{self, MobData, MobKind, interact};
use mc_game_core::world::entity::spawn_group_data::SpawnGroupData;
use mc_game_core::world::entity::{Entity, EntityId, ServerPlayer};
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::server_level_accessor::ServerLevelAccessor;
use mc_game_core::world::level::{BlockGetter as _, LevelReader as _};
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, Pose};
use mc_data::{EntityType, Item, SoundEvent};
use mc_nbt::CompoundTag;
use std::any::Any;
use std::sync::Arc;

define_accessor!(pub DATA_TRUSTING: Boolean = Ocelot);

/// `BABY_DIMENSIONS`.
fn baby_dimensions() -> EntityDimensions {
    let dims = EntityDimensions::scalable(0.3, 0.35).with_eye_height(0.34375);
    dims.with_attachments(EntityAttachments::create_default(0.3, 0.35).with_passenger(Vec3::new(0.0, 0.3125, 0.0)))
}

fn is_ocelot_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("ocelot_food").is_some_and(|t| t.contains(stack.item()))
}

fn is_trusting(mob: &dyn Entity) -> bool {
    mob.as_any().downcast_ref::<Ocelot>().is_some_and(Ocelot::is_trusting)
}

/// `Ocelot.OcelotAvoidEntityGoal<Player>`.
pub struct OcelotAvoidEntityGoal {
    avoid: AvoidEntityGoal,
}

impl OcelotAvoidEntityGoal {
    pub fn new() -> Self {
        Self { avoid: AvoidEntityGoal::new(Arc::new(|e| e.is_player()), 16.0, 0.8, 1.33) }
    }
}

impl Goal for OcelotAvoidEntityGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !is_trusting(mob) && self.avoid.can_use_impl(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !is_trusting(mob) && self.avoid.can_continue_to_use_impl(mob)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.avoid.start_impl(mob, level);
    }
    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.avoid.stop_impl();
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.avoid.tick_impl(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.avoid.flags()
    }
}

/// `Ocelot.OcelotTemptGoal`.
pub struct OcelotTemptGoal {
    tempt: TemptGoal,
}

impl OcelotTemptGoal {
    pub fn new() -> Self {
        Self { tempt: TemptGoal::new(0.6, Arc::new(|stack: &ItemStack| is_ocelot_food(stack)), true) }
    }

    pub fn is_running(&self) -> bool {
        self.tempt.is_running()
    }
}

impl Goal for OcelotTemptGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.tempt.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let can_scare = self.tempt.can_scare() && !is_trusting(mob);
        self.tempt.scare_check(mob, level, can_scare) && self.can_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.stop(mob, level);
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tempt.tick(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.tempt.flags()
    }
}

/// `Ocelot`.
pub struct Ocelot {
    pub mob: MobData,
}

impl Ocelot {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_TRUSTING, false);
        });
        let mut ocelot = Ocelot { mob };
        let navigation = StandardPathNavigation::ground(&ocelot);
        ocelot.mob.set_navigation(Box::new(navigation));
        ocelot.register_goals();
        ocelot.reassess_trusting_goals(None);
        ocelot
    }

    fn register_goals(&mut self) {
        let float = FloatGoal::new(self);
        let breed = BreedGoal::new(self, 0.8);
        let look_at = LookAtPlayerGoal::new_player(self, 10.0);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(1, Box::new(float));
        goals.add_goal(3, Box::new(OcelotTemptGoal::new()));
        goals.add_goal(7, Box::new(LeapAtTargetGoal::new(0.3)));
        goals.add_goal(8, Box::new(OcelotAttackGoal::new()));
        goals.add_goal(9, Box::new(breed));
        goals.add_goal(10, Box::new(WaterAvoidingRandomStrollGoal::with_probability(0.8, 1.0000001E-5)));
        goals.add_goal(11, Box::new(look_at));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(NearestAttackableTargetGoal::new(TargetType::of_type(EntityType::CHICKEN), false)));
        targets.add_goal(1, Box::new(NearestAttackableTargetGoal::build(TargetType::of_type(EntityType::TURTLE), 10, false, false, Some(Arc::new(turtle::baby_on_land_selector)))));
    }

    pub fn is_trusting(&self) -> bool {
        self.mob.living.entity.entity_data.get(DATA_TRUSTING)
    }

    /// `setTrusting(trusting)`.
    fn set_trusting(&mut self, level: Option<&mut ServerLevel>, trusting: bool) {
        self.mob.living.entity.entity_data.set(DATA_TRUSTING, trusting);
        self.reassess_trusting_goals(level);
    }

    /// `reassessTrustingGoals()`: the avoid-players goal exists exactly while the ocelot does not trust.
    fn reassess_trusting_goals(&mut self, level: Option<&mut ServerLevel>) {
        let mut selector = std::mem::take(&mut self.mob.goal_selector);
        let is_avoid = |g: &dyn Goal| (g as &dyn Any).is::<OcelotAvoidEntityGoal>();
        match level {
            Some(level) => selector.remove_all_goals(self, level, is_avoid),
            None => selector.remove_all_goals_not_running(is_avoid),
        }
        if !self.is_trusting() {
            selector.add_goal(4, Box::new(OcelotAvoidEntityGoal::new()));
        }
        self.mob.goal_selector = selector;
    }

    /// `spawnTrustingParticles(success)`: the particles are client-side, the random draws are not.
    fn spawn_trusting_particles(&mut self) {
        for _ in 0..7 {
            let random = &mut self.mob.living.entity.random;
            let (_xa, _ya, _za) = (random.next_gaussian() * 0.02, random.next_gaussian() * 0.02, random.next_gaussian() * 0.02);
            let (_x, _y, _z) = (self.base_mut().get_random_x(1.0), self.base_mut().get_random_y(), self.base_mut().get_random_z(1.0));
        }
    }
}

impl Entity for Ocelot {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:ocelot")
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        if self.is_baby() { baby_dimensions() } else { EntityDimensions::of_type(self.mob.living.entity.entity_type) }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        out.put_boolean("Trusting", self.is_trusting());
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_trusting(None, input.get_boolean_or("Trusting", false));
    }

    fn custom_server_ai_step(&mut self, _level: &mut ServerLevel) {
        super::custom_server_ai_step(self);
    }

    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.is_trusting() && self.base().tick_count > 2400
    }

    fn get_ambient_sound_in(&mut self, _level: &mut ServerLevel) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_OCELOT_AMBIENT)
    }

    fn get_ambient_sound_interval(&self) -> i32 {
        900
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_OCELOT_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_OCELOT_DEATH)
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let item_stack = player.get_item_in_hand(hand);
        let tempting = self.mob.goal_selector.find_goal::<OcelotTemptGoal>().is_none_or(OcelotTemptGoal::is_running);
        if tempting && !self.is_trusting() && self.is_food(&item_stack) && player.base().distance_to_sqr_vec(self.position()) < 9.0 {
            interact::use_player_item(level, player, hand);
            if self.base_mut().random.next_int_bounded(3) == 0 {
                self.set_trusting(Some(&mut *level), true);
                self.spawn_trusting_particles();
                broadcast_entity_event(level, self.id(), 41);
            } else {
                self.spawn_trusting_particles();
                broadcast_entity_event(level, self.id(), 40);
            }
            InteractionResult::SUCCESS
        } else {
            interact::mob_interact(self, level, player, hand)
        }
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(EntityType::OCELOT, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_ocelot_food(stack)
    }

    fn check_spawn_obstruction(&self, level: &dyn ServerLevelAccessor) -> bool {
        if level.is_unobstructed_entity(self) && !level.contains_any_liquid(&self.base().bounding_box()) {
            let pos = self.block_position();
            if pos.y() < level.get_sea_level() {
                return false;
            }
            let state = level.get_block_state(pos.below());
            if state.block() == Block::GRASS_BLOCK || state.is_tag("leaves") {
                return true;
            }
        }
        false
    }

    fn finalize_spawn(
        &mut self,
        level: &mut dyn ServerLevelAccessor,
        difficulty: DifficultyInstance,
        reason: EntitySpawnReason,
        group_data: Option<Box<dyn SpawnGroupData>>,
    ) -> Option<Box<dyn SpawnGroupData>> {
        let group_data = group_data.or_else(|| Some(Box::new(AgeableMobGroupData::with_chance(1.0))));
        mob::spawn::finalize_spawn(self, level, difficulty, reason, group_data)
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(0.0, f64::from(0.5f32 * self.base().eye_height), f64::from(self.base().get_bb_width() * 0.4))
    }

    fn is_stepping_carefully(&self) -> bool {
        self.is_crouching() || self.is_shift_key_down()
    }
}

/// `Ocelot.checkOcelotSpawnRules`.
fn check_ocelot_spawn_rules(_ty: EntityType, _level: &dyn ServerLevelAccessor, _reason: EntitySpawnReason, _pos: BlockPos, random: &mut dyn RandomSource) -> bool {
    random.next_int_bounded(3) != 0
}

mc_game_core::register_spawn_placement!(EntityType::OCELOT, check_ocelot_spawn_rules);

#[distributed_slice(ENTITY_FACTORIES)]
static OCELOT: EntityRegistration = EntityRegistration { entity_type: EntityType::OCELOT, create: |ty, id, registries| Box::new(Ocelot::new(ty, id, registries)) };
