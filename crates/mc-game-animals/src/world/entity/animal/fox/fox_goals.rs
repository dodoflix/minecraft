//! The goals nested in `Fox` (`FoxFloatGoal`, `FaceplantGoal`, `FoxPanicGoal`, `FoxBreedGoal`, `StalkPreyGoal`, `FoxPounceGoal`,
//! `SeekShelterGoal`, `FoxMeleeAttackGoal`, `SleepGoal`, `FoxFollowParentGoal`, `FoxStrollThroughVillageGoal`, `FoxEatBerriesGoal`,
//! `FoxSearchForItemsGoal`, `FoxLookAtPlayerGoal`, `PerchAndSearchGoal`, `DefendTrustedTargetGoal`). Java subclasses of a generic goal
//! embed it and call its `super` bodies; the struct names are the Java simple class names (the oracle prints them in `goals=`).

use super::fox::Fox;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, GameEvent, Item, SoundEvent, props};
use mc_game_ai::world::entity::ai::goal::{FleeSunGoal, JUMP_GOAL_FLAGS, StrollThroughVillageGoal};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::move_to_block::{self, MoveToBlock, MoveToBlockData};
use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::world::entity::ai::goal::{
    BreedGoal, Flag, FloatGoal, FollowParentGoal, Goal, GoalFlags, LookAtPlayerGoal,
    MeleeAttackGoal, PanicGoal, reduced_tick_delay,
};
use mc_game_core::world::entity::ai::targeting::TargetingConditions;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::mob::{
    self, MobKind, ageable, animal, combat, ext, navigation, tamable, tick,
};
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::level_access::{BlockGetter, LevelAccessor as _};
use std::sync::Arc;

fn fox(mob: &dyn Entity) -> &Fox {
    mob.as_any().downcast_ref::<Fox>().expect("fox")
}

fn fox_mut(mob: &mut dyn Entity) -> &mut Fox {
    mob.as_any_mut().downcast_mut::<Fox>().expect("fox")
}

/// `getTarget()` resolved to a live entity of the level.
fn target_of(mob: &dyn Entity, level: &ServerLevel) -> Option<EntityId> {
    tick::get_target_checked(mob, level)
}

/// `Fox.isPathClear(fox, target)` (both positions).
pub fn is_path_clear(level: &ServerLevel, fox_pos: Vec3, target_pos: Vec3) -> bool {
    let zdiff = target_pos.z - fox_pos.z;
    let xdiff = target_pos.x - fox_pos.x;
    let slope = zdiff / xdiff;
    for i in 0..6 {
        let frac = f64::from(i as f32 / 6.0f32);
        let z = if slope == 0.0 { 0.0 } else { zdiff * frac };
        let x = if slope == 0.0 {
            xdiff * frac
        } else {
            z / slope
        };
        for j in 1..4 {
            if !level
                .get_block_state(BlockPos::containing(
                    fox_pos.x + x,
                    fox_pos.y + f64::from(j),
                    fox_pos.z + z,
                ))
                .can_be_replaced()
            {
                return false;
            }
        }
    }
    true
}

/// `Fox.FoxAlertableEntitiesSelector.test(target, level)` (`Fox.this` = `fox`).
fn alertable_entities_selector(
    fox: &dyn Entity,
    target: &dyn Entity,
    _level: &ServerLevel,
) -> bool {
    if target.as_any().is::<Fox>() {
        false
    } else if matches!(target.get_type(), EntityType::CHICKEN | EntityType::RABBIT)
        || target.mob().is_some_and(|m| m.kind == MobKind::Monster)
    {
        true
    } else if target.mob().is_some_and(|m| m.tamable.is_some()) {
        !tamable::is_tame(target)
    } else if (target.is_player() && (target.is_spectator() || ext::is_creative_player(target)))
        || fox
            .as_any()
            .downcast_ref::<Fox>()
            .is_some_and(|f| f.trusts(target))
    {
        false
    } else {
        !target.is_sleeping() && !target.is_discrete()
    }
}

/// `Fox.FoxBehaviorGoal` (abstract): the shared `alertableTargeting`, `hasShelter()` and `alertable()`.
struct FoxBehavior {
    alertable_targeting: TargetingConditions,
}

impl FoxBehavior {
    fn new() -> Self {
        Self {
            alertable_targeting: TargetingConditions::for_combat()
                .range(12.0)
                .ignore_line_of_sight()
                .source_selector(Some(Arc::new(alertable_entities_selector))),
        }
    }

    /// `hasShelter()`.
    fn has_shelter(mob: &dyn Entity, level: &ServerLevel) -> bool {
        let b = mob.base();
        let fox_pos = BlockPos::containing(b.x(), b.bounding_box().max_y, b.z());
        !level.can_see_sky(fox_pos) && mob.get_walk_target_value(fox_pos, level) >= 0.0
    }

    /// `alertable()`.
    fn alertable(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        let bb = mob.base().bounding_box().inflate(12.0, 6.0, 12.0);
        !ext::get_nearby_entities(level, |_| true, &self.alertable_targeting, mob, &bb).is_empty()
    }
}

/// `Fox.DefendTrustedTargetGoal`.
pub struct DefendTrustedTargetGoal {
    inner: NearestAttackableTargetGoal,
    trusted_last_hurt_by: Option<EntityId>,
    trusted_last_hurt: Option<EntityId>,
    timestamp: i32,
}

impl DefendTrustedTargetGoal {
    pub fn new() -> Self {
        // `(target, level) -> TRUSTED_TARGET_SELECTOR.test(target) && !this.trusts(target)`
        let selector: mc_game_core::world::entity::ai::targeting::TargetSelector =
            Arc::new(|target: &dyn Entity, _level: &ServerLevel| {
                target.living().is_some_and(|l| {
                    l.last_hurt_mob.is_some()
                        && l.last_hurt_mob_timestamp < target.base().tick_count.wrapping_add(600)
                })
            });
        let inner = NearestAttackableTargetGoal::build(
            TargetType::Class(Arc::new(|e: &dyn Entity| e.living().is_some())),
            10,
            false,
            false,
            Some(selector),
        )
        .with_source_selector(Arc::new(
            |fox: &dyn Entity, target: &dyn Entity, _level: &ServerLevel| {
                !fox.as_any()
                    .downcast_ref::<Fox>()
                    .is_some_and(|f| f.trusts(target))
            },
        ));
        Self {
            inner,
            trusted_last_hurt_by: None,
            trusted_last_hurt: None,
            timestamp: 0,
        }
    }
}

impl Default for DefendTrustedTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for DefendTrustedTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.inner.random_interval > 0
            && mob
                .base_mut()
                .random
                .next_int_bounded(self.inner.random_interval)
                != 0
        {
            return false;
        }
        for uuid in fox(mob).get_trusted_entities() {
            let Some(trusted) = level
                .get_entity_by_uuid(uuid)
                .filter(|e| e.living().is_some())
            else {
                continue;
            };
            let trusted_id = trusted.id();
            self.trusted_last_hurt = Some(trusted_id);
            self.trusted_last_hurt_by = combat::get_last_hurt_by_mob(trusted, level);
            let timestamp = combat::get_last_hurt_by_mob_timestamp(trusted);
            let conditions = self.inner.target_conditions.clone();
            return timestamp != self.timestamp
                && self.inner.target_goal.can_attack(
                    mob,
                    level,
                    self.trusted_last_hurt_by,
                    &conditions,
                );
        }
        false
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.target = self.trusted_last_hurt_by;
        if let Some(trusted) = self.trusted_last_hurt.and_then(|t| level.get_entity(t)) {
            self.timestamp = combat::get_last_hurt_by_mob_timestamp(trusted);
        }
        mob.play_sound(level, SoundEvent::ENTITY_FOX_AGGRO, 1.0, 1.0);
        let fox = fox_mut(mob);
        fox.set_defending(true);
        fox.wake_up();
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FaceplantGoal`.
pub struct FaceplantGoal {
    countdown: i32,
}

impl FaceplantGoal {
    pub fn new() -> Self {
        Self { countdown: 0 }
    }
}

impl Default for FaceplantGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for FaceplantGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        fox(mob).is_faceplanted()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use(mob, level) && self.countdown > 0
    }

    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.countdown = self.adjusted_tick_delay(40);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        fox_mut(mob).set_faceplanted(false);
    }

    fn tick(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.countdown -= 1;
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Look, Flag::Jump, Flag::Move])
    }
}

/// `Fox.FoxBreedGoal`.
pub struct FoxBreedGoal {
    inner: BreedGoal,
}

impl FoxBreedGoal {
    pub fn new(fox: &dyn Entity, speed_modifier: f64) -> Self {
        Self {
            inner: BreedGoal::new(fox, speed_modifier),
        }
    }
}

/// `FoxBreedGoal.breed()`.
fn fox_breed(animal: &mut dyn Entity, level: &mut ServerLevel, partner: EntityId) {
    let Some(mut offspring) = animal.get_breed_offspring(level, partner) else {
        return;
    };
    let animal_love_cause = animal::get_love_cause(animal, level)
        .and_then(|p| level.get_entity(p))
        .map(|p| p.uuid());
    let partner_love_cause = level
        .get_entity(partner)
        .and_then(|p| animal::get_love_cause(p, level))
        .and_then(|p| level.get_entity(p))
        .map(|p| p.uuid());
    if let Some(cause) = animal_love_cause {
        fox_mut(&mut *offspring).add_trusted_entity(cause);
    }
    if let Some(cause) = partner_love_cause
        && animal_love_cause != Some(cause)
    {
        fox_mut(&mut *offspring).add_trusted_entity(cause);
    }
    ageable::set_baby_in_level(&mut *offspring, level, true);
    let p = animal.position();
    offspring.snap_to(p.x, p.y, p.z, 0.0, 0.0);
    animal::finalize_spawn_child_from_breeding(animal, level, partner, Some(&mut *offspring));
    level.add_fresh_entity_with_passengers(vec![offspring]);
}

impl Goal for FoxBreedGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        fox_mut(mob).clear_states();
        if let Some(partner) = self.inner.partner {
            level.with_entity(partner, |_, p| {
                if let Some(f) = p.as_any_mut().downcast_mut::<Fox>() {
                    f.clear_states();
                }
            });
        }
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick_with(mob, level, &mut fox_breed);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FoxFloatGoal`.
pub struct FoxFloatGoal {
    inner: FloatGoal,
}

impl FoxFloatGoal {
    pub fn new(fox: &mut dyn Entity) -> Self {
        Self {
            inner: FloatGoal::new(fox),
        }
    }
}

impl Goal for FoxFloatGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.inner.can_use_with_threshold(mob, 0.25)
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        fox_mut(mob).clear_states();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FoxPanicGoal`.
pub struct FoxPanicGoal {
    inner: PanicGoal,
}

impl FoxPanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: PanicGoal::new(speed_modifier),
        }
    }
}

impl Goal for FoxPanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let should_panic = !fox(mob).is_defending() && self.inner.should_panic(mob, level);
        self.inner.can_use_with(mob, level, should_panic, 5)
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

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.inner)
    }
}

/// `Fox.FoxEatBerriesGoal` (`MoveToBlockGoal`).
pub struct FoxEatBerriesGoal {
    data: MoveToBlockData,
    ticks_waited: i32,
}

impl FoxEatBerriesGoal {
    /// `FoxEatBerriesGoal(speedModifier, searchRange, verticalSearchRange)`.
    pub fn new(speed_modifier: f32, search_range: i32, vertical_search_range: i32) -> Self {
        Self {
            data: MoveToBlockData::with_vertical(
                f64::from(speed_modifier),
                search_range,
                vertical_search_range,
            ),
            ticks_waited: 0,
        }
    }

    /// `onReachedTarget()`.
    fn on_reached_target(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if level.mob_griefing() {
            let state = level.get_block_state(self.data.block_pos);
            if state.block() == Block::SWEET_BERRY_BUSH {
                self.pick_sweet_berries(mob, level, state);
            } else if has_glow_berries(state) {
                mc_game_core::world::level::block::impls::cave_vines::use_(
                    mob,
                    state,
                    level,
                    self.data.block_pos,
                );
            }
        }
    }

    /// `pickSweetBerries(state)`.
    fn pick_sweet_berries(
        &mut self,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
        state: BlockState,
    ) {
        let block_pos = self.data.block_pos;
        let age = state.get(props::AGE_3);
        let mut count = 1 + level.random().next_int_bounded(2) + i32::from(age == 3);
        if mob
            .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::MainHand)
            .is_empty()
        {
            mob.set_item_slot(
                level,
                mc_game_core::world::entity::equipment::EquipmentSlot::MainHand,
                mc_game_core::world::item::ItemStack::new(Item::SWEET_BERRIES),
            );
            count -= 1;
        }
        if count > 0 {
            mc_game_core::world::level::block::helpers::pop_resource(
                level,
                block_pos,
                mc_game_core::world::item::ItemStack::with_count(Item::SWEET_BERRIES, count),
            );
        }
        mob.play_sound(
            level,
            SoundEvent::BLOCK_SWEET_BERRY_BUSH_PICK_BERRIES,
            1.0,
            1.0,
        );
        let new_state = state.set(props::AGE_3, 1);
        level.set_block_flags(block_pos, new_state, 2);
        level.game_event_from_entity_ref_in_state(
            &*mob,
            None,
            GameEvent::BLOCK_CHANGE,
            mc_core::phys::Vec3::at_center_of(&block_pos),
        );
    }
}

/// `CaveVines.hasGlowBerries(state)`.
fn has_glow_berries(state: BlockState) -> bool {
    state.try_get(props::BERRIES).unwrap_or(false)
}

impl MoveToBlock for FoxEatBerriesGoal {
    fn mtb(&self) -> &MoveToBlockData {
        &self.data
    }

    fn mtb_mut(&mut self) -> &mut MoveToBlockData {
        &mut self.data
    }

    fn accepted_distance(&self) -> f64 {
        2.0
    }

    fn should_recalculate_path(&self) -> bool {
        self.data.try_ticks % 100 == 0
    }

    fn is_valid_target(&self, _mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        state.block() == Block::SWEET_BERRY_BUSH && state.get(props::AGE_3) >= 2
            || has_glow_berries(state)
    }
}

impl Goal for FoxEatBerriesGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !mob.is_sleeping() && move_to_block::can_use(self, mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        move_to_block::can_continue_to_use(self, mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.ticks_waited = 0;
        fox_mut(mob).set_sitting(false);
        move_to_block::start(self, mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if self.is_reached_target() {
            if self.ticks_waited >= 40 {
                self.on_reached_target(mob, level);
            } else {
                self.ticks_waited += 1;
            }
        } else if !self.is_reached_target() && mob.base_mut().random.next_float() < 0.05 {
            mob.play_sound(level, SoundEvent::ENTITY_FOX_SNIFF, 1.0, 1.0);
        }
        move_to_block::tick(self, mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Jump])
    }
}

/// `Fox.FoxFollowParentGoal`.
pub struct FoxFollowParentGoal {
    inner: FollowParentGoal,
}

impl FoxFollowParentGoal {
    pub fn new(fox: &dyn Entity, speed_modifier: f64) -> Self {
        Self {
            inner: FollowParentGoal::new(fox, speed_modifier),
        }
    }
}

impl Goal for FoxFollowParentGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !fox(mob).is_defending() && self.inner.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !fox(mob).is_defending() && self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        fox_mut(mob).clear_states();
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FoxLookAtPlayerGoal`.
pub struct FoxLookAtPlayerGoal {
    inner: LookAtPlayerGoal,
}

impl FoxLookAtPlayerGoal {
    pub fn new(fox: &dyn Entity, look_distance: f32) -> Self {
        Self {
            inner: LookAtPlayerGoal::new_player(fox, look_distance),
        }
    }
}

impl Goal for FoxLookAtPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level) && !fox(mob).is_faceplanted() && !fox(mob).is_interested()
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
            && !fox(mob).is_faceplanted()
            && !fox(mob).is_interested()
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FoxMeleeAttackGoal`.
pub struct FoxMeleeAttackGoal {
    inner: MeleeAttackGoal,
}

impl FoxMeleeAttackGoal {
    pub fn new(speed_modifier: f64, track_target: bool) -> Self {
        Self {
            inner: MeleeAttackGoal::new(speed_modifier, track_target),
        }
    }
}

impl Goal for FoxMeleeAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let f = fox(mob);
        !f.is_sitting()
            && !mob.is_sleeping()
            && !mob.is_crouching()
            && !f.is_faceplanted()
            && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        fox_mut(mob).set_is_interested(false);
        self.inner.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop_impl(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        // `checkAndPerformAttack(target)`: no swing animation, but the bite sound
        self.inner
            .tick_with(mob, level, &mut |goal, mob, level, target| {
                if goal.can_perform_attack(mob, level, target) {
                    goal.reset_attack_cooldown();
                    combat::do_hurt_target(mob, level, target);
                    mob.play_sound(level, SoundEvent::ENTITY_FOX_BITE, 1.0, 1.0);
                }
            });
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.FoxPounceGoal` (`JumpGoal`).
pub struct FoxPounceGoal;

impl Goal for FoxPounceGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !fox(mob).is_fully_crouched() {
            return false;
        }
        let Some(target) = target_of(mob, level)
            .and_then(|t| level.get_entity(t))
            .filter(|t| t.is_alive())
        else {
            return false;
        };
        // `target.getMotionDirection() != target.getDirection()` only differs for boats/minecarts, never living targets
        let target_pos = target.position();
        let target_block = target.block_position();
        let has_clear_path = is_path_clear(level, mob.position(), target_pos);
        if !has_clear_path {
            navigation::create_path_entity(mob, level, target_block, 0);
            let f = fox_mut(mob);
            f.set_is_crouching(false);
            f.set_is_interested(false);
        }
        has_clear_path
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if target_of(mob, level)
            .and_then(|t| level.get_entity(t))
            .is_none_or(|t| !t.is_alive())
        {
            return false;
        }
        let yd = mob.base().delta_movement().y;
        (!(yd * yd < f64::from(0.05f32))
            || !(mob.base().x_rot().abs() < 15.0)
            || !mob.base().on_ground())
            && !fox(mob).is_faceplanted()
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.set_jumping(level, true);
        let f = fox_mut(mob);
        f.set_is_pouncing(true);
        f.set_is_interested(false);
        if let Some(target_id) = target_of(mob, level)
            && let Some(target) = level.get_entity(target_id)
        {
            mob::look_at_entity_full(mob, target, 60.0, 30.0);
            let p = mob.position();
            let uv = Vec3::new(
                target.base().x() - p.x,
                target.base().y() - p.y,
                target.base().z() - p.z,
            )
            .normalize();
            let movement = mob.base().delta_movement();
            mob.base_mut()
                .set_delta_movement(movement.add_xyz(uv.x * 0.8, 0.9, uv.z * 0.8));
        }
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let f = fox_mut(mob);
        f.set_is_crouching(false);
        f.crouch_amount = 0.0;
        f.crouch_amount_o = 0.0;
        f.set_is_interested(false);
        f.set_is_pouncing(false);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target_id = target_of(mob, level);
        if let Some(target) = target_id.and_then(|t| level.get_entity(t)) {
            mob::look_at_entity_full(mob, target, 60.0, 30.0);
        }
        if !fox(mob).is_faceplanted() {
            let movement = mob.base().delta_movement();
            if movement.y * movement.y < f64::from(0.03f32) && mob.base().x_rot() != 0.0 {
                let rot = mth::rot_lerp_f32(0.2, mob.base().x_rot(), 0.0);
                mob.set_x_rot(rot);
            } else {
                let direction = movement.horizontal_distance();
                let upwards_bias = if mob.living().is_some_and(|l| l.jumping) && movement.y > 0.0 {
                    6.5f32
                } else {
                    1.0f32
                };
                let biased_y = movement.y * f64::from(upwards_bias);
                let len = (direction * direction + biased_y * biased_y).sqrt();
                if len > f64::from(1.0E-5f32) {
                    let signum = if (-biased_y).is_nan() {
                        f64::NAN
                    } else if -biased_y > 0.0 {
                        1.0
                    } else if -biased_y < 0.0 {
                        -1.0
                    } else {
                        -biased_y
                    };
                    let rotation = signum
                        * (direction / len).acos()
                        * f64::from(mc_core::math::mth::RAD_TO_DEG);
                    mob.set_x_rot(rotation as f32);
                }
            }
        }
        let near = target_id
            .and_then(|t| level.get_entity(t))
            .is_some_and(|t| ext::distance_to(mob, t) <= 2.0);
        if near {
            if let Some(t) = target_id {
                combat::do_hurt_target(mob, level, t);
            }
        } else if mob.base().x_rot() > 0.0
            && mob.base().on_ground()
            && mob.base().delta_movement().y as f32 != 0.0
            && level.get_block_state(mob.block_position()).block() == Block::SNOW
        {
            mob.set_x_rot(60.0);
            mob.set_target(level, None);
            fox_mut(mob).set_faceplanted(true);
        }
    }

    fn flags(&self) -> GoalFlags {
        JUMP_GOAL_FLAGS
    }
}

/// `Fox.FoxSearchForItemsGoal`.
pub struct FoxSearchForItemsGoal;

/// `getEntitiesOfClass(ItemEntity.class, fox.getBoundingBox().inflate(8.0, 8.0, 8.0), ALLOWED_ITEMS)`.
fn allowed_items_nearby(mob: &dyn Entity, level: &ServerLevel) -> Vec<EntityId> {
    let bb = mob.base().bounding_box().inflate(8.0, 8.0, 8.0);
    level.get_entities_of_class(
        &bb,
        |e| e.as_any().is::<ItemEntity>(),
        |e| {
            e.as_any()
                .downcast_ref::<ItemEntity>()
                .is_some_and(|i| !i.has_pick_up_delay() && i.is_alive())
        },
    )
}

fn move_to_first_item(mob: &mut dyn Entity, level: &mut ServerLevel, items: &[EntityId]) {
    if let Some(pos) = items
        .first()
        .and_then(|i| level.get_entity(*i))
        .map(|i| i.block_position())
    {
        navigation::move_to_entity(mob, level, pos, f64::from(1.2f32));
    }
}

impl Goal for FoxSearchForItemsGoal {
    #[expect(
        clippy::if_same_then_else,
        reason = "mirrors FoxSearchForItemsGoal.canUse branch structure (the random draw must stay ordered)"
    )]
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let mainhand = mc_game_core::world::entity::equipment::EquipmentSlot::MainHand;
        if !mob.get_item_by_slot(mainhand).is_empty() {
            false
        } else if target_of(mob, level).is_some()
            || combat::get_last_hurt_by_mob(mob, level).is_some()
        {
            false
        } else if !fox(mob).can_move() {
            false
        } else if mob
            .base_mut()
            .random
            .next_int_bounded(reduced_tick_delay(10))
            != 0
        {
            false
        } else {
            !allowed_items_nearby(mob, level).is_empty()
                && mob.get_item_by_slot(mainhand).is_empty()
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let items = allowed_items_nearby(mob, level);
        move_to_first_item(mob, level, &items);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let items = allowed_items_nearby(mob, level);
        if mob
            .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::MainHand)
            .is_empty()
        {
            move_to_first_item(mob, level, &items);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Fox.FoxStrollThroughVillageGoal`.
pub struct FoxStrollThroughVillageGoal {
    inner: StrollThroughVillageGoal,
}

impl FoxStrollThroughVillageGoal {
    /// `FoxStrollThroughVillageGoal(searchRadius, interval)`.
    pub fn new(_search_radius: i32, interval: i32) -> Self {
        Self {
            inner: StrollThroughVillageGoal::new(interval),
        }
    }

    /// `canFoxMove()`.
    fn can_fox_move(mob: &dyn Entity, level: &ServerLevel) -> bool {
        !mob.is_sleeping()
            && !fox(mob).is_sitting()
            && !fox(mob).is_defending()
            && target_of(mob, level).is_none()
    }
}

impl Goal for FoxStrollThroughVillageGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_use(mob, level) && Self::can_fox_move(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level) && Self::can_fox_move(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        fox_mut(mob).clear_states();
        self.inner.start(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.PerchAndSearchGoal`.
pub struct PerchAndSearchGoal {
    behavior: FoxBehavior,
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
    looks_remaining: i32,
}

impl PerchAndSearchGoal {
    pub fn new() -> Self {
        Self {
            behavior: FoxBehavior::new(),
            rel_x: 0.0,
            rel_z: 0.0,
            look_time: 0,
            looks_remaining: 0,
        }
    }

    /// `resetLook()`.
    fn reset_look(&mut self, mob: &mut dyn Entity) {
        let rnd = (std::f64::consts::PI * 2.0) * mob.base_mut().random.next_double();
        self.rel_x = rnd.cos();
        self.rel_z = rnd.sin();
        let roll = 80 + mob.base_mut().random.next_int_bounded(20);
        self.look_time = self.adjusted_tick_delay(roll);
    }
}

impl Default for PerchAndSearchGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for PerchAndSearchGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        combat::get_last_hurt_by_mob(mob, level).is_none()
            && mob.base_mut().random.next_float() < 0.02
            && !mob.is_sleeping()
            && target_of(mob, level).is_none()
            && navigation::is_done(mob, level)
            && !self.behavior.alertable(mob, level)
            && !fox(mob).is_pouncing()
            && !mob.is_crouching()
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.looks_remaining > 0
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.reset_look(mob);
        self.looks_remaining = 2 + mob.base_mut().random.next_int_bounded(3);
        fox_mut(mob).set_sitting(true);
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        fox_mut(mob).set_sitting(false);
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.look_time -= 1;
        if self.look_time <= 0 {
            self.looks_remaining -= 1;
            self.reset_look(mob);
        }
        let p = mob.position();
        let eye_y = mob.base().get_eye_y();
        let (max_y, max_x) = (
            mob.get_max_head_y_rot() as f32,
            mob.get_max_head_x_rot() as f32,
        );
        mob.mob_mut()
            .expect("mob")
            .look_control_mut()
            .set_look_at_full(p.x + self.rel_x, eye_y, p.z + self.rel_z, max_y, max_x);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}

/// `Fox.SeekShelterGoal` (`FleeSunGoal`).
pub struct SeekShelterGoal {
    inner: FleeSunGoal,
    interval: i32,
}

impl SeekShelterGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            inner: FleeSunGoal::new(speed_modifier),
            interval: reduced_tick_delay(100),
        }
    }
}

impl Goal for SeekShelterGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob.is_sleeping() && target_of(mob, level).is_none() {
            if level.is_thundering() && level.can_see_sky(mob.block_position()) {
                self.inner.set_wanted_pos(mob, level)
            } else if self.interval > 0 {
                self.interval -= 1;
                false
            } else {
                self.interval = 100;
                let pos = mob.block_position();
                level.is_bright_outside()
                    && level.can_see_sky(pos)
                    && !level.is_village(pos)
                    && self.inner.set_wanted_pos(mob, level)
            }
        } else {
            false
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        fox_mut(mob).clear_states();
        self.inner.start(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}

/// `Fox.SleepGoal`'s `WAIT_TIME_BEFORE_SLEEP` = `reducedTickDelay(140)`.
pub const WAIT_TIME_BEFORE_SLEEP: i32 = 70;

/// `Fox.SleepGoal`.
pub struct SleepGoal {
    behavior: FoxBehavior,
    countdown: i32,
}

impl SleepGoal {
    /// `SleepGoal()`: `countdown = fox.random.nextInt(WAIT_TIME_BEFORE_SLEEP)`.
    pub fn new(fox: &mut dyn Entity) -> Self {
        Self {
            behavior: FoxBehavior::new(),
            countdown: fox
                .base_mut()
                .random
                .next_int_bounded(WAIT_TIME_BEFORE_SLEEP),
        }
    }

    /// `canSleep()`.
    fn can_sleep(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            false
        } else {
            level.is_bright_outside()
                && FoxBehavior::has_shelter(mob, level)
                && !self.behavior.alertable(mob, level)
                && !mob.base().is_in_powder_snow
        }
    }
}

impl Goal for SleepGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let moving = mob
            .living()
            .is_some_and(|l| l.xxa != 0.0 || l.yya != 0.0 || l.zza != 0.0);
        if moving {
            false
        } else {
            self.can_sleep(mob, level) || mob.is_sleeping()
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_sleep(mob, level)
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.countdown = mob
            .base_mut()
            .random
            .next_int_bounded(WAIT_TIME_BEFORE_SLEEP);
        fox_mut(mob).clear_states();
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let f = fox_mut(mob);
        f.set_sitting(false);
        f.set_is_crouching(false);
        f.set_is_interested(false);
        mob.set_jumping(level, false);
        fox_mut(mob).set_sleeping(true);
        navigation::stop(mob, level);
        let p = mob.position();
        mob::set_wanted_position(mob, p.x, p.y, p.z, 0.0);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look, Flag::Jump])
    }
}

/// `Fox.StalkPreyGoal`.
pub struct StalkPreyGoal;

impl Goal for StalkPreyGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.is_sleeping() {
            return false;
        }
        let Some(target) = target_of(mob, level).and_then(|t| level.get_entity(t)) else {
            return false;
        };
        target.is_alive()
            && matches!(target.get_type(), EntityType::CHICKEN | EntityType::RABBIT)
            && mob.base().distance_to_sqr_vec(target.position()) > 36.0
            && !mob.is_crouching()
            && !fox(mob).is_interested()
            && !mob.living().is_some_and(|l| l.jumping)
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let f = fox_mut(mob);
        f.set_sitting(false);
        f.set_faceplanted(false);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let target = target_of(mob, level);
        let clear = target
            .and_then(|t| level.get_entity(t))
            .map(|t| is_path_clear(level, mob.position(), t.position()));
        if clear == Some(true) {
            let f = fox_mut(mob);
            f.set_is_interested(true);
            f.set_is_crouching(true);
            navigation::stop(mob, level);
            let (max_y, max_x) = (
                mob.get_max_head_y_rot() as f32,
                mob.get_max_head_x_rot() as f32,
            );
            if let Some(t) = target.and_then(|t| level.get_entity(t)) {
                mob::look_at_entity_full(mob, t, max_y, max_x);
            }
        } else {
            let f = fox_mut(mob);
            f.set_is_interested(false);
            f.set_is_crouching(false);
        }
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = target_of(mob, level) else {
            return;
        };
        let (max_y, max_x) = (
            mob.get_max_head_y_rot() as f32,
            mob.get_max_head_x_rot() as f32,
        );
        let Some(target) = level.get_entity(target_id) else {
            return;
        };
        mob::look_at_entity_full(mob, target, max_y, max_x);
        if mob.base().distance_to_sqr_vec(target.position()) <= 36.0 {
            let f = fox_mut(mob);
            f.set_is_interested(true);
            f.set_is_crouching(true);
            navigation::stop(mob, level);
        } else {
            let pos = target.block_position();
            navigation::move_to_entity(mob, level, pos, 1.5);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
