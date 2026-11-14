//! `Bee`: flying neutral animal that pollinates flowers, grows crops and lives in hives.
//!
//! Not implemented: `registerDebugValues` (the bee debug subscription).

use super::bee_goals::{
    BeeAttackGoal, BeeBecomeAngryTargetGoal, BeeEnterHiveGoal, BeeGoToHiveGoal,
    BeeGoToKnownFlowerGoal, BeeGrowCropGoal, BeeHurtByOtherGoal, BeeLocateHiveGoal,
    BeePollinateGoal, BeeWanderGoal, ValidateFlowerGoal, ValidateHiveGoal,
};
use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource as _;
use mc_data::block::{Block, BlockState};
use mc_data::enums::{DoubleBlockHalf, EntitySpawnReason, Pose};
use mc_data::{Attribute, EntityType, Item, MobEffect, SoundEvent, props};
use mc_game_core::define_accessor;
use mc_game_core::registry::RegistryAccess;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageSource;
use mc_game_core::world::entity::ai::control::{
    FlyingMoveControl, LookControl, LookControlBase, look_control,
};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, ResetUniversalAngerTargetGoal, TemptGoal,
};
use mc_game_core::world::entity::ai::navigation::{
    NavClass, NavigationData, PathNavigation, default_tick,
};
use mc_game_core::world::entity::animal::bee_access::BeeAccess;
use mc_game_core::world::entity::dimensions::EntityDimensions;
use mc_game_core::world::entity::effect::MobEffectInstance;
use mc_game_core::world::entity::factory::{
    ENTITY_FACTORIES, EntityRegistration, EntitySpawnRequest,
};
use mc_game_core::world::entity::living::DATA_STINGER_COUNT_ID;
use mc_game_core::world::entity::living_entity::{damage, util::FluidKind};
use mc_game_core::world::entity::mob::{self, MobData, MobKind, interact, neutral};
use mc_game_core::world::entity::scenario_dump::{SCENARIO_DUMP_EXTRA, ScenarioDumpExtra};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::item::enchantment::actors::Actors;
use mc_game_core::world::item::enchantment_helper;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::pathfinder::PathType;
use mc_game_core::world::level::{BlockGetter as _, LevelAccessor as _, LevelReader};
use mc_nbt::CompoundTag;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

define_accessor!(pub DATA_FLAGS_ID: Byte = Bee);
define_accessor!(pub DATA_ANGER_END_TIME: Long = Bee);

const FLAG_ROLL: i32 = 2;
const FLAG_HAS_STUNG: i32 = 4;
const FLAG_HAS_NECTAR: i32 = 8;
/// `TICKS_PER_FLAP = Mth.ceil(1.4959966F)`.
pub const TICKS_PER_FLAP: i32 = 2;
const TOO_FAR_DISTANCE: i32 = 48;

/// `Bee`.
pub struct Bee {
    pub mob: MobData,
    roll_amount: f32,
    roll_amount_o: f32,
    time_since_sting: i32,
    pub(super) ticks_without_nectar_since_exiting_hive: i32,
    pub(super) stay_out_of_hive_countdown: i32,
    num_crops_grown_since_pollination: i32,
    pub(super) remaining_cooldown_before_locating_new_hive: i32,
    pub(super) remaining_cooldown_before_locating_new_flower: i32,
    pub(super) saved_flower_pos: Option<BlockPos>,
    pub(super) hive_pos: Option<BlockPos>,
    under_water_ticks: i32,
    /// `beePollinateGoal.isPollinating()` (read by the navigation, the look control and `wantsToEnterHive`).
    pub(super) pollinating: bool,
    /// `goToHiveGoal.blacklistedTargets` (read by `BeeLocateHiveGoal`).
    pub(super) blacklisted_hives: Vec<BlockPos>,
}

pub(super) fn bee(e: &dyn Entity) -> &Bee {
    e.as_any().downcast_ref::<Bee>().expect("Bee")
}

pub(super) fn bee_mut(e: &mut dyn Entity) -> &mut Bee {
    e.as_any_mut().downcast_mut::<Bee>().expect("Bee")
}

/// `Bee.attractsBees(state)`.
pub fn attracts_bees(state: BlockState) -> bool {
    if !state.is_tag("bee_attractive") {
        return false;
    }
    if state.try_get(props::WATERLOGGED).unwrap_or(false) {
        return false;
    }
    state.block() != Block::SUNFLOWER
        || state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper
}

impl Bee {
    pub fn new(ty: EntityType, id: EntityId, _registries: &RegistryAccess) -> Self {
        let mob = MobData::new(ty, id, MobKind::Animal, |b| {
            b.define(DATA_FLAGS_ID, 0i8);
            b.define(DATA_ANGER_END_TIME, -1i64);
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
            under_water_ticks: 0,
            pollinating: false,
            blacklisted_hives: Vec::new(),
        };
        bee.mob.neutral = Some(neutral::NeutralData {
            end_time_accessor: Some(DATA_ANGER_END_TIME),
            ..neutral::NeutralData::default()
        });
        let mut navigation = BeeNavigation {
            data: NavigationData::new(&bee, NavClass::Flying),
        };
        navigation.set_can_open_doors(false);
        navigation.set_can_float(false);
        navigation.set_required_path_length(&bee, 48.0);
        bee.mob.set_navigation(Box::new(navigation));
        bee.register_goals();
        // field initialiser, after the goals drew their validation intervals
        bee.remaining_cooldown_before_locating_new_flower =
            bee.base_mut().random.next_int_between_inclusive(20, 60);
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
        let breed = BreedGoal::new(self, 1.0);
        let follow_parent = FollowParentGoal::new(self, 1.25);
        let float = FloatGoal::new(self);
        // the validation goals draw their interval from the bee's random in this order
        let validate_hive = ValidateHiveGoal::new(&mut self.base_mut().random);
        let validate_flower = ValidateFlowerGoal::new(&mut self.base_mut().random);
        let goals = &mut self.mob.goal_selector;
        goals.add_goal(0, Box::new(BeeAttackGoal::new()));
        goals.add_goal(1, Box::new(BeeEnterHiveGoal));
        goals.add_goal(2, Box::new(breed));
        goals.add_goal(
            3,
            Box::new(TemptGoal::new(
                1.25,
                Arc::new(|stack: &ItemStack| is_bee_food(stack)),
                false,
            )),
        );
        goals.add_goal(3, Box::new(validate_hive));
        goals.add_goal(3, Box::new(validate_flower));
        goals.add_goal(4, Box::new(BeePollinateGoal::new()));
        goals.add_goal(5, Box::new(follow_parent));
        goals.add_goal(5, Box::new(BeeLocateHiveGoal));
        goals.add_goal(5, Box::new(BeeGoToHiveGoal::new()));
        goals.add_goal(6, Box::new(BeeGoToKnownFlowerGoal::new()));
        goals.add_goal(7, Box::new(BeeGrowCropGoal));
        goals.add_goal(8, Box::new(BeeWanderGoal));
        goals.add_goal(9, Box::new(float));
        let targets = &mut self.mob.target_selector;
        targets.add_goal(1, Box::new(BeeHurtByOtherGoal::new()));
        targets.add_goal(2, Box::new(BeeBecomeAngryTargetGoal::new()));
        targets.add_goal(3, Box::new(ResetUniversalAngerTargetGoal::new(true)));
    }

    fn get_flag(&self, flag: i32) -> bool {
        i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID)) & flag != 0
    }

    fn set_flag(&mut self, flag: i32, value: bool) {
        let flags = i32::from(self.mob.living.entity.entity_data.get(DATA_FLAGS_ID));
        let flags = if value { flags | flag } else { flags & !flag };
        self.mob
            .living
            .entity
            .entity_data
            .set(DATA_FLAGS_ID, flags as i8);
    }

    pub fn has_nectar(&self) -> bool {
        self.get_flag(FLAG_HAS_NECTAR)
    }

    pub(super) fn set_has_nectar(&mut self, has_nectar: bool) {
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

    pub fn has_hive(&self) -> bool {
        self.hive_pos.is_some()
    }

    pub fn has_saved_flower_pos(&self) -> bool {
        self.saved_flower_pos.is_some()
    }

    pub(super) fn get_crops_grown_since_pollination(&self) -> i32 {
        self.num_crops_grown_since_pollination
    }

    pub(super) fn increment_num_crops_grown_since_pollination(&mut self) {
        self.num_crops_grown_since_pollination += 1;
    }

    pub(super) fn reset_ticks_without_nectar_since_exiting_hive(&mut self) {
        self.ticks_without_nectar_since_exiting_hive = 0;
    }

    pub(super) fn is_tired_of_looking_for_nectar(&self) -> bool {
        self.ticks_without_nectar_since_exiting_hive > 3600
    }

    /// `dropHive()`.
    pub(super) fn drop_hive(&mut self) {
        self.hive_pos = None;
        self.remaining_cooldown_before_locating_new_hive = 200;
    }

    /// `dropFlower()`.
    pub(super) fn drop_flower(&mut self) {
        self.saved_flower_pos = None;
        self.remaining_cooldown_before_locating_new_flower =
            self.base_mut().random.next_int_between_inclusive(20, 60);
    }

    /// `closerThan(targetPos, distance)`.
    pub(super) fn closer_than(&self, target: BlockPos, distance: i32) -> bool {
        target.closer_than(&self.block_position(), f64::from(distance))
    }

    /// `isTooFarAway(targetPos)`.
    pub(super) fn is_too_far_away(&self, target: BlockPos) -> bool {
        !self.closer_than(target, TOO_FAR_DISTANCE)
    }

    /// `getRollAmount(a)`.
    pub fn get_roll_amount(&self, a: f32) -> f32 {
        mth::lerp_f32(a, self.roll_amount_o, self.roll_amount)
    }

    /// `updateRollAmount()`.
    fn update_roll_amount(&mut self) {
        self.roll_amount_o = self.roll_amount;
        if self.is_rolling() {
            self.roll_amount = (self.roll_amount + 0.2).min(1.0);
        } else {
            self.roll_amount = (self.roll_amount - 0.24).max(0.0);
        }
    }

    /// `isFlying()`.
    pub fn is_flying(&self) -> bool {
        !self.base().on_ground()
    }
}

/// `itemStack.is(ItemTags.BEE_FOOD)`.
fn is_bee_food(stack: &ItemStack) -> bool {
    mc_data::tags::get::<Item>("bee_food").is_some_and(|t| t.contains(stack.item()))
}

/// `getBeehiveBlockEntity() != null`, as the position of the hive: the saved hive when it is near enough and still holds a
/// beehive block entity.
pub(super) fn valid_hive_pos(e: &dyn Entity, level: &ServerLevel) -> Option<BlockPos> {
    use mc_game_core::world::block_entity::BeehiveBlockEntity;
    let pos = bee(e).hive_pos?;
    if bee(e).is_too_far_away(pos) {
        return None;
    }
    level
        .get_block_entity(pos)
        .is_some_and(|be| be.as_any().is::<BeehiveBlockEntity>())
        .then_some(pos)
}

/// `Bee.wantsToEnterHive()`.
pub(super) fn wants_to_enter_hive(e: &dyn Entity, level: &ServerLevel) -> bool {
    use mc_game_core::world::block_entity::BeehiveBlockEntity;
    let b = bee(e);
    if b.stay_out_of_hive_countdown > 0
        || b.pollinating
        || b.has_stung()
        || mob::tick::get_target_checked(e, level).is_some()
    {
        return false;
    }
    let wants = b.has_nectar()
        || b.is_tired_of_looking_for_nectar()
        || level.attribute_bool_vec(
            mc_game_core::world::attribute::attributes::BEES_STAY_IN_HIVE,
            &e.position(),
        );
    wants
        && !valid_hive_pos(e, level).is_some_and(|pos| {
            level
                .get_block_entity(pos)
                .and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>())
                .is_some_and(|hive| hive.is_fire_nearby(level))
        })
}

/// `Bee.BeeLookControl`.
#[derive(Default)]
struct BeeLookControl {
    base: LookControlBase,
    angry: bool,
    reset_x_rot: bool,
}

impl LookControl for BeeLookControl {
    fn base(&self) -> &LookControlBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut LookControlBase {
        &mut self.base
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.angry = neutral::is_angry(&*mob, level);
        self.reset_x_rot = !bee(&*mob).pollinating;
        if !self.angry {
            look_control::default_tick(self, mob, level);
        }
    }

    fn reset_x_rot_on_tick(&self) -> bool {
        self.reset_x_rot
    }
}

/// The anonymous `FlyingPathNavigation` of `Bee.createNavigation`.
struct BeeNavigation {
    data: NavigationData,
}

impl PathNavigation for BeeNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }

    fn is_stable_destination(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        !level.get_block_state(pos.below()).is_air()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &ServerLevel) {
        if !bee(&*mob).pollinating {
            default_tick(self, mob, level);
        }
    }
}

impl BeeAccess for Bee {
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
        self.set_has_nectar(false);
        self.num_crops_grown_since_pollination = 0;
    }

    fn set_stay_out_of_hive_countdown(&mut self, ticks: i32) {
        self.stay_out_of_hive_countdown = ticks;
    }
}

fn write_pos(out: &mut CompoundTag, key: &str, pos: Option<BlockPos>) {
    if let Some(pos) = pos {
        out.put_int_array(key, vec![pos.x(), pos.y(), pos.z()]);
    }
}

impl Entity for Bee {
    mc_game_core::mob_entity_accessors!(mob);

    fn get_encode_id(&self) -> Option<&'static str> {
        Some("minecraft:bee")
    }

    fn as_bee(&self) -> Option<&dyn BeeAccess> {
        Some(self)
    }

    fn as_bee_mut(&mut self) -> Option<&mut dyn BeeAccess> {
        Some(self)
    }

    fn get_dimensions(&self, _pose: Pose) -> EntityDimensions {
        let dims = EntityDimensions::of_type(self.mob.living.entity.entity_type);
        if self.is_baby() {
            dims.scale(0.5)
        } else {
            dims
        }
    }

    fn get_walk_target_value(&self, pos: BlockPos, level: &dyn LevelReader) -> f32 {
        if level.get_block_state(pos).is_air() {
            10.0
        } else {
            0.0
        }
    }

    fn add_additional_save_data(&self, out: &mut CompoundTag) {
        mob::nbt::add_additional_save_data(self, out);
        write_pos(out, "hive_pos", self.hive_pos);
        write_pos(out, "flower_pos", self.saved_flower_pos);
        out.put_boolean("HasNectar", self.has_nectar());
        out.put_boolean("HasStung", self.has_stung());
        out.put_int(
            "TicksSincePollination",
            self.ticks_without_nectar_since_exiting_hive,
        );
        out.put_int("CannotEnterHiveTicks", self.stay_out_of_hive_countdown);
        out.put_int(
            "CropsGrownSincePollination",
            self.num_crops_grown_since_pollination,
        );
        neutral::add_persistent_anger_save_data(self, out);
    }

    fn read_additional_save_data(&mut self, input: &CompoundTag) {
        mob::nbt::read_additional_save_data(self, input);
        self.set_has_nectar(input.get_boolean_or("HasNectar", false));
        self.set_has_stung(input.get_boolean_or("HasStung", false));
        self.ticks_without_nectar_since_exiting_hive = input.get_int_or("TicksSincePollination", 0);
        self.stay_out_of_hive_countdown = input.get_int_or("CannotEnterHiveTicks", 0);
        self.num_crops_grown_since_pollination = input.get_int_or("CropsGrownSincePollination", 0);
        self.hive_pos = mc_game_core::world::entity::living::read_block_pos(input.get("hive_pos"));
        self.saved_flower_pos =
            mc_game_core::world::entity::living::read_block_pos(input.get("flower_pos"));
        neutral::read_persistent_anger_save_data(self, None, input);
    }

    fn start_persistent_anger_timer(&mut self, level: &mut ServerLevel) {
        // `TimeUtil.rangeOfSeconds(20, 39)`
        let time = self.base_mut().random.next_int_between_inclusive(400, 780);
        neutral::set_time_to_remain_angry(self, level, i64::from(time));
    }

    fn do_hurt_target(&mut self, level: &mut ServerLevel, target: &mut dyn Entity) -> bool {
        let damage_source = level.damage_sources().sting(&*self);
        let damage = self
            .mob
            .living
            .get_attribute_value(Attribute::ATTACK_DAMAGE) as i32 as f32;
        let target_id = target.id();
        let was_hurt = level.lend(self, |level| {
            target.hurt_server(level, &damage_source, damage)
        });
        if was_hurt {
            enchantment_helper::do_post_attack_effects(
                level,
                &mut Actors::of(&mut *self).and(&mut *target),
                target_id,
                &damage_source,
            );
            if target.living().is_some() {
                let stingers = target.base().entity_data.get(DATA_STINGER_COUNT_ID);
                target
                    .base_mut()
                    .entity_data
                    .set(DATA_STINGER_COUNT_ID, stingers + 1);
                let poison_time = match level.difficulty() {
                    Difficulty::Normal => 10,
                    Difficulty::Hard => 18,
                    _ => 0,
                };
                if poison_time > 0 {
                    let id = self.id();
                    target.add_effect(
                        level,
                        MobEffectInstance::of(MobEffect::POISON, poison_time * 20, 0),
                        Some(id),
                    );
                }
            }
            self.set_has_stung(true);
            neutral::stop_being_angry(self, level);
            self.play_sound(level, SoundEvent::ENTITY_BEE_STING, 1.0, 1.0);
        }
        was_hurt
    }

    fn tick(&mut self, level: &mut ServerLevel) {
        mob::tick::tick(self, level);
        if self.has_nectar()
            && self.get_crops_grown_since_pollination() < 10
            && self.base_mut().random.next_float() < 0.05
        {
            // `for (i = 0; i < random.nextInt(2) + 1; i++)` draws the bound anew every pass
            let mut i = 0;
            while i < self.base_mut().random.next_int_bounded(2) + 1 {
                // `spawnFluidParticle`: the particle is client-only but both positions draw from the level random
                level.random().next_double();
                level.random().next_double();
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
            let source = level.damage_sources().drown();
            self.hurt_server(level, &source, 1.0);
        }
        if has_stung {
            self.time_since_sting += 1;
            let bound = mth::clamp_i32(1200 - self.time_since_sting, 1, 1200);
            if self.time_since_sting % 5 == 0 && self.base_mut().random.next_int_bounded(bound) == 0
            {
                let source = level.damage_sources().generic();
                let health = self.get_health();
                self.hurt_server(level, &source, health);
            }
        }
        if !self.has_nectar() {
            self.ticks_without_nectar_since_exiting_hive += 1;
        }
        neutral::update_persistent_anger(self, level, false);
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
            && mob::tick::get_target_checked(self, level)
                .and_then(|t| level.get_entity(t))
                .is_some_and(|t| t.base().distance_to_sqr_vec(self.position()) < 4.0);
        self.set_rolling(should_roll);
        if self.base().tick_count % 20 == 0 && valid_hive_pos(self, level).is_none() {
            self.hive_pos = None;
        }
    }

    fn mob_interact(
        &mut self,
        level: &mut ServerLevel,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        hand: InteractionHand,
    ) -> InteractionResult {
        let held = player.get_item_in_hand(hand);
        if self.is_food(&held)
            && let Some(block) = held.item().block()
            && let Some(effect) =
                mc_game_core::world::level::block::behaviour(block).get_bee_interaction_effect()
        {
            self.use_player_item(level, player, hand);
            self.add_effect(level, effect, None);
            return InteractionResult::SUCCESS;
        }
        interact::mob_interact(self, level, player, hand)
    }

    fn is_food(&self, stack: &ItemStack) -> bool {
        is_bee_food(stack)
    }

    fn play_step_sound(&mut self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState) {}

    fn get_hurt_sound(&self, _source: &DamageSource) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BEE_HURT)
    }

    fn get_death_sound(&self) -> Option<SoundEvent> {
        Some(SoundEvent::ENTITY_BEE_DEATH)
    }

    fn get_sound_volume(&self) -> f32 {
        0.4
    }

    fn get_breed_offspring(
        &mut self,
        level: &mut ServerLevel,
        _partner: EntityId,
    ) -> Option<Box<dyn Entity>> {
        mc_game_core::world::entity::factory::create(
            EntityType::BEE,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Breeding,
                ignore_checks: false,
            },
        )
    }

    fn check_fall_damage(
        &mut self,
        _level: &mut ServerLevel,
        _ya: f64,
        _on_ground: bool,
        _on_state: BlockState,
        _pos: BlockPos,
    ) {
    }

    fn is_flapping(&self) -> bool {
        self.is_flying() && self.base().tick_count % TICKS_PER_FLAP == 0
    }

    fn omnidirectional_air_mover(&self) -> bool {
        true
    }

    fn hurt_server(&mut self, level: &mut ServerLevel, source: &DamageSource, amount: f32) -> bool {
        if self.is_invulnerable_to(level, source) {
            return false;
        }
        self.pollinating = false;
        damage::hurt_server(self, level, source, amount)
    }

    fn jump_in_liquid(&mut self, _fluid: FluidKind) {
        let movement = self.base().delta_movement();
        self.base_mut()
            .set_delta_movement(movement.add_xyz(0.0, 0.01, 0.0));
    }

    fn get_leash_offset(&self) -> Vec3 {
        Vec3::new(
            0.0,
            f64::from(0.5f32 * self.base().eye_height),
            f64::from(self.base().get_bb_width() * 0.2),
        )
    }
}

fn dump_pos(pos: Option<BlockPos>) -> String {
    pos.map_or_else(
        || "-".to_string(),
        |p| format!("{},{},{}", p.x(), p.y(), p.z()),
    )
}

/// The scenario dump fields of the bee (`flags hive flower hivecd flowercd nonectar stayout crops sting wet roll poll black
/// angerend angerat`).
#[distributed_slice(SCENARIO_DUMP_EXTRA)]
static BEE_DUMP: ScenarioDumpExtra = ScenarioDumpExtra {
    order: 30,
    applies: |e| e.as_any().is::<Bee>(),
    fields: |e, _level, ctx| {
        let b = bee(e);
        let f = |k: &str, v: String| (k.to_string(), v);
        vec![
            f(
                "flags",
                b.mob
                    .living
                    .entity
                    .entity_data
                    .get(DATA_FLAGS_ID)
                    .to_string(),
            ),
            f("hive", dump_pos(b.hive_pos)),
            f("flower", dump_pos(b.saved_flower_pos)),
            f(
                "hivecd",
                b.remaining_cooldown_before_locating_new_hive.to_string(),
            ),
            f(
                "flowercd",
                b.remaining_cooldown_before_locating_new_flower.to_string(),
            ),
            f(
                "nonectar",
                b.ticks_without_nectar_since_exiting_hive.to_string(),
            ),
            f("stayout", b.stay_out_of_hive_countdown.to_string()),
            f("crops", b.num_crops_grown_since_pollination.to_string()),
            f("sting", b.time_since_sting.to_string()),
            f("wet", b.under_water_ticks.to_string()),
            f(
                "roll",
                mc_core::math::java::java_float_to_string(b.roll_amount),
            ),
            f("poll", u8::from(b.pollinating).to_string()),
            f(
                "black",
                if b.blacklisted_hives.is_empty() {
                    "-".to_string()
                } else {
                    b.blacklisted_hives
                        .iter()
                        .map(|p| dump_pos(Some(*p)))
                        .collect::<Vec<_>>()
                        .join(";")
                },
            ),
            f(
                "angerend",
                neutral::get_persistent_anger_end_time(e).to_string(),
            ),
            f(
                "angerat",
                ctx.name_uuid(neutral::get_persistent_anger_target(e)),
            ),
        ]
    },
};

#[distributed_slice(ENTITY_FACTORIES)]
static BEE: EntityRegistration = EntityRegistration {
    entity_type: EntityType::BEE,
    create: |ty, id, registries| Box::new(Bee::new(ty, id, registries)),
};
