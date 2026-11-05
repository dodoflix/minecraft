//! `net.minecraft.world.entity.animal.bee.Bee` with all its inner goals.
//!
//! The Java inner goal classes keep state that `Bee` itself reads (`beePollinateGoal.isPollinating()`,
//! `goToHiveGoal.travellingTicks`, `goToHiveGoal.isTargetBlacklisted`, ...). The goal selector is taken out of the mob
//! while goals run, so that state lives in [`Bee`] (`PollinateState`, `GoToHiveState`, ...) and the goals are thin
//! structs that operate on `&mut Bee` (the same trick as Java's `Bee.this`).

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BeehiveBlockEntity;
use crate::world::damagesource::DamageSource;
use crate::world::entity::ai::control::flying_move_control::FlyingMoveControl;
use crate::world::entity::ai::control::look_control::{self, LookControl, LookControlBase};
use crate::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, Goal, GoalFlags, HurtByTargetGoal, MeleeAttackGoal, NearestAttackableTargetGoal, ResetUniversalAngerTargetGoal, TemptGoal, Flag,
};
use crate::world::entity::ai::navigation::{NavClass, NavigationData, PathNavigation};
use crate::world::entity::ai::util::{air_and_water_random_pos, air_random_pos, hover_random_pos};
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::factory::{self, ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest};
use crate::world::entity::living_entity::damage::make_sound;
use crate::world::entity::living_entity::util::{hurt, l, lm};
use crate::world::entity::mob::{self, MobData, MobKind, NeutralData, navigation, neutral};
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::pathfinder::{Path, PathType};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;
use mc_data::{EntityType, Item, MobEffect, SoundEvent, props};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::collections::HashMap;

define_accessor!(pub DATA_FLAGS_ID: Byte = Bee);
define_accessor!(pub DATA_ANGER_END_TIME: Long = Bee);

pub const FLAP_DEGREES_PER_TICK: f32 = 120.32113;
/// `Mth.ceil(1.4959966F)`.
pub const TICKS_PER_FLAP: i32 = 2;
const FLAG_ROLL: i32 = 2;
const FLAG_HAS_STUNG: i32 = 4;
const FLAG_HAS_NECTAR: i32 = 8;
const TICKS_BEFORE_GOING_TO_KNOWN_FLOWER: i32 = 600;
const TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME: i32 = 3600;
const MAX_CROPS_GROWABLE: i32 = 10;
const TOO_FAR_DISTANCE: i32 = 48;
const HIVE_CLOSE_ENOUGH_DISTANCE: i32 = 2;
const RESTRICTED_WANDER_DISTANCE_REDUCTION: i32 = 24;
const DEFAULT_WANDER_DISTANCE_REDUCTION: i32 = 16;
const PATHFIND_TO_HIVE_WHEN_CLOSER_THAN: i32 = 16;
const HIVE_SEARCH_DISTANCE: i32 = 20;
const COOLDOWN_BEFORE_LOCATING_NEW_HIVE: i32 = 200;
const COOLDOWN_BEFORE_LOCATING_NEW_FLOWER: i32 = 200;
const MIN_FIND_FLOWER_RETRY_COOLDOWN: i32 = 20;
const MAX_FIND_FLOWER_RETRY_COOLDOWN: i32 = 60;
const MAX_TRAVELLING_TICKS: i32 = 2400;
/// `PERSISTENT_ANGER_TIME = TimeUtil.rangeOfSeconds(20, 39)`.
const PERSISTENT_ANGER_TIME: (i32, i32) = (20 * 20, 39 * 20);

const PI_OVER_10: f32 = (std::f64::consts::PI / 10.0) as f32;
const PI_OVER_2: f32 = (std::f64::consts::PI / 2.0) as f32;

/// `BeePollinateGoal`'s fields.
#[derive(Default)]
struct PollinateState {
    successful_pollinating_ticks: i32,
    last_sound_played_tick: i32,
    pollinating: bool,
    hover_pos: Option<Vec3>,
    pollinating_ticks: i32,
    /// `Long2LongOpenHashMap unreachableFlowerCache` (only `getOrDefault` / `put` are used).
    unreachable_flower_cache: HashMap<i64, i64>,
}

/// `BeeGoToHiveGoal`'s fields.
#[derive(Default)]
struct GoToHiveState {
    travelling_ticks: i32,
    blacklisted_targets: Vec<BlockPos>,
    last_path: Option<Path>,
    ticks_stuck: i32,
}

pub struct Bee {
    pub mob: MobData,
    roll_amount: f32,
    roll_amount_o: f32,
    time_since_sting: i32,
    ticks_without_nectar_since_exiting_hive: i32,
    stay_out_of_hive_countdown: i32,
    num_crops_grown_since_pollination: i32,
    remaining_cooldown_before_locating_new_hive: i32,
    remaining_cooldown_before_locating_new_flower: i32,
    saved_flower_pos: Option<BlockPos>,
    hive_pos: Option<BlockPos>,
    pollinate: PollinateState,
    go_to_hive: GoToHiveState,
    go_to_known_flower_travelling_ticks: i32,
    under_water_ticks: i32,
}

impl Bee {
    /// `new Bee(type, level)`.
    pub fn new(ty: EntityType, id: EntityId) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_FLAGS_ID, 0);
            b.define(DATA_ANGER_END_TIME, -1);
        });
        let mut bee = Bee {
            mob,
            roll_amount: 0.0,
            roll_amount_o: 0.0,
            time_since_sting: 0,
            ticks_without_nectar_since_exiting_hive: 0,
            stay_out_of_hive_countdown: 0,
            num_crops_grown_since_pollination: 0,
            remaining_cooldown_before_locating_new_hive: 0,
            remaining_cooldown_before_locating_new_flower: 0,
            saved_flower_pos: None,
            hive_pos: None,
            pollinate: PollinateState::default(),
            go_to_hive: GoToHiveState::default(),
            go_to_known_flower_travelling_ticks: 0,
            under_water_ticks: 0,
        };
        bee.mob.neutral = Some(NeutralData { end_time_accessor: Some(DATA_ANGER_END_TIME), ..NeutralData::default() });
        // `createNavigation(level)`
        let mut navigation = BeeNavigation::new(&bee);
        navigation.set_can_open_doors(false);
        navigation.set_can_float(false);
        navigation.set_required_path_length(&bee, 48.0);
        bee.mob.set_navigation(Box::new(navigation));
        bee.register_goals();
        // field initializer, which runs after `Mob.<init>` (so after the goal constructors' RNG draws)
        bee.remaining_cooldown_before_locating_new_flower = mth::next_int(&mut bee.mob.living.entity.random, MIN_FIND_FLOWER_RETRY_COOLDOWN, MAX_FIND_FLOWER_RETRY_COOLDOWN);
        bee.mob.move_control = Some(Box::new(FlyingMoveControl::new(20, true)));
        bee.mob.look_control = Some(Box::new(BeeLookControl::default()));
        bee.mob.set_pathfinding_malus(PathType::Fire, -1.0);
        bee.mob.set_pathfinding_malus(PathType::Water, -1.0);
        bee.mob.set_pathfinding_malus(PathType::WaterBorder, 16.0);
        bee.mob.set_pathfinding_malus(PathType::Cocoa, -1.0);
        bee.mob.set_pathfinding_malus(PathType::Fence, -1.0);
        bee
    }

    /// `registerGoals()`.
    fn register_goals(&mut self) {
        let attack = BeeAttackGoal::new(self, f64::from(1.4f32), true);
        let breed = BreedGoal::new(self, 1.0);
        let tempt = TemptGoal::new(1.25, std::sync::Arc::new(|stack: &ItemStack| is_bee_food(stack)), false);
        let validate_hive = ValidateHiveGoal::new(self);
        let validate_flower = ValidateFlowerGoal::new(self);
        let follow_parent = FollowParentGoal::new(self, 1.25);
        let float = FloatGoal::new(self);
        let hurt_by_other = BeeHurtByOtherGoal::new(self);
        let become_angry = BeeBecomeAngryTargetGoal::new(self);
        let reset_anger = ResetUniversalAngerTargetGoal::new(self, true);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(attack));
        goals.add_goal(1, Box::new(BaseBee(BeeEnterHiveGoal)));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(3, Box::new(tempt));
        goals.add_goal(3, Box::new(BaseBee(validate_hive)));
        goals.add_goal(3, Box::new(BaseBee(validate_flower)));
        goals.add_goal(4, Box::new(BaseBee(BeePollinateGoal)));
        goals.add_goal(5, Box::new(follow_parent));
        goals.add_goal(5, Box::new(BaseBee(BeeLocateHiveGoal)));
        goals.add_goal(5, Box::new(BaseBee(BeeGoToHiveGoal)));
        goals.add_goal(6, Box::new(BaseBee(BeeGoToKnownFlowerGoal)));
        goals.add_goal(7, Box::new(BaseBee(BeeGrowCropGoal)));
        goals.add_goal(8, Box::new(BeeWanderGoal));
        goals.add_goal(9, Box::new(float));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(hurt_by_other));
        targets.add_goal(2, Box::new(become_angry));
        targets.add_goal(3, Box::new(reset_anger));
    }

    // ---- fields ----------------------------------------------------------------------------------------------------

    pub fn get_saved_flower_pos(&self) -> Option<BlockPos> {
        self.saved_flower_pos
    }

    pub fn has_saved_flower_pos(&self) -> bool {
        self.saved_flower_pos.is_some()
    }

    pub fn set_saved_flower_pos(&mut self, saved_flower_pos: BlockPos) {
        self.saved_flower_pos = Some(saved_flower_pos);
    }

    /// `getTravellingTicks()`.
    pub fn get_travelling_ticks(&self) -> i32 {
        self.go_to_hive.travelling_ticks.max(self.go_to_known_flower_travelling_ticks)
    }

    /// `getBlacklistedHives()`.
    pub fn get_blacklisted_hives(&self) -> &[BlockPos] {
        &self.go_to_hive.blacklisted_targets
    }

    fn is_tired_of_looking_for_nectar(&self) -> bool {
        self.ticks_without_nectar_since_exiting_hive > TICKS_WITHOUT_NECTAR_BEFORE_GOING_HOME
    }

    fn drop_hive(&mut self) {
        self.hive_pos = None;
        self.remaining_cooldown_before_locating_new_hive = COOLDOWN_BEFORE_LOCATING_NEW_HIVE;
    }

    fn drop_flower(&mut self) {
        self.saved_flower_pos = None;
        self.remaining_cooldown_before_locating_new_flower = mth::next_int(&mut self.mob.living.entity.random, MIN_FIND_FLOWER_RETRY_COOLDOWN, MAX_FIND_FLOWER_RETRY_COOLDOWN);
    }

    /// `wantsToEnterHive()`.
    fn wants_to_enter_hive(&self, level: &ServerLevel) -> bool {
        if self.stay_out_of_hive_countdown <= 0 && !self.pollinate.pollinating && !self.has_stung() && Entity::get_target(self).is_none() {
            let wants_to_enter_hive = self.has_nectar()
                || self.is_tired_of_looking_for_nectar()
                || level.attribute_bool_vec(crate::world::attribute::attributes::BEES_STAY_IN_HIVE, &self.position());
            wants_to_enter_hive && !self.is_hive_near_fire(level)
        } else {
            false
        }
    }

    pub fn set_stay_out_of_hive_countdown(&mut self, ticks: i32) {
        self.stay_out_of_hive_countdown = ticks;
    }

    /// `getRollAmount(a)`.
    pub fn get_roll_amount(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.roll_amount_o, self.roll_amount)
    }

    fn update_roll_amount(&mut self) {
        self.roll_amount_o = self.roll_amount;
        if self.is_rolling() {
            self.roll_amount = (self.roll_amount + 0.2).min(1.0);
        } else {
            self.roll_amount = (self.roll_amount - 0.24).max(0.0);
        }
    }

    pub fn reset_ticks_without_nectar_since_exiting_hive(&mut self) {
        self.ticks_without_nectar_since_exiting_hive = 0;
    }

    fn is_hive_near_fire(&self, level: &ServerLevel) -> bool {
        match self.get_beehive_block_entity(level) {
            Some(hive_pos) => level.get_block_entity(hive_pos).and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>()).is_some_and(|be| be.is_fire_nearby(level)),
            None => false,
        }
    }

    fn does_hive_have_space(&self, level: &ServerLevel, hive_pos: BlockPos) -> bool {
        level.get_block_entity(hive_pos).and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>()).is_some_and(|be| !be.is_full())
    }

    pub fn has_hive(&self) -> bool {
        self.hive_pos.is_some()
    }

    pub fn get_hive_pos(&self) -> Option<BlockPos> {
        self.hive_pos
    }

    pub fn get_crops_grown_since_pollination(&self) -> i32 {
        self.num_crops_grown_since_pollination
    }

    /// `getBeehiveBlockEntity()`: the position of the hive's block entity when it exists and is near enough.
    fn get_beehive_block_entity(&self, level: &ServerLevel) -> Option<BlockPos> {
        let hive_pos = self.hive_pos?;
        if self.is_too_far_away(hive_pos) {
            return None;
        }
        level.get_block_entity(hive_pos).filter(|be| be.as_any().is::<BeehiveBlockEntity>()).map(|_| hive_pos)
    }

    fn is_hive_valid(&self, level: &ServerLevel) -> bool {
        self.get_beehive_block_entity(level).is_some()
    }

    pub fn has_nectar(&self) -> bool {
        self.get_flag(FLAG_HAS_NECTAR)
    }

    fn set_has_nectar(&mut self, has_nectar: bool) {
        if has_nectar {
            self.reset_ticks_without_nectar_since_exiting_hive();
        }
        self.set_flag(FLAG_HAS_NECTAR, has_nectar);
    }

    pub fn has_stung(&self) -> bool {
        self.get_flag(FLAG_HAS_STUNG)
    }

    fn set_has_stung(&mut self, has_stung: bool) {
        self.set_flag(FLAG_HAS_STUNG, has_stung);
    }

    fn is_rolling(&self) -> bool {
        self.get_flag(FLAG_ROLL)
    }

    fn set_rolling(&mut self, rolling: bool) {
        self.set_flag(FLAG_ROLL, rolling);
    }

    fn is_too_far_away(&self, target_pos: BlockPos) -> bool {
        !self.closer_than(target_pos, TOO_FAR_DISTANCE)
    }

    fn closer_than(&self, target_pos: BlockPos, distance: i32) -> bool {
        target_pos.closer_than(&self.block_position(), f64::from(distance))
    }

    fn set_flag(&mut self, flag: i32, value: bool) {
        let current = i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID));
        let new = if value { current | flag } else { current & !flag };
        self.mob.living.entity.entity_data.set(DATA_FLAGS_ID, new as i8);
    }

    fn get_flag(&self, flag: i32) -> bool {
        (i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID)) & flag) != 0
    }

    pub fn is_flying(&self) -> bool {
        !self.base().on_ground()
    }

    /// `dropOffNectar()`.
    pub fn drop_off_nectar(&mut self) {
        self.set_has_nectar(false);
        self.num_crops_grown_since_pollination = 0;
    }

    pub fn set_hive_pos(&mut self, hive_pos: BlockPos) {
        self.hive_pos = Some(hive_pos);
    }

    /// `Bee.attractsBees(state)`.
    pub fn attracts_bees(state: BlockState) -> bool {
        if state.is_tag("bee_attractive") {
            if state.has_property(props::WATERLOGGED) && state.get(props::WATERLOGGED) {
                false
            } else if state.is_block(Block::SUNFLOWER) {
                state.get(props::DOUBLE_BLOCK_HALF) == mc_data::enums::DoubleBlockHalf::Upper
            } else {
                true
            }
        } else {
            false
        }
    }

    /// `pathfindRandomlyTowards(targetPos)`.
    fn pathfind_randomly_towards(&mut self, level: &mut ServerLevel, target_pos: BlockPos) {
        let target_vec = Vec3::at_bottom_center_of(&target_pos);
        let mut y_adjust = 0;
        let bee_pos = self.block_position();
        let y_delta = target_vec.y as i32 - bee_pos.y();
        if y_delta > 2 {
            y_adjust = 4;
        } else if y_delta < -2 {
            y_adjust = -4;
        }
        let mut xz_dist = 6;
        let mut y_dist = 8;
        let dist = bee_pos.dist_manhattan(&target_pos);
        if dist < 15 {
            xz_dist = dist / 2;
            y_dist = dist / 2;
        }
        if let Some(next_pos_towards) = air_random_pos::get_pos_towards(self, level, xz_dist, y_dist, y_adjust, target_vec, f64::from(PI_OVER_10)) {
            navigation::with_navigation(self, level, |n, _, _| n.set_max_visited_nodes_multiplier(0.5));
            navigation::move_to(self, level, next_pos_towards.x, next_pos_towards.y, next_pos_towards.z, 1.0);
        }
    }
}

fn is_bee_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("bee_food").is_some_and(|t| t.contains(stack.item()))
}

/// `Bee` of an entity running a bee goal.
fn bee(e: &mut dyn Entity) -> &mut Bee {
    e.as_any_mut().downcast_mut::<Bee>().expect("bee goal on a bee")
}

fn bee_ref(e: &dyn Entity) -> &Bee {
    e.as_any().downcast_ref::<Bee>().expect("bee")
}

impl crate::world::block_entity::BeeAccess for Bee {
    fn get_saved_flower_pos(&self) -> Option<BlockPos> {
        self.saved_flower_pos
    }
    fn set_saved_flower_pos(&mut self, pos: BlockPos) {
        self.saved_flower_pos = Some(pos);
    }
    fn set_hive_pos(&mut self, pos: BlockPos) {
        self.hive_pos = Some(pos);
    }
    fn drop_off_nectar(&mut self) {
        Bee::drop_off_nectar(self);
    }
    fn set_stay_out_of_hive_countdown(&mut self, ticks: i32) {
        self.stay_out_of_hive_countdown = ticks;
    }
    fn get_target(&self) -> Option<EntityId> {
        self.mob.target
    }
    fn set_target(&mut self, target: Option<EntityId>) {
        self.mob.target = target;
    }
    fn is_age_locked(&self) -> bool {
        mob::ageable::is_age_locked(self)
    }
    fn get_age(&self) -> i32 {
        mob::ageable::get_age(self)
    }
    fn set_age(&mut self, age: i32) {
        mob::ageable::set_age(self, age);
    }
    fn get_in_love_time(&self) -> i32 {
        mob::animal::get_in_love_time(self)
    }
    fn set_in_love_time(&mut self, ticks: i32) {
        self.mob.animal.as_mut().expect("animal").in_love = ticks;
    }
    fn drop_leash(&mut self, level: &mut ServerLevel) {
        mob::leashable::drop_leash(self, level);
    }
}

impl Entity for Bee {
    crate::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:bee")
    }

    fn as_bee_access(&mut self) -> Option<&mut dyn crate::world::block_entity::BeeAccess> {
        Some(self)
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn crate::world::level::LevelReader) -> f32 {
        if level.get_block_state(pos).is_air() { 10.0 } else { 0.0 }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        if let Some(pos) = self.hive_pos {
            out.put_int_array("hive_pos", vec![pos.x(), pos.y(), pos.z()]);
        }
        if let Some(pos) = self.saved_flower_pos {
            out.put_int_array("flower_pos", vec![pos.x(), pos.y(), pos.z()]);
        }
        out.put_boolean("HasNectar", self.has_nectar());
        out.put_boolean("HasStung", self.has_stung());
        out.put_int("TicksSincePollination", self.ticks_without_nectar_since_exiting_hive);
        out.put_int("CannotEnterHiveTicks", self.stay_out_of_hive_countdown);
        out.put_int("CropsGrownSincePollination", self.num_crops_grown_since_pollination);
        neutral::add_persistent_anger_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_has_nectar(input.get_boolean_or("HasNectar", false));
        self.set_has_stung(input.get_boolean_or("HasStung", false));
        self.ticks_without_nectar_since_exiting_hive = input.get_int_or("TicksSincePollination", 0);
        self.stay_out_of_hive_countdown = input.get_int_or("CannotEnterHiveTicks", 0);
        self.num_crops_grown_since_pollination = input.get_int_or("CropsGrownSincePollination", 0);
        self.hive_pos = read_block_pos(input, "hive_pos");
        self.saved_flower_pos = read_block_pos(input, "flower_pos");
        neutral::read_persistent_anger_save_data(self, None, input);
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: EntityId) -> bool {
        let damage_source = level.damage_sources().sting(self);
        let damage = mob::attribute_value(self, mc_data::Attribute::ATTACK_DAMAGE) as i32 as f32;
        let me = self.id();
        let difficulty = level.difficulty();
        let was_hurt = level
            .with_entity(target, |level, t| {
                let was_hurt = t.hurt_server(level, &damage_source, damage);
                if was_hurt {
                    crate::world::item::enchantment_helper::do_post_attack_effects(level, t, &damage_source);
                    if t.living().is_some() {
                        let stinger_count = t.base().entity_data.get(crate::world::entity::living::DATA_STINGER_COUNT_ID);
                        t.base_mut().entity_data.set(crate::world::entity::living::DATA_STINGER_COUNT_ID, stinger_count + 1);
                        let poison_time = match difficulty {
                            Difficulty::Normal => 10,
                            Difficulty::Hard => 18,
                            _ => 0,
                        };
                        if poison_time > 0 {
                            t.add_effect(level, MobEffectInstance::of(MobEffect::POISON, poison_time * 20, 0), Some(me));
                        }
                    }
                }
                was_hurt
            })
            .unwrap_or(false);
        if was_hurt {
            self.set_has_stung(true);
            neutral::stop_being_angry(self, level);
            self.play_sound(level, SoundEvent::ENTITY_BEE_STING, 1.0, 1.0);
        }
        was_hurt
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.has_nectar() && self.get_crops_grown_since_pollination() < MAX_CROPS_GROWABLE && self.base_mut().random.next_float() < 0.05 {
            // `for (i = 0; i < random.nextInt(2) + 1; i++) spawnFluidParticle(..)`: the loop condition draws every pass, the
            // particle arguments draw two doubles from the LEVEL random (`level.addParticle` is a no-op on the server)
            let mut i = 0;
            while i < self.base_mut().random.next_int_bounded(2) + 1 {
                let _ = level.random().next_double();
                let _ = level.random().next_double();
                i += 1;
            }
        }
        self.update_roll_amount();
    }

    fn custom_server_ai_step(&mut self, level: &mut ServerLevel) {
        let has_stung = self.has_stung();
        if self.is_in_water() {
            self.under_water_ticks += 1;
        } else {
            self.under_water_ticks = 0;
        }
        if self.under_water_ticks > 20 {
            hurt(level, self, |sources| sources.drown(), 1.0);
        }
        if has_stung {
            self.time_since_sting += 1;
            if self.time_since_sting % 5 == 0 && {
                let bound = mth::clamp_i32(1200 - self.time_since_sting, 1, 1200);
                self.base_mut().random.next_int_bounded(bound) == 0
            } {
                let health = self.get_health();
                hurt(level, self, |sources| sources.generic(), health);
            }
        }
        if !self.has_nectar() {
            self.ticks_without_nectar_since_exiting_hive += 1;
        }
        neutral::update_persistent_anger(self, level, false);
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        let (min, max) = PERSISTENT_ANGER_TIME;
        let time = self.base_mut().random.next_int_bounded(max - min + 1) + min;
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn ai_step(&mut self, level: &mut ServerLevel) {
        mob::tick::ai_step(self, level);
        if self.stay_out_of_hive_countdown > 0 {
            self.stay_out_of_hive_countdown -= 1;
        }
        if self.remaining_cooldown_before_locating_new_hive > 0 {
            self.remaining_cooldown_before_locating_new_hive -= 1;
        }
        if self.remaining_cooldown_before_locating_new_flower > 0 {
            self.remaining_cooldown_before_locating_new_flower -= 1;
        }
        let should_roll = neutral::is_angry(self, level)
            && !self.has_stung()
            && self.get_target().and_then(|t| level.get_entity(t)).is_some_and(|t| t.base().distance_to_sqr_vec(self.position()) < 4.0);
        self.set_rolling(should_roll);
        if self.base().tick_count % 20 == 0 && !self.is_hive_valid(level) {
            self.hive_pos = None;
        }
    }

    fn mob_interact(&mut self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        let held_item = player.get_item_in_hand(hand);
        if is_bee_food(&held_item)
            && let Some(block) = held_item.item().as_block()
            && let Some(effect) = crate::world::level::block::behaviour(block).get_bee_interaction_effect()
        {
            mob::interact::use_player_item(level, player, hand);
            self.add_effect(level, effect, None);
            return InteractionResult::SUCCESS;
        }
        mob::interact::mob_interact(self, level, player, hand)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_bee_food(stack)
    }

    fn play_step_sound(&mut self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {}

    fn get_ambient_sound(&self) -> Option<SoundEvent> {
        None
    }

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BEE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BEE_DEATH)
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn get_breed_offspring(&mut self, level: &mut ServerLevel, _partner: EntityId) -> Option<Box<dyn Entity>> {
        factory::create(EntityType::BEE, level, EntitySpawnRequest { reason: EntitySpawnReason::Breeding, ignore_checks: false })
    }

    fn check_fall_damage(&mut self, _level: &mut ServerLevel, _ya: f64, _on_ground: bool, _on_state: BlockState, _pos: BlockPos) {}

    fn is_flapping(&self) -> bool {
        self.is_flying() && self.base().tick_count % TICKS_PER_FLAP == 0
    }

    fn omnidirectional_air_mover(&self) -> bool {
        true
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, damage: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            false
        } else {
            self.pollinate.pollinating = false;
            crate::world::entity::living_entity::damage::hurt_server(self, level, source, damage)
        }
    }

    fn jump_in_liquid(&mut self, _fluid: crate::world::entity::living_entity::util::FluidKind) {
        let dm = self.base().delta_movement().add(Vec3::new(0.0, 0.01, 0.0));
        self.base_mut().set_delta_movement(dm);
    }
}

/// `BlockPos.CODEC` read of an optional int-array field.
fn read_block_pos(input: &CompoundTag, key: &str) -> Option<BlockPos> {
    let array = input.get_int_array(key)?;
    (array.len() == 3).then(|| BlockPos::new(array[0], array[1], array[2]))
}

// ==== navigation / look control ======================================================================================

/// The anonymous `FlyingPathNavigation` subclass of `Bee.createNavigation`.
pub struct BeeNavigation {
    data: NavigationData,
}

impl BeeNavigation {
    fn new(bee: &Bee) -> Self {
        BeeNavigation { data: NavigationData::new(bee, NavClass::Flying) }
    }
}

impl PathNavigation for BeeNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }

    /// `isStableDestination(pos)`: `!level.getBlockState(pos.below()).isAir()`.
    fn is_stable_destination(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        !level.get_block_state(pos.below()).is_air()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &ServerLevel) {
        if !bee_ref(mob).pollinate.pollinating {
            crate::world::entity::ai::navigation::default_tick(self, mob, level);
        }
    }
}

/// `Bee.BeeLookControl`.
#[derive(Default)]
struct BeeLookControl {
    base: LookControlBase,
    /// `beePollinateGoal.isPollinating()`, read from the bee at the start of every tick (the control is taken out of the
    /// bee while it ticks, so `resetXRotOnTick()` cannot reach it).
    pollinating: bool,
}

impl LookControl for BeeLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !neutral::is_angry(mob, level) {
            self.pollinating = bee_ref(mob).pollinate.pollinating;
            look_control::default_tick(self, mob, level);
        }
    }

    fn reset_x_rot_on_tick(&self) -> bool {
        !self.pollinating
    }
}

// ==== goals ===========================================================================================================

/// `Bee.BaseBeeGoal`: `canUse() = canBeeUse() && !isAngry()`.
trait BeeGoal: 'static {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool;
    fn can_bee_continue_to_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool;
    fn start(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) {}
    fn stop(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) {}
    fn tick(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) {}
    fn requires_update_every_tick(&self) -> bool {
        false
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
    fn adjusted(&self, ticks: i32) -> i32 {
        if self.requires_update_every_tick() { ticks } else { crate::world::entity::ai::goal::reduced_tick_delay(ticks) }
    }
}

struct BaseBee<G: BeeGoal>(G);

impl<G: BeeGoal> Goal for BaseBee<G> {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.0.can_bee_use(bee(mob), level) && !neutral::is_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.0.can_bee_continue_to_use(bee(mob), level) && !neutral::is_angry(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.0.start(bee(mob), level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.0.stop(bee(mob), level);
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.0.tick(bee(mob), level);
    }
    fn requires_update_every_tick(&self) -> bool {
        self.0.requires_update_every_tick()
    }
    fn flags(&self) -> GoalFlags {
        self.0.flags()
    }
}

/// `Bee.BeeAttackGoal` (`MeleeAttackGoal` with the anger / sting conditions).
struct BeeAttackGoal {
    inner: MeleeAttackGoal,
}

impl BeeAttackGoal {
    fn new(_bee: &mut Bee, speed_modifier: f64, track_target: bool) -> Self {
        Self { inner: MeleeAttackGoal::new(speed_modifier, track_target) }
    }
}

impl Goal for BeeAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level) && neutral::is_angry(mob, level) && !bee(mob).has_stung()
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level) && neutral::is_angry(mob, level) && !bee(mob).has_stung()
    }
    fn is_interruptable(&self) -> bool {
        self.inner.is_interruptable()
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }
    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Bee.BeeBecomeAngryTargetGoal`: `NearestAttackableTargetGoal<Player>(bee, Player.class, 10, true, false, bee::isAngryAt)`.
struct BeeBecomeAngryTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl BeeBecomeAngryTargetGoal {
    fn new(_bee: &mut Bee) -> Self {
        Self { inner: NearestAttackableTargetGoal::new_player(10, true, false, Some(std::sync::Arc::new(|target: &dyn Entity, level: &ServerLevel| target_is_angry_at(target, level)))) }
    }

    fn bee_can_target(mob: &mut dyn Entity, level: &ServerLevel) -> bool {
        neutral::is_angry(mob, level) && !bee(mob).has_stung()
    }
}

/// `bee::isAngryAt`: the selector is evaluated for a target with the bee as the targeter (see the goal's selector hook).
fn target_is_angry_at(_target: &dyn Entity, _level: &ServerLevel) -> bool {
    true
}

impl Goal for BeeBecomeAngryTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::bee_can_target(mob, level) && self.inner.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let bee_can_target = Self::bee_can_target(mob, level);
        if bee_can_target && mob.get_target().is_some() {
            self.inner.can_continue_to_use(mob, level)
        } else {
            self.inner.clear_target_mob();
            false
        }
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

/// `Bee.BeeHurtByOtherGoal`.
struct BeeHurtByOtherGoal {
    inner: HurtByTargetGoal,
}

impl BeeHurtByOtherGoal {
    fn new(_bee: &mut Bee) -> Self {
        Self { inner: HurtByTargetGoal::new(&[]).set_alert_others_with(|mob, level, other, hurt_by_mob| {
            // `alertOther(other, hurtByMob)`: `other instanceof Bee && this.mob.hasLineOfSight(hurtByMob)`
            if other.as_any().is::<Bee>() && mob::ext::has_line_of_sight(level, mob, hurt_by_mob) {
                Some(hurt_by_mob.id())
            } else {
                None
            }
        }) }
    }
}

impl Goal for BeeHurtByOtherGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        neutral::is_angry(mob, level) && self.inner.can_continue_to_use(mob, level)
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

/// `Bee.BeeEnterHiveGoal`.
struct BeeEnterHiveGoal;

impl BeeGoal for BeeEnterHiveGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        if let Some(hive_pos) = bee.hive_pos
            && bee.wants_to_enter_hive(level)
            && hive_pos.closer_to_center_than(&bee.position(), 2.0)
        {
            if bee.get_beehive_block_entity(level).is_some() {
                if level.get_block_entity(hive_pos).and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>()).is_some_and(|be| !be.is_full()) {
                    return true;
                }
                bee.hive_pos = None;
            }
        }
        false
    }

    fn can_bee_continue_to_use(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        if let Some(hive_pos) = bee.get_beehive_block_entity(level) {
            level.with_block_entity(hive_pos, |level, be| {
                if let Some(hive) = be.as_any_mut().downcast_mut::<BeehiveBlockEntity>() {
                    hive.add_occupant(level, bee);
                }
            });
        }
    }
}

/// `Bee.BeeGoToHiveGoal`.
struct BeeGoToHiveGoal;

impl BeeGoToHiveGoal {
    fn has_reached_target(bee: &mut Bee, level: &mut ServerLevel, target_pos: BlockPos) -> bool {
        if bee.closer_than(target_pos, 2) {
            return true;
        }
        let path = navigation::with_navigation_ref(bee, level, |n| n.get_path()).flatten();
        path.is_some_and(|p| p.get_target() == target_pos && p.can_reach() && p.is_done())
    }

    fn drop_and_blacklist_hive(bee: &mut Bee) {
        if let Some(hive_pos) = bee.hive_pos {
            bee.go_to_hive.blacklisted_targets.push(hive_pos);
            while bee.go_to_hive.blacklisted_targets.len() > 3 {
                bee.go_to_hive.blacklisted_targets.remove(0);
            }
        }
        bee.drop_hive();
    }

    fn pathfind_directly_towards(bee: &mut Bee, level: &mut ServerLevel, target_pos: BlockPos) -> bool {
        let close_enough = if bee.closer_than(target_pos, 3) { 1 } else { 2 };
        navigation::with_navigation(bee, level, |n, mob, level| {
            n.set_max_visited_nodes_multiplier(10.0);
            n.move_to_xyz_reach(mob, &*level, f64::from(target_pos.x()), f64::from(target_pos.y()), f64::from(target_pos.z()), close_enough, 1.0);
        });
        navigation::with_navigation_ref(bee, level, |n| n.get_path()).flatten().is_some_and(|p| p.can_reach())
    }
}

impl BeeGoal for BeeGoToHiveGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        match bee.hive_pos {
            Some(hive_pos) => {
                !bee.is_too_far_away(hive_pos)
                    && !bee.mob.has_home()
                    && bee.wants_to_enter_hive(level)
                    && !Self::has_reached_target(bee, level, hive_pos)
                    && level.get_block_state(hive_pos).is_tag("beehives")
            }
            None => false,
        }
    }

    fn can_bee_continue_to_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        self.can_bee_use(bee, level)
    }

    fn start(&mut self, bee: &mut Bee, _level: &mut ServerLevel) {
        bee.go_to_hive.travelling_ticks = 0;
        bee.go_to_hive.ticks_stuck = 0;
    }

    fn stop(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        bee.go_to_hive.travelling_ticks = 0;
        bee.go_to_hive.ticks_stuck = 0;
        navigation::stop(bee, level);
        navigation::with_navigation(bee, level, |n, _, _| n.reset_max_visited_nodes_multiplier());
    }

    fn tick(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        let Some(hive_pos) = bee.hive_pos else { return };
        bee.go_to_hive.travelling_ticks += 1;
        if bee.go_to_hive.travelling_ticks > self.adjusted(MAX_TRAVELLING_TICKS) {
            Self::drop_and_blacklist_hive(bee);
        } else if !navigation::is_in_progress(bee, level) {
            if !bee.closer_than(hive_pos, PATHFIND_TO_HIVE_WHEN_CLOSER_THAN) {
                if bee.is_too_far_away(hive_pos) {
                    bee.drop_hive();
                } else {
                    bee.pathfind_randomly_towards(level, hive_pos);
                }
            } else {
                let can_reach_all_the_way_to_target = Self::pathfind_directly_towards(bee, level, hive_pos);
                if !can_reach_all_the_way_to_target {
                    Self::drop_and_blacklist_hive(bee);
                } else {
                    let current = navigation::with_navigation_ref(bee, level, |n| n.get_path()).flatten();
                    if bee.go_to_hive.last_path.is_some() && current.as_ref().is_some_and(|p| p.same_as(bee.go_to_hive.last_path.as_ref())) {
                        bee.go_to_hive.ticks_stuck += 1;
                        if bee.go_to_hive.ticks_stuck > 60 {
                            bee.drop_hive();
                            bee.go_to_hive.ticks_stuck = 0;
                        }
                    } else {
                        bee.go_to_hive.last_path = current;
                    }
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeGoToKnownFlowerGoal`.
struct BeeGoToKnownFlowerGoal;

impl BeeGoal for BeeGoToKnownFlowerGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, _level: &mut ServerLevel) -> bool {
        match bee.saved_flower_pos {
            Some(flower) => !bee.mob.has_home() && bee.ticks_without_nectar_since_exiting_hive > TICKS_BEFORE_GOING_TO_KNOWN_FLOWER && !bee.closer_than(flower, 2),
            None => false,
        }
    }

    fn can_bee_continue_to_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        self.can_bee_use(bee, level)
    }

    fn start(&mut self, bee: &mut Bee, _level: &mut ServerLevel) {
        bee.go_to_known_flower_travelling_ticks = 0;
    }

    fn stop(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        bee.go_to_known_flower_travelling_ticks = 0;
        navigation::stop(bee, level);
        navigation::with_navigation(bee, level, |n, _, _| n.reset_max_visited_nodes_multiplier());
    }

    fn tick(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        let Some(flower) = bee.saved_flower_pos else { return };
        bee.go_to_known_flower_travelling_ticks += 1;
        if bee.go_to_known_flower_travelling_ticks > self.adjusted(MAX_TRAVELLING_TICKS) {
            bee.drop_flower();
        } else if !navigation::is_in_progress(bee, level) {
            if bee.is_too_far_away(flower) {
                bee.drop_flower();
            } else {
                bee.pathfind_randomly_towards(level, flower);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeGrowCropGoal`.
struct BeeGrowCropGoal;

impl BeeGoal for BeeGrowCropGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        if bee.get_crops_grown_since_pollination() >= MAX_CROPS_GROWABLE {
            false
        } else if bee.base_mut().random.next_float() < 0.3 {
            false
        } else {
            bee.has_nectar() && bee.is_hive_valid(level)
        }
    }

    fn can_bee_continue_to_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        self.can_bee_use(bee, level)
    }

    fn tick(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        let bound = self.adjusted(30);
        if bee.base_mut().random.next_int_bounded(bound) != 0 {
            return;
        }
        for i in 1..=2 {
            let below_pos = bee.block_position().below_n(i);
            let below_state = level.get_block_state(below_pos);
            let mut grow_state: Option<BlockState> = None;
            if below_state.is_tag("bee_growables") {
                if crate::world::level::block::helpers::is_instance_of(below_state, "CropBlock") {
                    grow_state = crate::world::level::block::impls::crop_block::next_age_state_if_not_max(below_state);
                } else if crate::world::level::block::helpers::is_instance_of(below_state, "StemBlock") {
                    let age = below_state.get(props::AGE_7);
                    if age < 7 {
                        grow_state = Some(below_state.set(props::AGE_7, age + 1));
                    }
                } else if below_state.is_block(Block::SWEET_BERRY_BUSH) {
                    let age = below_state.get(props::AGE_3);
                    if age < 3 {
                        grow_state = Some(below_state.set(props::AGE_3, age + 1));
                    }
                } else if below_state.is_block(Block::CAVE_VINES) || below_state.is_block(Block::CAVE_VINES_PLANT) {
                    let behaviour = crate::world::level::block::behaviour(below_state.block());
                    use crate::world::level::block::bonemeal::BonemealSource;
                    if behaviour.is_valid_bonemeal_target(&*level, below_pos, below_state, BonemealSource::Mob) {
                        behaviour.perform_bonemeal(level, below_pos, below_state, BonemealSource::Mob);
                        grow_state = Some(level.get_block_state(below_pos));
                    }
                }
                if let Some(grow_state) = grow_state {
                    level.level_event_impl(None, 2011, below_pos, 15);
                    level.set_block_and_update(below_pos, grow_state);
                    bee.num_crops_grown_since_pollination += 1;
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Bee.BeeLocateHiveGoal`.
struct BeeLocateHiveGoal;

impl BeeLocateHiveGoal {
    /// `findNearbyHivesWithSpace()`.
    fn find_nearby_hives_with_space(bee: &Bee, level: &ServerLevel) -> Vec<BlockPos> {
        let bee_pos = bee.block_position();
        let mut hives: Vec<BlockPos> = level
            .poi_manager()
            .get_in_range(crate::world::entity::ai::village::poi::PoiTypePredicate::Tag("bee_home"), bee_pos, HIVE_SEARCH_DISTANCE, crate::world::entity::ai::village::poi::Occupancy::Any)
            .into_iter()
            .map(|r| r.pos)
            .filter(|p| bee.does_hive_have_space(level, *p))
            .collect();
        // `sorted(Comparator.comparingDouble(pos -> pos.distSqr(beePos)))`: stable
        hives.sort_by(|a, b| a.dist_sqr(&bee_pos).total_cmp(&b.dist_sqr(&bee_pos)));
        hives
    }
}

impl BeeGoal for BeeLocateHiveGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        bee.remaining_cooldown_before_locating_new_hive == 0 && !bee.has_hive() && bee.wants_to_enter_hive(level)
    }

    fn can_bee_continue_to_use(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        bee.remaining_cooldown_before_locating_new_hive = COOLDOWN_BEFORE_LOCATING_NEW_HIVE;
        let hives_with_space = Self::find_nearby_hives_with_space(bee, level);
        if !hives_with_space.is_empty() {
            for pos_to_check in &hives_with_space {
                if !bee.go_to_hive.blacklisted_targets.contains(pos_to_check) {
                    bee.hive_pos = Some(*pos_to_check);
                    return;
                }
            }
            bee.go_to_hive.blacklisted_targets.clear();
            bee.hive_pos = Some(hives_with_space[0]);
        }
    }
}

/// `Bee.BeePollinateGoal`.
struct BeePollinateGoal;

impl BeePollinateGoal {
    fn has_pollinated_long_enough(bee: &Bee) -> bool {
        bee.pollinate.successful_pollinating_ticks > 400
    }

    fn set_wanted_pos(bee: &mut Bee) {
        if let Some(hover) = bee.pollinate.hover_pos {
            mob::set_wanted_position(bee, hover.x, hover.y, hover.z, f64::from(0.35f32));
        }
    }

    fn get_offset(bee: &mut Bee) -> f32 {
        (bee.base_mut().random.next_float() * 2.0 - 1.0) * 0.33333334
    }

    /// `findNearbyFlower()`.
    fn find_nearby_flower(bee: &mut Bee, level: &mut ServerLevel) -> Option<BlockPos> {
        let mut temp_cache: HashMap<i64, i64> = HashMap::new();
        let origin = bee.block_position();
        let mut result = None;
        for pos in BlockPos::within_box_by_manhattan_distance(&origin, 5, 5, 5) {
            let state = level.get_block_state(pos);
            if !Bee::attracts_bees(state) {
                continue;
            }
            let unreachable_until_time = bee.pollinate.unreachable_flower_cache.get(&pos.as_long()).copied().unwrap_or(i64::MIN);
            if level.game_time() < unreachable_until_time {
                temp_cache.insert(pos.as_long(), unreachable_until_time);
                continue;
            }
            let path = navigation::create_path_pos(bee, level, pos, 1);
            if path.is_some_and(|p| p.can_reach()) {
                result = Some(pos);
                break;
            }
            temp_cache.insert(pos.as_long(), level.game_time() + 600);
        }
        bee.pollinate.unreachable_flower_cache = temp_cache;
        result
    }
}

impl BeeGoal for BeePollinateGoal {
    fn can_bee_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        if bee.remaining_cooldown_before_locating_new_flower > 0 {
            false
        } else if bee.has_nectar() {
            false
        } else if level.is_raining() {
            false
        } else {
            match Self::find_nearby_flower(bee, level) {
                Some(nearby_pos) => {
                    bee.saved_flower_pos = Some(nearby_pos);
                    navigation::move_to(bee, level, f64::from(nearby_pos.x()) + 0.5, f64::from(nearby_pos.y()) + 0.5, f64::from(nearby_pos.z()) + 0.5, f64::from(1.2f32));
                    true
                }
                None => {
                    bee.remaining_cooldown_before_locating_new_flower = mth::next_int(&mut bee.mob.living.entity.random, MIN_FIND_FLOWER_RETRY_COOLDOWN, MAX_FIND_FLOWER_RETRY_COOLDOWN);
                    false
                }
            }
        }
    }

    fn can_bee_continue_to_use(&mut self, bee: &mut Bee, level: &mut ServerLevel) -> bool {
        if !bee.pollinate.pollinating {
            false
        } else if !bee.has_saved_flower_pos() {
            false
        } else if level.is_raining() {
            false
        } else if Self::has_pollinated_long_enough(bee) {
            bee.base_mut().random.next_float() < 0.2
        } else {
            true
        }
    }

    fn start(&mut self, bee: &mut Bee, _level: &mut ServerLevel) {
        bee.pollinate.successful_pollinating_ticks = 0;
        bee.pollinate.pollinating_ticks = 0;
        bee.pollinate.last_sound_played_tick = 0;
        bee.pollinate.pollinating = true;
        bee.reset_ticks_without_nectar_since_exiting_hive();
    }

    fn stop(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        if Self::has_pollinated_long_enough(bee) {
            bee.set_has_nectar(true);
        }
        bee.pollinate.pollinating = false;
        navigation::stop(bee, level);
        bee.remaining_cooldown_before_locating_new_flower = COOLDOWN_BEFORE_LOCATING_NEW_FLOWER;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        let Some(saved_flower_pos) = bee.saved_flower_pos else { return };
        bee.pollinate.pollinating_ticks += 1;
        if bee.pollinate.pollinating_ticks > 600 {
            bee.drop_flower();
            bee.pollinate.pollinating = false;
            bee.remaining_cooldown_before_locating_new_flower = COOLDOWN_BEFORE_LOCATING_NEW_FLOWER;
        } else {
            let flower_pos = Vec3::at_bottom_center_of(&saved_flower_pos).add(Vec3::new(0.0, f64::from(0.6f32), 0.0));
            if flower_pos.distance_to(bee.position()) > 1.0 {
                bee.pollinate.hover_pos = Some(flower_pos);
                Self::set_wanted_pos(bee);
            } else {
                if bee.pollinate.hover_pos.is_none() {
                    bee.pollinate.hover_pos = Some(flower_pos);
                }
                let hover_pos = bee.pollinate.hover_pos.expect("hover position was just set");
                let arrived_at_hover_pos = bee.position().distance_to(hover_pos) <= 0.1;
                let mut should_set_wanted_pos = true;
                if !arrived_at_hover_pos && bee.pollinate.pollinating_ticks > 600 {
                    bee.drop_flower();
                } else {
                    if arrived_at_hover_pos {
                        let should_change_hover_positions = bee.base_mut().random.next_int_bounded(25) == 0;
                        if should_change_hover_positions {
                            let dx = f64::from(Self::get_offset(bee));
                            let dz = f64::from(Self::get_offset(bee));
                            bee.pollinate.hover_pos = Some(Vec3::new(flower_pos.x + dx, flower_pos.y, flower_pos.z + dz));
                            navigation::stop(bee, level);
                        } else {
                            should_set_wanted_pos = false;
                        }
                        mob::look_at_xyz(bee, flower_pos.x, flower_pos.y, flower_pos.z);
                    }
                    if should_set_wanted_pos {
                        Self::set_wanted_pos(bee);
                    }
                    bee.pollinate.successful_pollinating_ticks += 1;
                    if bee.base_mut().random.next_float() < 0.05 && bee.pollinate.successful_pollinating_ticks > bee.pollinate.last_sound_played_tick + 60 {
                        bee.pollinate.last_sound_played_tick = bee.pollinate.successful_pollinating_ticks;
                        bee.play_sound(level, SoundEvent::ENTITY_BEE_POLLINATE, 1.0, 1.0);
                    }
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeWanderGoal`.
struct BeeWanderGoal;

impl BeeWanderGoal {
    /// `getWanderThreshold()`.
    fn get_wander_threshold(bee: &Bee) -> i32 {
        let distance_reduction = if !bee.has_hive() && !bee.has_saved_flower_pos() { DEFAULT_WANDER_DISTANCE_REDUCTION } else { RESTRICTED_WANDER_DISTANCE_REDUCTION };
        TOO_FAR_DISTANCE - distance_reduction
    }

    /// `findPos()`.
    fn find_pos(bee: &mut Bee, level: &mut ServerLevel) -> Option<Vec3> {
        let wander_direction = match bee.hive_pos {
            Some(hive_pos) if bee.is_hive_valid(level) && !bee.closer_than(hive_pos, Self::get_wander_threshold(bee)) => {
                let hive_pos_vec = Vec3::at_center_of(&hive_pos);
                hive_pos_vec.subtract(bee.position()).normalize()
            }
            _ => bee.get_view_vector(0.0),
        };
        let ground_based_position = hover_random_pos::get_pos(bee, level, 8, 7, wander_direction.x, wander_direction.z, PI_OVER_2, 3, 1);
        match ground_based_position {
            Some(pos) => Some(pos),
            None => air_and_water_random_pos::get_pos(bee, level, 8, 4, -2, wander_direction.x, wander_direction.z, f64::from(PI_OVER_2)),
        }
    }
}

impl Goal for BeeWanderGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        navigation::is_done(mob, level) && mob.base_mut().random.next_int_bounded(10) == 0
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        navigation::is_in_progress(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target_pos = Self::find_pos(bee(mob), level);
        if let Some(target_pos) = target_pos {
            let path = navigation::create_path_pos(mob, level, BlockPos::containing_position(&target_pos), 1);
            navigation::move_to_path(mob, level, path, 1.0);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.ValidateFlowerGoal`.
struct ValidateFlowerGoal {
    validate_flower_cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateFlowerGoal {
    fn new(bee: &mut Bee) -> Self {
        Self { validate_flower_cooldown: mth::next_int(&mut bee.mob.living.entity.random, 20, 40), last_validate_tick: -1 }
    }
}

impl BeeGoal for ValidateFlowerGoal {
    fn start(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        if let Some(flower) = bee.saved_flower_pos
            && level.is_loaded(flower)
            && !Bee::attracts_bees(level.get_block_state(flower))
        {
            bee.drop_flower();
        }
        self.last_validate_tick = level.game_time();
    }

    fn can_bee_use(&mut self, _bee: &mut Bee, level: &mut ServerLevel) -> bool {
        level.game_time() > self.last_validate_tick + i64::from(self.validate_flower_cooldown)
    }

    fn can_bee_continue_to_use(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) -> bool {
        false
    }
}

/// `Bee.ValidateHiveGoal`.
struct ValidateHiveGoal {
    validate_hive_cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateHiveGoal {
    fn new(bee: &mut Bee) -> Self {
        Self { validate_hive_cooldown: mth::next_int(&mut bee.mob.living.entity.random, 20, 40), last_validate_tick: -1 }
    }
}

impl BeeGoal for ValidateHiveGoal {
    fn start(&mut self, bee: &mut Bee, level: &mut ServerLevel) {
        if let Some(hive_pos) = bee.hive_pos
            && level.is_loaded(hive_pos)
            && !bee.is_hive_valid(level)
        {
            bee.drop_hive();
        }
        self.last_validate_tick = level.game_time();
    }

    fn can_bee_use(&mut self, _bee: &mut Bee, level: &mut ServerLevel) -> bool {
        level.game_time() > self.last_validate_tick + i64::from(self.validate_hive_cooldown)
    }

    fn can_bee_continue_to_use(&mut self, _bee: &mut Bee, _level: &mut ServerLevel) -> bool {
        false
    }
}

#[distributed_slice(ENTITY_FACTORIES)]
static BEE: EntityRegistration = EntityRegistration { entity_type: EntityType::BEE, create: |ty, id, _registries| Box::new(Bee::new(ty, id)) };

#[allow(dead_code)]
fn _unused(e: &mut dyn Entity) {
    let _ = l(e);
    let _ = lm(e);
    let _ = make_sound;
    let _ = HIVE_CLOSE_ENOUGH_DISTANCE;
    let _ = PATHFIND_TO_HIVE_WHEN_CLOSER_THAN;
}
