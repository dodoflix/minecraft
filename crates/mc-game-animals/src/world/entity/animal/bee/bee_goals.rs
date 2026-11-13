//! The goals of `Bee` (its inner classes `BeeAttackGoal` .. `ValidateHiveGoal`). The state that vanilla reads across goals
//! (`beePollinateGoal.isPollinating()`, the hive blacklist) lives in the [`Bee`](super::Bee) struct.

use super::bee::{attracts_bees, bee, bee_mut, valid_hive_pos, wants_to_enter_hive};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::{SoundEvent, props};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::BeehiveBlockEntity;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::target::{
    HurtByTargetGoal, NearestAttackableTargetGoal, TargetType,
};
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, MeleeAttackGoal};
use mc_game_core::world::entity::ai::util::{
    air_and_water_random_pos, air_random_pos, hover_random_pos,
};
use mc_game_core::world::entity::ai::village::poi::{Occupancy, PoiTypeHolder};
use mc_game_core::world::entity::mob::{self, ext, navigation, neutral};
use mc_game_core::world::level::block::BlockStateExt as _;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::helpers::is_instance_of;
use mc_game_core::world::level::block::impls::crop_block;
use mc_game_core::world::level::pathfinder::Path;
use mc_game_core::world::level::{BlockGetter as _, LevelAccessor as _, LevelReader as _};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// `Bee.BaseBeeGoal.canUse()` / `canContinueToUse()`: the goal's own test and not angry.
fn not_angry(mob: &dyn Entity, level: &ServerLevel) -> bool {
    !neutral::is_angry(mob, level)
}

/// `getBlockEntity(pos) instanceof BeehiveBlockEntity hive`.
fn hive_at(level: &ServerLevel, pos: BlockPos) -> Option<&BeehiveBlockEntity> {
    level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>())
}

/// `Bee.pathfindRandomlyTowards(targetPos)`.
fn pathfind_randomly_towards(mob: &mut dyn Entity, level: &mut ServerLevel, target_pos: BlockPos) {
    let target_vec = Vec3::at_bottom_center_of(&target_pos);
    let mut y_adjust = 0;
    let bee_pos = mob.block_position();
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
    let next = air_random_pos::get_pos_towards(
        mob,
        level,
        xz_dist,
        y_dist,
        y_adjust,
        target_vec,
        f64::from((PI / 10.0) as f32),
    );
    if let Some(next) = next {
        navigation::with_navigation(mob, level, |nav, m, l| {
            nav.set_max_visited_nodes_multiplier(0.5);
            nav.move_to_xyz(m, &*l, next.x, next.y, next.z, 1.0);
        });
    }
}

/// `navigation.getPath()`.
fn current_path(mob: &dyn Entity, level: &ServerLevel) -> Option<Path> {
    navigation::with_navigation_ref(mob, level, |n| n.get_path()).flatten()
}

/// `Bee.BeeAttackGoal`.
pub(super) struct BeeAttackGoal {
    melee: MeleeAttackGoal,
}

impl BeeAttackGoal {
    pub(super) fn new() -> Self {
        Self {
            melee: MeleeAttackGoal::new(f64::from(1.4f32), true),
        }
    }
}

impl Goal for BeeAttackGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeAttackGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_use_impl(mob, level)
            && neutral::is_angry(mob, level)
            && !bee(mob).has_stung()
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_continue_to_use_impl(mob, level)
            && neutral::is_angry(mob, level)
            && !bee(mob).has_stung()
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.start_impl(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.stop_impl(mob, level);
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.tick_impl(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.melee.flags()
    }
}

/// `Bee.BeeBecomeAngryTargetGoal`.
pub(super) struct BeeBecomeAngryTargetGoal {
    nearest: NearestAttackableTargetGoal,
}

impl BeeBecomeAngryTargetGoal {
    pub(super) fn new() -> Self {
        Self {
            nearest: NearestAttackableTargetGoal::build(TargetType::Player, 10, true, false, None)
                .with_source_selector(Arc::new(|mob, target, level| {
                    neutral::is_angry_at(mob, level, target)
                })),
        }
    }

    fn bee_can_target(mob: &dyn Entity, level: &ServerLevel) -> bool {
        neutral::is_angry(mob, level) && !bee(mob).has_stung()
    }
}

impl Goal for BeeBecomeAngryTargetGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeBecomeAngryTargetGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::bee_can_target(mob, level) && self.nearest.can_use_impl(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if Self::bee_can_target(mob, level) && mob::tick::get_target_checked(mob, level).is_some() {
            self.nearest.can_continue_to_use(mob, level)
        } else {
            self.nearest.target_goal.target_mob = None;
            false
        }
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.nearest.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.nearest.flags()
    }
}

/// `Bee.BeeEnterHiveGoal`.
pub(super) struct BeeEnterHiveGoal;

impl Goal for BeeEnterHiveGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeEnterHiveGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        // `canBeeUse()`
        if let Some(hive_pos) = bee(mob).hive_pos
            && wants_to_enter_hive(mob, level)
            && hive_pos.closer_to_center_than(&mob.position(), 2.0)
            && let Some(pos) = valid_hive_pos(mob, level)
        {
            if hive_at(level, pos).is_some_and(|hive| !hive.is_full()) {
                return not_angry(mob, level);
            }
            bee_mut(mob).hive_pos = None;
        }
        false
    }
    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(pos) = valid_hive_pos(mob, level) {
            level.with_typed_block_entity::<BeehiveBlockEntity, _>(pos, |level, hive| {
                hive.add_occupant(level, mob);
            });
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Bee.BeeGoToHiveGoal`.
pub(super) struct BeeGoToHiveGoal {
    travelling_ticks: i32,
    last_path: Option<Path>,
    ticks_stuck: i32,
}

impl BeeGoToHiveGoal {
    pub(super) fn new() -> Self {
        Self {
            travelling_ticks: 0,
            last_path: None,
            ticks_stuck: 0,
        }
    }

    fn can_bee_use(mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(hive_pos) = bee(mob).hive_pos else {
            return false;
        };
        !bee(mob).is_too_far_away(hive_pos)
            && !mob.mob().is_some_and(|m| m.has_home())
            && wants_to_enter_hive(mob, level)
            && !Self::has_reached_target(mob, level, hive_pos)
            && level.get_block_state(hive_pos).is_tag("beehives")
    }

    fn has_reached_target(mob: &dyn Entity, level: &ServerLevel, target_pos: BlockPos) -> bool {
        if bee(mob).closer_than(target_pos, 2) {
            return true;
        }
        current_path(mob, level)
            .is_some_and(|p| p.get_target() == target_pos && p.can_reach() && p.is_done())
    }

    fn pathfind_directly_towards(
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
        target_pos: BlockPos,
    ) -> bool {
        let close_enough = if bee(mob).closer_than(target_pos, 3) {
            1
        } else {
            2
        };
        navigation::with_navigation(mob, level, |nav, m, l| {
            nav.set_max_visited_nodes_multiplier(10.0);
            nav.move_to_xyz_reach(
                m,
                &*l,
                f64::from(target_pos.x()),
                f64::from(target_pos.y()),
                f64::from(target_pos.z()),
                close_enough,
                1.0,
            );
        });
        current_path(mob, level).is_some_and(|p| p.can_reach())
    }

    fn drop_and_blacklist_hive(mob: &mut dyn Entity) {
        let b = bee_mut(mob);
        if let Some(hive_pos) = b.hive_pos {
            b.blacklisted_hives.push(hive_pos);
            while b.blacklisted_hives.len() > 3 {
                b.blacklisted_hives.remove(0);
            }
        }
        b.drop_hive();
    }
}

impl Goal for BeeGoToHiveGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeGoToHiveGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob, level) && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob, level) && not_angry(mob, level)
    }
    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
        navigation::with_navigation(mob, level, |nav, _, _| {
            nav.stop();
            nav.reset_max_visited_nodes_multiplier();
        });
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(hive_pos) = bee(mob).hive_pos else {
            return;
        };
        self.travelling_ticks += 1;
        if self.travelling_ticks > self.adjusted_tick_delay(2400) {
            Self::drop_and_blacklist_hive(mob);
        } else if !navigation::is_in_progress(mob, level) {
            if !bee(mob).closer_than(hive_pos, 16) {
                if bee(mob).is_too_far_away(hive_pos) {
                    bee_mut(mob).drop_hive();
                } else {
                    pathfind_randomly_towards(mob, level, hive_pos);
                }
            } else if !Self::pathfind_directly_towards(mob, level, hive_pos) {
                Self::drop_and_blacklist_hive(mob);
            } else if let Some(last) = &self.last_path
                && current_path(mob, level).is_some_and(|p| p.same_as(Some(last)))
            {
                self.ticks_stuck += 1;
                if self.ticks_stuck > 60 {
                    bee_mut(mob).drop_hive();
                    self.ticks_stuck = 0;
                }
            } else {
                self.last_path = current_path(mob, level);
            }
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeGoToKnownFlowerGoal`.
pub(super) struct BeeGoToKnownFlowerGoal {
    travelling_ticks: i32,
}

impl BeeGoToKnownFlowerGoal {
    pub(super) fn new() -> Self {
        Self {
            travelling_ticks: 0,
        }
    }

    fn can_bee_use(mob: &dyn Entity) -> bool {
        let b = bee(mob);
        b.saved_flower_pos.is_some_and(|flower| {
            !mob.mob().is_some_and(|m| m.has_home())
                && b.ticks_without_nectar_since_exiting_hive > 600
                && !b.closer_than(flower, 2)
        })
    }
}

impl Goal for BeeGoToKnownFlowerGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeGoToKnownFlowerGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob) && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob) && not_angry(mob, level)
    }
    fn start(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.travelling_ticks = 0;
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.travelling_ticks = 0;
        navigation::with_navigation(mob, level, |nav, _, _| {
            nav.stop();
            nav.reset_max_visited_nodes_multiplier();
        });
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(flower) = bee(mob).saved_flower_pos else {
            return;
        };
        self.travelling_ticks += 1;
        if self.travelling_ticks > self.adjusted_tick_delay(2400) {
            bee_mut(mob).drop_flower();
        } else if !navigation::is_in_progress(mob, level) {
            if bee(mob).is_too_far_away(flower) {
                bee_mut(mob).drop_flower();
            } else {
                pathfind_randomly_towards(mob, level, flower);
            }
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeGrowCropGoal`.
pub(super) struct BeeGrowCropGoal;

impl BeeGrowCropGoal {
    fn can_bee_use(mob: &mut dyn Entity, level: &ServerLevel) -> bool {
        // short-circuit: no random draw once the crop budget is spent
        if bee(mob).get_crops_grown_since_pollination() >= 10
            || mob.base_mut().random.next_float() < 0.3
        {
            false
        } else {
            bee(mob).has_nectar() && valid_hive_pos(mob, level).is_some()
        }
    }

    /// The block state the crop below the bee grows into.
    fn grow_state(
        level: &mut ServerLevel,
        pos: BlockPos,
        state: mc_data::block::BlockState,
    ) -> Option<mc_data::block::BlockState> {
        if let Some(grown) = crop_block::next_age_state_if_not_max(state) {
            Some(grown)
        } else if is_instance_of(state, "StemBlock") {
            let age = state.get(props::AGE_7);
            (age < 7).then(|| state.set(props::AGE_7, age + 1))
        } else if state.block() == Block::SWEET_BERRY_BUSH {
            let age = state.get(props::AGE_3);
            (age < 3).then(|| state.set(props::AGE_3, age + 1))
        } else if state.block() == Block::CAVE_VINES || state.block() == Block::CAVE_VINES_PLANT {
            let behaviour = mc_game_core::world::level::block::behaviour(state.block());
            if behaviour.is_valid_bonemeal_target_server(level, pos, state, BonemealSource::Mob) {
                behaviour.perform_bonemeal(level, pos, state, BonemealSource::Mob);
                Some(level.get_block_state(pos))
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Goal for BeeGrowCropGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeGrowCropGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob, level) && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::can_bee_use(mob, level) && not_angry(mob, level)
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let bound = self.adjusted_tick_delay(30);
        if mob.base_mut().random.next_int_bounded(bound) != 0 {
            return;
        }
        for i in 1..=2 {
            let below_pos = mob.block_position().below_n(i);
            let below_state = level.get_block_state(below_pos);
            if !below_state.is_tag("bee_growables") {
                continue;
            }
            if let Some(grow_state) = Self::grow_state(level, below_pos, below_state) {
                level.level_event(None, 2011, below_pos, 15);
                level.set_block_and_update(below_pos, grow_state);
                bee_mut(mob).increment_num_crops_grown_since_pollination();
            }
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Bee.BeeHurtByOtherGoal`: `setAlertOthers()`; an alerted bee targets the attacker when the hurt bee can see it.
pub(super) struct BeeHurtByOtherGoal {
    hurt: HurtByTargetGoal,
    /// `this.mob.hasLineOfSight(hurtByMob)`, evaluated for the hurt bee when the goal starts.
    can_see_attacker: Arc<AtomicBool>,
}

impl BeeHurtByOtherGoal {
    pub(super) fn new() -> Self {
        let can_see_attacker = Arc::new(AtomicBool::new(false));
        let mut hurt = HurtByTargetGoal::new(vec![]).set_alert_others(vec![]);
        let can_see = Arc::clone(&can_see_attacker);
        hurt.alert_other = Some(Box::new(move |level, other, hurt_by| {
            if can_see.load(Ordering::Relaxed) {
                level.with_entity(other, |level, o| o.set_target(level, Some(hurt_by)));
            }
        }));
        Self {
            hurt,
            can_see_attacker,
        }
    }
}

impl Goal for BeeHurtByOtherGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeHurtByOtherGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.hurt.can_use(mob, level)
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        neutral::is_angry(mob, level) && self.hurt.can_continue_to_use(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let sees = mob::combat::get_last_hurt_by_mob(mob, level)
            .and_then(|id| level.get_entity(id))
            .is_some_and(|attacker| ext::has_line_of_sight(level, mob, attacker));
        self.can_see_attacker.store(sees, Ordering::Relaxed);
        self.hurt.start(mob, level);
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.hurt.stop(mob, level);
    }
    fn flags(&self) -> GoalFlags {
        self.hurt.flags()
    }
}

/// `Bee.BeeLocateHiveGoal`.
pub(super) struct BeeLocateHiveGoal;

impl BeeLocateHiveGoal {
    /// `findNearbyHivesWithSpace()`.
    fn find_nearby_hives_with_space(mob: &dyn Entity, level: &ServerLevel) -> Vec<BlockPos> {
        let bee_pos = mob.block_position();
        let bee_home = Identifier::parse("minecraft:bee_home").expect("static id");
        let is_bee_home = |t: PoiTypeHolder| {
            Identifier::parse(t.registered_name()).is_ok_and(|id| {
                level
                    .registries
                    .is_in_tag("minecraft:point_of_interest_type", &id, &bee_home)
            })
        };
        let mut hives: Vec<BlockPos> = level
            .poi_manager()
            .get_in_range(&is_bee_home, bee_pos, 20, Occupancy::Any)
            .iter()
            .map(|r| r.get_pos())
            .filter(|pos| hive_at(level, *pos).is_some_and(|hive| !hive.is_full()))
            .collect();
        hives.sort_by(|a, b| a.dist_sqr(&bee_pos).total_cmp(&b.dist_sqr(&bee_pos)));
        hives
    }
}

impl Goal for BeeLocateHiveGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeLocateHiveGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let b = bee(mob);
        b.remaining_cooldown_before_locating_new_hive == 0
            && !b.has_hive()
            && wants_to_enter_hive(mob, level)
            && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        bee_mut(mob).remaining_cooldown_before_locating_new_hive = 200;
        let hives = Self::find_nearby_hives_with_space(mob, level);
        if hives.is_empty() {
            return;
        }
        let b = bee_mut(mob);
        if let Some(free) = hives.iter().find(|p| !b.blacklisted_hives.contains(p)) {
            b.hive_pos = Some(*free);
            return;
        }
        b.blacklisted_hives.clear();
        b.hive_pos = Some(hives[0]);
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Bee.BeePollinateGoal`.
pub(super) struct BeePollinateGoal {
    successful_pollinating_ticks: i32,
    last_sound_played_tick: i32,
    hover_pos: Option<Vec3>,
    pollinating_ticks: i32,
    unreachable_flower_cache: HashMap<i64, i64>,
}

impl BeePollinateGoal {
    pub(super) fn new() -> Self {
        Self {
            successful_pollinating_ticks: 0,
            last_sound_played_tick: 0,
            hover_pos: None,
            pollinating_ticks: 0,
            unreachable_flower_cache: HashMap::new(),
        }
    }

    fn has_pollinated_long_enough(&self) -> bool {
        self.successful_pollinating_ticks > 400
    }

    fn set_wanted_pos(&self, mob: &mut dyn Entity) {
        let hover = self.hover_pos.expect("hover position");
        mob.set_wanted_position(hover.x, hover.y, hover.z, f64::from(0.35f32));
    }

    fn get_offset(mob: &mut dyn Entity) -> f64 {
        f64::from((mob.base_mut().random.next_float() * 2.0 - 1.0) * 0.33333334f32)
    }

    /// `findNearbyFlower()`.
    fn find_nearby_flower(
        &mut self,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
    ) -> Option<BlockPos> {
        let mut temp_cache: HashMap<i64, i64> = HashMap::new();
        let game_time = level.game_time();
        let origin = mob.block_position();
        let mut result = None;
        for pos in BlockPos::within_box_by_manhattan_distance(&origin, 5, 5, 5) {
            if !attracts_bees(level.get_block_state(pos)) {
                continue;
            }
            let unreachable_until = self
                .unreachable_flower_cache
                .get(&pos.as_long())
                .copied()
                .unwrap_or(i64::MIN);
            if game_time < unreachable_until {
                temp_cache.insert(pos.as_long(), unreachable_until);
                continue;
            }
            let path = navigation::create_path_pos(mob, level, pos, 1);
            if path.is_some_and(|p| p.can_reach()) {
                result = Some(pos);
                break;
            }
            temp_cache.insert(pos.as_long(), game_time + 600);
        }
        self.unreachable_flower_cache = temp_cache;
        result
    }
}

impl Goal for BeePollinateGoal {
    fn name(&self) -> &'static str {
        "Bee$BeePollinateGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        // `canBeeUse()`
        if bee(mob).remaining_cooldown_before_locating_new_flower > 0
            || bee(mob).has_nectar()
            || level.is_raining()
        {
            return false;
        }
        match self.find_nearby_flower(mob, level) {
            Some(pos) => {
                bee_mut(mob).saved_flower_pos = Some(pos);
                navigation::move_to(
                    mob,
                    level,
                    f64::from(pos.x()) + 0.5,
                    f64::from(pos.y()) + 0.5,
                    f64::from(pos.z()) + 0.5,
                    f64::from(1.2f32),
                );
                not_angry(mob, level)
            }
            None => {
                bee_mut(mob).remaining_cooldown_before_locating_new_flower =
                    mob.base_mut().random.next_int_between_inclusive(20, 60);
                false
            }
        }
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        // `canBeeContinueToUse()`
        let can =
            if !bee(mob).pollinating || bee(mob).saved_flower_pos.is_none() || level.is_raining() {
                false
            } else if self.has_pollinated_long_enough() {
                mob.base_mut().random.next_float() < 0.2
            } else {
                true
            };
        can && not_angry(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.successful_pollinating_ticks = 0;
        self.pollinating_ticks = 0;
        self.last_sound_played_tick = 0;
        let b = bee_mut(mob);
        b.pollinating = true;
        b.reset_ticks_without_nectar_since_exiting_hive();
    }
    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let long_enough = self.has_pollinated_long_enough();
        let b = bee_mut(mob);
        if long_enough {
            b.set_has_nectar(true);
        }
        b.pollinating = false;
        navigation::stop(mob, level);
        bee_mut(mob).remaining_cooldown_before_locating_new_flower = 200;
    }
    fn requires_update_every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(saved) = bee(mob).saved_flower_pos else {
            return;
        };
        self.pollinating_ticks += 1;
        if self.pollinating_ticks > 600 {
            let b = bee_mut(mob);
            b.drop_flower();
            b.pollinating = false;
            b.remaining_cooldown_before_locating_new_flower = 200;
            return;
        }
        let flower_pos = Vec3::at_bottom_center_of(&saved).add_xyz(0.0, f64::from(0.6f32), 0.0);
        if flower_pos.distance_to(mob.position()) > 1.0 {
            self.hover_pos = Some(flower_pos);
            self.set_wanted_pos(mob);
            return;
        }
        let hover = *self.hover_pos.get_or_insert(flower_pos);
        let arrived_at_hover_pos = mob.position().distance_to(hover) <= 0.1;
        let mut should_set_wanted_pos = true;
        if arrived_at_hover_pos {
            if mob.base_mut().random.next_int_bounded(25) == 0 {
                let x = flower_pos.x + Self::get_offset(mob);
                let z = flower_pos.z + Self::get_offset(mob);
                self.hover_pos = Some(Vec3::new(x, flower_pos.y, z));
                navigation::stop(mob, level);
            } else {
                should_set_wanted_pos = false;
            }
            mob::look_at_xyz(mob, flower_pos.x, flower_pos.y, flower_pos.z);
        }
        if should_set_wanted_pos {
            self.set_wanted_pos(mob);
        }
        self.successful_pollinating_ticks += 1;
        if mob.base_mut().random.next_float() < 0.05
            && self.successful_pollinating_ticks > self.last_sound_played_tick + 60
        {
            self.last_sound_played_tick = self.successful_pollinating_ticks;
            mob.play_sound(level, SoundEvent::ENTITY_BEE_POLLINATE, 1.0, 1.0);
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.BeeWanderGoal`.
pub(super) struct BeeWanderGoal;

impl BeeWanderGoal {
    /// `getWanderThreshold()`.
    fn get_wander_threshold(mob: &dyn Entity) -> i32 {
        let b = bee(mob);
        let distance_reduction = if !b.has_hive() && !b.has_saved_flower_pos() {
            16
        } else {
            24
        };
        48 - distance_reduction
    }

    /// `findPos()`.
    fn find_pos(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        let wander_direction = match valid_hive_pos(mob, level) {
            Some(hive) if !bee(mob).closer_than(hive, Self::get_wander_threshold(mob)) => {
                Vec3::at_center_of(&hive)
                    .subtract(mob.position())
                    .normalize()
            }
            _ => mob.get_view_vector(0.0),
        };
        let ground_based = hover_random_pos::get_pos(
            mob,
            level,
            8,
            7,
            wander_direction.x,
            wander_direction.z,
            (PI / 2.0) as f32,
            3,
            1,
        );
        ground_based.or_else(|| {
            air_and_water_random_pos::get_pos(
                mob,
                level,
                8,
                4,
                -2,
                wander_direction.x,
                wander_direction.z,
                f64::from((PI / 2.0) as f32),
            )
        })
    }
}

impl Goal for BeeWanderGoal {
    fn name(&self) -> &'static str {
        "Bee$BeeWanderGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        navigation::is_done(mob, level) && mob.base_mut().random.next_int_bounded(10) == 0
    }
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        navigation::is_in_progress(mob, level)
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(target) = Self::find_pos(mob, level) {
            let path = navigation::create_path_pos(
                mob,
                level,
                BlockPos::containing(target.x, target.y, target.z),
                1,
            );
            navigation::move_to_path(mob, level, path, 1.0);
        }
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}

/// `Bee.ValidateFlowerGoal`.
pub(super) struct ValidateFlowerGoal {
    validate_flower_cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateFlowerGoal {
    pub(super) fn new(random: &mut dyn RandomSource) -> Self {
        Self {
            validate_flower_cooldown: random.next_int_between_inclusive(20, 40),
            last_validate_tick: -1,
        }
    }
}

impl Goal for ValidateFlowerGoal {
    fn name(&self) -> &'static str {
        "Bee$ValidateFlowerGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        level.game_time() > self.last_validate_tick + i64::from(self.validate_flower_cooldown)
            && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(flower) = bee(mob).saved_flower_pos
            && level.is_loaded(flower)
            && !attracts_bees(level.get_block_state(flower))
        {
            bee_mut(mob).drop_flower();
        }
        self.last_validate_tick = level.game_time();
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}

/// `Bee.ValidateHiveGoal`.
pub(super) struct ValidateHiveGoal {
    validate_hive_cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateHiveGoal {
    pub(super) fn new(random: &mut dyn RandomSource) -> Self {
        Self {
            validate_hive_cooldown: random.next_int_between_inclusive(20, 40),
            last_validate_tick: -1,
        }
    }
}

impl Goal for ValidateHiveGoal {
    fn name(&self) -> &'static str {
        "Bee$ValidateHiveGoal"
    }
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        level.game_time() > self.last_validate_tick + i64::from(self.validate_hive_cooldown)
            && not_angry(mob, level)
    }
    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(hive) = bee(mob).hive_pos
            && level.is_loaded(hive)
            && valid_hive_pos(mob, level).is_none()
        {
            bee_mut(mob).drop_hive();
        }
        self.last_validate_tick = level.game_time();
    }
    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
