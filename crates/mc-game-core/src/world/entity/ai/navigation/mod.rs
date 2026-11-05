//! `net.minecraft.world.entity.ai.navigation`: `PathNavigation` and its subclasses.
//!
//! # Shape
//! A navigation lives in the mob (`MobData.navigation: Option<Box<dyn PathNavigation>>`) and is `take`n out of it while
//! it runs, so every method receives the mob and the level explicitly where vanilla uses `this.mob` / `this.level`.
//! Navigation state is [`NavigationData`]; vanilla's class hierarchy (`PathNavigation` ⊃ `GroundPathNavigation` ⊃
//! `WallClimberNavigation`, `WaterBoundPathNavigation`, `FlyingPathNavigation`, `AmphibiousPathNavigation`) is a
//! [`NavClass`] tag, and **every overridable vanilla method is a [`PathNavigation`] trait method** whose default is the
//! tagged class's behavior ([`default_tick`], [`default_can_cut_corner`], ...). Standard navigations are
//! [`StandardPathNavigation`]; a mob-defined subclass (`Bee`'s anonymous `FlyingPathNavigation`, `Frog`/`Turtle` navigations,
//! `Strider`, `Creaking`, `Warden`, `HappyGhast`) is a struct that embeds a [`NavigationData`] built with the class it
//! extends, implements the trait and overrides what vanilla overrides; `super.xxx(..)` is `default_xxx(self, ..)`.
//! Since all internal calls go through the trait, an override is seen by the navigation's own logic exactly like Java's
//! virtual dispatch.
//!
//! The constructors need only `(&dyn Entity, &ServerLevel)`. Targets of `create_path_*` that vanilla receives as an
//! `Entity` are passed as `target.blockPosition()` (all the navigation reads of it).

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use crate::world::level::material::FluidStateExt;
use crate::world::level::path_navigation_region::PathNavigationRegion;
use crate::world::level::pathfinder::node_evaluator::{EvaluatorState, NodeEvaluator, PathEnv};
use crate::world::level::pathfinder::pathfinding_context::PathfindingContext;
use crate::world::level::pathfinder::walk_node_evaluator::{get_floor_level_static, get_path_type_static};
use crate::world::level::pathfinder::{EntityMob, Node, Path, PathFinder, PathType, PathfindingMob};
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use std::any::Any;

/// `MAX_TIME_RECOMPUTE`.
const MAX_TIME_RECOMPUTE: i64 = 20;
/// `STUCK_CHECK_INTERVAL`.
const STUCK_CHECK_INTERVAL: i32 = 100;
/// `STUCK_THRESHOLD_DISTANCE_FACTOR`.
const STUCK_THRESHOLD_DISTANCE_FACTOR: f32 = 0.25;

/// The vanilla class a navigation extends (see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavClass {
    Ground,
    WallClimber,
    Water,
    Flying,
    Amphibious,
}

impl NavClass {
    /// `this instanceof GroundPathNavigation`.
    pub fn is_ground(self) -> bool {
        matches!(self, NavClass::Ground | NavClass::WallClimber)
    }
}

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The fields of `PathNavigation` and of the standard subclasses.
pub struct NavigationData {
    pub class: NavClass,
    pub path: Option<Path>,
    pub speed_modifier: f64,
    pub tick: i32,
    pub last_stuck_check: i32,
    pub last_stuck_check_pos: Vec3,
    pub timeout_cached_node: BlockPos,
    pub timeout_timer: i64,
    pub last_timeout_check: i64,
    pub timeout_limit: f64,
    pub max_distance_to_waypoint: f32,
    pub has_delayed_recomputation: bool,
    pub time_last_recompute: i64,
    target_pos: Option<BlockPos>,
    reach_range: i32,
    max_visited_nodes_multiplier: f32,
    /// `pathFinder` (owns the node evaluator, which vanilla also keeps as `nodeEvaluator`).
    pub path_finder: PathFinder,
    is_stuck: bool,
    required_path_length: f32,
    // ---- GroundPathNavigation / WallClimberNavigation / WaterBoundPathNavigation state ----
    pub avoid_sun: bool,
    pub can_path_to_targets_below_surface: bool,
    /// `WallClimberNavigation.pathToPosition`.
    pub path_to_position: Option<BlockPos>,
    /// `WaterBoundPathNavigation.allowBreaching`.
    pub allow_breaching: bool,
}

impl NavigationData {
    /// `super(mob, level)` of a navigation that builds its path finder with `create_path_finder(maxVisitedNodes)`
    /// (vanilla's `createPathFinder`).
    pub fn with_path_finder(mob: &dyn Entity, class: NavClass, create_path_finder: impl FnOnce(i32) -> PathFinder) -> NavigationData {
        let follow_range = mob.living().map_or(0.0, |l| l.attributes.get_base_value(mc_data::Attribute::FOLLOW_RANGE));
        Self::from_follow_range(class, follow_range, create_path_finder)
    }

    /// [`NavigationData::with_path_finder`] from the mob's `FOLLOW_RANGE` base value.
    pub fn from_follow_range(class: NavClass, follow_range_base: f64, create_path_finder: impl FnOnce(i32) -> PathFinder) -> NavigationData {
        let path_finder = create_path_finder(mth::floor_f64(follow_range_base * 16.0));
        NavigationData {
            class,
            path: None,
            speed_modifier: 0.0,
            tick: 0,
            last_stuck_check: 0,
            last_stuck_check_pos: Vec3::ZERO,
            timeout_cached_node: BlockPos::ZERO,
            timeout_timer: 0,
            last_timeout_check: 0,
            timeout_limit: 0.0,
            max_distance_to_waypoint: 0.5,
            has_delayed_recomputation: false,
            time_last_recompute: 0,
            target_pos: None,
            reach_range: 0,
            max_visited_nodes_multiplier: 1.0,
            path_finder,
            is_stuck: false,
            required_path_length: 16.0,
            avoid_sun: false,
            can_path_to_targets_below_surface: false,
            path_to_position: None,
            allow_breaching: false,
        }
    }

    /// `super(mob, level)` with the class's standard `createPathFinder`.
    pub fn new(mob: &dyn Entity, class: NavClass) -> NavigationData {
        let follow_range = mob.living().map_or(0.0, |l| l.attributes.get_base_value(mc_data::Attribute::FOLLOW_RANGE));
        NavigationData::for_class(class, follow_range, mob.get_type() == EntityType::DOLPHIN)
    }

    /// The standard navigation data of `class` for a mob with this `FOLLOW_RANGE` base value (`is_dolphin`:
    /// `WaterBoundPathNavigation.allowBreaching`).
    pub fn for_class(class: NavClass, follow_range_base: f64, is_dolphin: bool) -> NavigationData {
        let mut data = NavigationData::from_follow_range(class, follow_range_base, |max_visited| match class {
            NavClass::Ground | NavClass::WallClimber => PathFinder::new(Box::new(Evaluator(EvaluatorState::walk())), max_visited),
            NavClass::Water => {
                let mut evaluator = Evaluator(EvaluatorState::swim(is_dolphin));
                evaluator.set_can_pass_doors(false);
                PathFinder::new(Box::new(evaluator), max_visited)
            }
            NavClass::Flying => PathFinder::new(Box::new(Evaluator(EvaluatorState::fly())), max_visited),
            NavClass::Amphibious => PathFinder::new(Box::new(Evaluator(EvaluatorState::amphibious(false))), max_visited),
        });
        data.allow_breaching = class == NavClass::Water && is_dolphin;
        data
    }

    /// `getMaxPathLength()` for a mob whose `FOLLOW_RANGE` attribute value is `follow_range_value`.
    pub fn max_path_length(&self, follow_range_value: f64) -> f32 {
        (follow_range_value as f32).max(self.required_path_length)
    }

    /// `setRequiredPathLength(length)` + `updatePathfinderMaxVisitedNodes()` for that follow range value.
    pub fn set_required_path_length(&mut self, follow_range_value: f64, length: f32) {
        self.required_path_length = length;
        let max_visited_nodes = mth::floor_f32(self.max_path_length(follow_range_value) * 16.0);
        self.path_finder.set_max_visited_nodes(max_visited_nodes);
    }

    /// `getTargetPos()`.
    pub fn target_pos(&self) -> Option<BlockPos> {
        self.target_pos
    }

    /// `nodeEvaluator`.
    pub fn node_evaluator(&self) -> &dyn NodeEvaluator {
        self.path_finder.node_evaluator()
    }

    pub fn node_evaluator_mut(&mut self) -> &mut dyn NodeEvaluator {
        self.path_finder.node_evaluator_mut()
    }
}

/// A stock evaluator with no overrides.
struct Evaluator(EvaluatorState);

impl NodeEvaluator for Evaluator {
    fn state(&self) -> &EvaluatorState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut EvaluatorState {
        &mut self.0
    }
}

/// `PathNavigation` (see the module docs). Methods taking `&dyn Entity`/`&ServerLevel` only read; the ones that run
/// the path finder take `&mut dyn Entity` (the evaluators may adjust the mob's maluses and run its pathfinding hooks).
pub trait PathNavigation: AsAny {
    fn data(&self) -> &NavigationData;
    fn data_mut(&mut self) -> &mut NavigationData;

    // ---- abstract / overridable behavior -----------------------------------------------------------------------

    /// `canUpdatePath()`.
    fn can_update_path(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        default_can_update_path(self, mob, level)
    }
    /// `getTempMobPos()`.
    fn get_temp_mob_pos(&self, mob: &dyn Entity, level: &ServerLevel) -> Vec3 {
        default_get_temp_mob_pos(self, mob, level)
    }
    /// `canNavigateGround()`.
    fn can_navigate_ground(&self) -> bool {
        matches!(self.data().class, NavClass::Ground | NavClass::WallClimber | NavClass::Amphibious)
    }
    /// `canMoveDirectly(startPos, stopPos)`.
    fn can_move_directly(&self, mob: &dyn Entity, level: &ServerLevel, start_pos: Vec3, stop_pos: Vec3) -> bool {
        default_can_move_directly(self, mob, level, start_pos, stop_pos)
    }
    /// `canCutCorner(pathType)`.
    fn can_cut_corner(&self, path_type: PathType) -> bool {
        default_can_cut_corner(self, path_type)
    }
    /// `isStableDestination(pos)`.
    fn is_stable_destination(&self, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
        default_is_stable_destination(self, mob, level, pos)
    }
    /// `getGroundY(target)`.
    fn get_ground_y(&self, level: &ServerLevel, target: Vec3) -> f64 {
        default_get_ground_y(self, level, target)
    }
    /// `getMaxVerticalDistanceToWaypoint()`.
    fn get_max_vertical_distance_to_waypoint(&self) -> f32 {
        if self.data().class == NavClass::Water { 0.5 } else { 1.0 }
    }
    /// `trimPath()`.
    fn trim_path(&mut self, mob: &dyn Entity, level: &ServerLevel) {
        default_trim_path(self, mob, level)
    }
    /// `setCanFloat(canFloat)`.
    fn set_can_float(&mut self, can_float: bool) {
        default_set_can_float(self, can_float)
    }

    // ---- path creation -------------------------------------------------------------------------------------------

    /// `createPath(BlockPos pos, reachRange)`.
    fn create_path_pos(&mut self, mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos, reach_range: i32) -> Option<Path> {
        default_create_path_pos(self, mob, level, pos, reach_range)
    }
    /// `createPath(Entity target, reachRange)` (`target_pos` = `target.blockPosition()`).
    fn create_path_entity(&mut self, mob: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos, reach_range: i32) -> Option<Path> {
        default_create_path_entity(self, mob, level, target_pos, reach_range)
    }
    /// `createPath(x, y, z, reachRange)` (final).
    fn create_path_xyz(&mut self, mob: &mut dyn Entity, level: &ServerLevel, x: f64, y: f64, z: f64, reach_range: i32) -> Option<Path> {
        self.create_path_pos(mob, level, BlockPos::containing(x, y, z), reach_range)
    }
    /// `createPath(Set<BlockPos>/Stream<BlockPos>, reachRange)`; `positions` in the iteration order of the vanilla set.
    fn create_path_set(&mut self, mob: &mut dyn Entity, level: &ServerLevel, positions: &[BlockPos], reach_range: i32) -> Option<Path> {
        self.create_path_targets(mob, level, positions, 8, false, reach_range)
    }
    /// `createPath(BlockPos pos, reachRange, maxPathLength)`.
    fn create_path_pos_len(&mut self, mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos, reach_range: i32, max_path_length: f32) -> Option<Path> {
        create_path_targets(self, mob, level, &[pos], 8, false, reach_range, max_path_length)
    }
    /// `createPath(targets, radiusOffset, above, reachRange)`.
    fn create_path_targets(&mut self, mob: &mut dyn Entity, level: &ServerLevel, targets: &[BlockPos], radius_offset: i32, above: bool, reach_range: i32) -> Option<Path> {
        let max_path_length = get_max_path_length(self, mob);
        create_path_targets(self, mob, level, targets, radius_offset, above, reach_range, max_path_length)
    }

    // ---- moveTo ---------------------------------------------------------------------------------------------------

    /// `moveTo(x, y, z, speedModifier)`.
    fn move_to_xyz(&mut self, mob: &mut dyn Entity, level: &ServerLevel, x: f64, y: f64, z: f64, speed_modifier: f64) -> bool {
        let path = self.create_path_xyz(mob, level, x, y, z, 1);
        self.move_to_path(mob, level, path, speed_modifier)
    }
    /// `moveTo(x, y, z, reachRange, speedModifier)`.
    fn move_to_xyz_reach(&mut self, mob: &mut dyn Entity, level: &ServerLevel, x: f64, y: f64, z: f64, reach_range: i32, speed_modifier: f64) -> bool {
        let path = self.create_path_xyz(mob, level, x, y, z, reach_range);
        self.move_to_path(mob, level, path, speed_modifier)
    }
    /// `moveTo(Entity target, speedModifier)` (`target_pos` = `target.blockPosition()`).
    fn move_to_entity(&mut self, mob: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos, speed_modifier: f64) -> bool {
        default_move_to_entity(self, mob, level, target_pos, speed_modifier)
    }
    /// `moveTo(Entity target, reachRange, speedModifier)`.
    fn move_to_entity_reach(&mut self, mob: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos, reach_range: i32, speed_modifier: f64) -> bool {
        match self.create_path_entity(mob, level, target_pos, reach_range) {
            Some(path) => self.move_to_path(mob, level, Some(path), speed_modifier),
            None => false,
        }
    }
    /// `moveTo(Path, speedModifier)`.
    fn move_to_path(&mut self, mob: &mut dyn Entity, level: &ServerLevel, new_path: Option<Path>, speed_modifier: f64) -> bool {
        default_move_to_path(self, mob, level, new_path, speed_modifier)
    }

    // ---- ticking -------------------------------------------------------------------------------------------------

    /// `tick()`.
    fn tick(&mut self, mob: &mut dyn Entity, level: &ServerLevel) {
        default_tick(self, mob, level)
    }
    /// `followThePath()`.
    fn follow_the_path(&mut self, mob: &mut dyn Entity, level: &ServerLevel) {
        default_follow_the_path(self, mob, level)
    }
    /// `doStuckDetection(mobPos)`.
    fn do_stuck_detection(&mut self, mob: &dyn Entity, level: &ServerLevel, mob_pos: Vec3) {
        default_do_stuck_detection(self, mob, level, mob_pos)
    }
    /// `recomputePath()`.
    fn recompute_path(&mut self, mob: &mut dyn Entity, level: &ServerLevel) {
        default_recompute_path(self, mob, level)
    }
    /// `stop()`.
    fn stop(&mut self) {
        self.data_mut().path = None;
    }

    // ---- simple accessors ----------------------------------------------------------------------------------------

    /// `getPath()` (a handle to the navigation's path).
    fn get_path(&self) -> Option<Path> {
        self.data().path.clone()
    }
    /// `getTargetPos()`.
    fn get_target_pos(&self) -> Option<BlockPos> {
        self.data().target_pos
    }
    /// `setSpeedModifier(speedModifier)`.
    fn set_speed_modifier(&mut self, speed_modifier: f64) {
        self.data_mut().speed_modifier = speed_modifier;
    }
    /// `isDone()`.
    fn is_done(&self) -> bool {
        self.data().path.as_ref().is_none_or(Path::is_done)
    }
    /// `isInProgress()`.
    fn is_in_progress(&self) -> bool {
        !self.is_done()
    }
    /// `isStuck()`.
    fn is_stuck(&self) -> bool {
        self.data().is_stuck
    }
    /// `canFloat()`.
    fn can_float(&self) -> bool {
        self.data().node_evaluator().can_float()
    }
    /// `setCanOpenDoors(canOpenDoors)`.
    fn set_can_open_doors(&mut self, can_open_doors: bool) {
        self.data_mut().node_evaluator_mut().set_can_open_doors(can_open_doors);
    }
    /// `getMaxDistanceToWaypoint()`.
    fn get_max_distance_to_waypoint(&self) -> f32 {
        self.data().max_distance_to_waypoint
    }
    /// `resetMaxVisitedNodesMultiplier()`.
    fn reset_max_visited_nodes_multiplier(&mut self) {
        self.data_mut().max_visited_nodes_multiplier = 1.0;
    }
    /// `setMaxVisitedNodesMultiplier(multiplier)`.
    fn set_max_visited_nodes_multiplier(&mut self, multiplier: f32) {
        self.data_mut().max_visited_nodes_multiplier = multiplier;
    }
    /// `updatePathfinderMaxVisitedNodes()`.
    fn update_pathfinder_max_visited_nodes(&mut self, mob: &dyn Entity) {
        let max_visited_nodes = mth::floor_f32(get_max_path_length(self, mob) * 16.0);
        self.data_mut().path_finder.set_max_visited_nodes(max_visited_nodes);
    }
    /// `setRequiredPathLength(length)`.
    fn set_required_path_length(&mut self, mob: &dyn Entity, length: f32) {
        let follow_range = mob.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::FOLLOW_RANGE));
        self.data_mut().set_required_path_length(follow_range, length);
    }
    /// `shouldRecomputePath(pos)`.
    fn should_recompute_path(&self, mob: &dyn Entity, pos: BlockPos) -> bool {
        let d = self.data();
        if d.has_delayed_recomputation {
            return false;
        }
        match &d.path {
            Some(path) if !path.is_done() && path.get_node_count() != 0 => {
                let target = path.get_end_node().expect("non-empty path");
                let b = mob.base();
                let middle = Vec3::new((f64::from(target.x) + b.x()) / 2.0, (f64::from(target.y) + b.y()) / 2.0, (f64::from(target.z) + b.z()) / 2.0);
                pos.closer_to_center_than(&middle, f64::from(path.get_node_count() as i32 - path.get_next_node_index()))
            }
            _ => false,
        }
    }
    /// `getNodeEvaluator().getPathType(mob, pos)`.
    fn get_node_evaluator_path_type(&mut self, mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos) -> PathType {
        let mut view = EntityMob::new(mob, level);
        let context = PathfindingContext::new(level, Some(level.path_type_cache()), &view);
        let mut env = PathEnv { mob: &mut view, context };
        self.data_mut().node_evaluator_mut().get_path_type(&mut env, pos.x(), pos.y(), pos.z())
    }
}

/// `Math.max(mob.getAttributeValue(FOLLOW_RANGE), requiredPathLength)`.
fn get_max_path_length<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity) -> f32 {
    let follow_range = mob.living().map_or(0.0, |l| l.get_attribute_value(mc_data::Attribute::FOLLOW_RANGE));
    this.data().max_path_length(follow_range)
}

/// `WalkNodeEvaluator.getPathTypeStatic(mob, pos)`.
pub fn get_path_type_static_for_mob(mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos) -> PathType {
    let view = EntityMob::new(mob, level);
    let context = PathfindingContext::new(level, Some(level.path_type_cache()), &view);
    get_path_type_static(&context, pos)
}

/// `new GroundPathNavigation` & co (no overrides).
pub struct StandardPathNavigation {
    data: NavigationData,
}

impl StandardPathNavigation {
    pub fn new(mob: &dyn Entity, class: NavClass) -> Self {
        StandardPathNavigation { data: NavigationData::new(mob, class) }
    }
    /// `new GroundPathNavigation(mob, level)`.
    pub fn ground(mob: &dyn Entity) -> Self {
        Self::new(mob, NavClass::Ground)
    }
    /// `new WallClimberNavigation(mob, level)`.
    pub fn wall_climber(mob: &dyn Entity) -> Self {
        Self::new(mob, NavClass::WallClimber)
    }
    /// `new WaterBoundPathNavigation(mob, level)`.
    pub fn water_bound(mob: &dyn Entity) -> Self {
        Self::new(mob, NavClass::Water)
    }
    /// `new FlyingPathNavigation(mob, level)`.
    pub fn flying(mob: &dyn Entity) -> Self {
        Self::new(mob, NavClass::Flying)
    }
    /// `new AmphibiousPathNavigation(mob, level)`.
    pub fn amphibious(mob: &dyn Entity) -> Self {
        Self::new(mob, NavClass::Amphibious)
    }
}

impl PathNavigation for StandardPathNavigation {
    fn data(&self) -> &NavigationData {
        &self.data
    }
    fn data_mut(&mut self) -> &mut NavigationData {
        &mut self.data
    }
}

/// `GroundPathNavigation.setAvoidSun` etc. are plain fields of [`NavigationData`]; these mirror the setters.
impl NavigationData {
    pub fn set_avoid_sun(&mut self, avoid_sun: bool) {
        self.avoid_sun = avoid_sun;
    }
    pub fn set_can_walk_over_fences(&mut self, can_walk_over_fences: bool) {
        self.node_evaluator_mut().set_can_walk_over_fences(can_walk_over_fences);
    }
    pub fn set_can_path_to_targets_below_surface(&mut self, v: bool) {
        self.can_path_to_targets_below_surface = v;
    }
}

// ---- class implementations ("super") --------------------------------------------------------------------------------

pub fn default_can_update_path<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, _level: &ServerLevel) -> bool {
    match this.data().class {
        NavClass::Ground | NavClass::WallClimber => mob.base().on_ground() || mob.is_in_liquid() || mob.base().is_passenger(),
        NavClass::Water => this.data().allow_breaching || mob.is_in_liquid(),
        NavClass::Flying => this.can_float() && mob.is_in_liquid() || !mob.base().is_passenger(),
        NavClass::Amphibious => true,
    }
}

pub fn default_get_temp_mob_pos<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, level: &ServerLevel) -> Vec3 {
    let b = mob.base();
    match this.data().class {
        NavClass::Ground | NavClass::WallClimber => Vec3::new(b.x(), f64::from(get_surface_y(this, mob, level)), b.z()),
        NavClass::Water | NavClass::Amphibious => Vec3::new(b.x(), b.y() + f64::from(b.get_bb_height()) * 0.5, b.z()),
        NavClass::Flying => b.position(),
    }
}

/// `GroundPathNavigation.getSurfaceY()`.
fn get_surface_y<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, level: &ServerLevel) -> i32 {
    let b = mob.base();
    if mob.is_in_floatable_fluid() && this.can_float() {
        let mut surface = b.get_block_y();
        let mut state = level.get_block_state(BlockPos::containing(b.x(), f64::from(surface), b.z()));
        let mut steps = 0;
        while state.get_fluid_state().is_tag("entity_floatable") {
            surface += 1;
            state = level.get_block_state(BlockPos::containing(b.x(), f64::from(surface), b.z()));
            steps += 1;
            if steps > 16 {
                return b.get_block_y();
            }
        }
        surface
    } else {
        mth::floor_f64(b.y() + 0.5)
    }
}

pub fn default_can_move_directly<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, level: &ServerLevel, start_pos: Vec3, stop_pos: Vec3) -> bool {
    match this.data().class {
        NavClass::Water => is_clear_for_movement_between(mob, level, start_pos, stop_pos, false),
        NavClass::Flying => is_clear_for_movement_between(mob, level, start_pos, stop_pos, true),
        NavClass::Amphibious => mob.is_in_liquid() && is_clear_for_movement_between(mob, level, start_pos, stop_pos, false),
        NavClass::Ground | NavClass::WallClimber => false,
    }
}

/// `PathNavigation.isClearForMovementBetween(mob, start, stop, blockedByFluids)`.
pub fn is_clear_for_movement_between(mob: &dyn Entity, level: &ServerLevel, start_pos: Vec3, stop_pos: Vec3, blocked_by_fluids: bool) -> bool {
    let to = Vec3::new(stop_pos.x, stop_pos.y + f64::from(mob.base().get_bb_height()) * 0.5, stop_pos.z);
    let context = ClipContext::new(
        start_pos,
        to,
        ClipBlock::Collider,
        if blocked_by_fluids { ClipFluid::Any } else { ClipFluid::None },
        crate::world::entity::movement::collision_context(mob),
    );
    level.clip(&context).miss
}

pub fn default_can_cut_corner<N: PathNavigation + ?Sized>(_this: &N, path_type: PathType) -> bool {
    path_type != PathType::FireInNeighbor && path_type != PathType::DamagingInNeighbor && path_type != PathType::WalkableDoor
}

pub fn default_is_stable_destination<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
    match this.data().class {
        NavClass::Ground | NavClass::WallClimber => level.get_block_state(pos.below()).is_solid_render(),
        NavClass::Water => !level.get_block_state(pos).is_solid_render(),
        NavClass::Flying => entity_can_stand_on(level.get_block_state(pos), level, pos, mob),
        NavClass::Amphibious => !level.get_block_state(pos.below()).is_air(),
    }
}

/// `BlockStateBase.entityCanStandOn(level, pos, entity)`.
pub fn entity_can_stand_on(state: mc_data::block::BlockState, level: &dyn BlockGetter, pos: BlockPos, entity: &dyn Entity) -> bool {
    let shape = state.get_collision_shape_ctx(level, pos, &crate::world::entity::movement::collision_context(entity));
    crate::world::level::shapes::is_face_full(&shape, Direction::Up)
}

pub fn default_get_ground_y<N: PathNavigation + ?Sized>(this: &N, level: &ServerLevel, target: Vec3) -> f64 {
    if matches!(this.data().class, NavClass::Water | NavClass::Amphibious) {
        return target.y;
    }
    let block_pos = BlockPos::containing(target.x, target.y, target.z);
    if level.get_block_state(block_pos.below()).is_air() { target.y } else { get_floor_level_static(level, block_pos) }
}

pub fn default_trim_path<N: PathNavigation + ?Sized>(this: &mut N, mob: &dyn Entity, level: &ServerLevel) {
    let Some(path) = this.data().path.clone() else { return };
    // PathNavigation.trimPath: raise nodes in cauldrons
    for i in 0..path.get_node_count() {
        let node = path.get_node(i);
        let next_node = if i + 1 < path.get_node_count() { Some(path.get_node(i + 1)) } else { None };
        let state = level.get_block_state(BlockPos::new(node.x, node.y, node.z));
        if state.is_tag("cauldrons") {
            path.replace_node(i, node.clone_and_move(node.x, node.y + 1, node.z));
            if let Some(next) = next_node
                && node.y >= next.y
            {
                path.replace_node(i + 1, node.clone_and_move(next.x, node.y + 1, next.z));
            }
        }
    }
    // GroundPathNavigation.trimPath
    if this.data().class.is_ground() && this.data().avoid_sun {
        let b = mob.base();
        if level.can_see_sky(BlockPos::containing(b.x(), b.y() + 0.5, b.z())) {
            return;
        }
        for i in 0..path.get_node_count() {
            let node = path.get_node(i);
            if level.can_see_sky(BlockPos::new(node.x, node.y, node.z)) {
                path.truncate_nodes(i);
                return;
            }
        }
    }
}

pub fn default_set_can_float<N: PathNavigation + ?Sized>(this: &mut N, can_float: bool) {
    // WaterBoundPathNavigation and AmphibiousPathNavigation ignore it
    if !matches!(this.data().class, NavClass::Water | NavClass::Amphibious) {
        this.data_mut().node_evaluator_mut().set_can_float(can_float);
    }
}

pub fn default_create_path_pos<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel, mut pos: BlockPos, reach_range: i32) -> Option<Path> {
    if this.data().class == NavClass::WallClimber {
        this.data_mut().path_to_position = Some(pos);
    }
    if this.data().class.is_ground() {
        // GroundPathNavigation.createPath(pos, reachRange)
        level.chunk_now(pos.x() >> 4, pos.z() >> 4)?;
        if !this.data().can_path_to_targets_below_surface {
            pos = find_surface_position(level, pos);
        }
    }
    let max_path_length = get_max_path_length(this, mob);
    create_path_targets(this, mob, level, &[pos], 8, false, reach_range, max_path_length)
}

/// `GroundPathNavigation.findSurfacePosition(chunk, pos, reachRange)`.
pub fn find_surface_position(level: &ServerLevel, mut pos: BlockPos) -> BlockPos {
    let (min_y, max_y) = (level.get_min_y(), level.get_max_y());
    if level.get_block_state(pos).is_air() {
        let mut column = pos.below();
        while column.y() >= min_y && level.get_block_state(column).is_air() {
            column = column.below();
        }
        if column.y() >= min_y {
            return column.above();
        }
        column = BlockPos::new(column.x(), pos.y() + 1, column.z());
        while column.y() <= max_y && level.get_block_state(column).is_air() {
            column = column.above();
        }
        pos = column;
    }
    if !level.get_block_state(pos).is_solid() {
        return pos;
    }
    let mut column = pos.above();
    while column.y() <= max_y && level.get_block_state(column).is_solid() {
        column = column.above();
    }
    column
}

pub fn default_create_path_entity<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos, reach_range: i32) -> Option<Path> {
    match this.data().class {
        NavClass::Ground | NavClass::Flying => this.create_path_pos(mob, level, target_pos, reach_range),
        NavClass::WallClimber => {
            this.data_mut().path_to_position = Some(target_pos);
            this.create_path_pos(mob, level, target_pos, reach_range)
        }
        NavClass::Water | NavClass::Amphibious => this.create_path_targets(mob, level, &[target_pos], 16, true, reach_range),
    }
}

/// `createPath(targets, radiusOffset, above, reachRange, maxPathLength)`.
#[allow(clippy::too_many_arguments)]
pub fn create_path_targets<N: PathNavigation + ?Sized>(
    this: &mut N,
    mob: &mut dyn Entity,
    level: &ServerLevel,
    targets: &[BlockPos],
    radius_offset: i32,
    above: bool,
    reach_range: i32,
    max_path_length: f32,
) -> Option<Path> {
    if targets.is_empty() {
        return None;
    }
    if mob.base().y() < f64::from(level.get_min_y()) {
        return None;
    }
    if !this.can_update_path(mob, level) {
        return None;
    }
    if let Some(path) = &this.data().path
        && !path.is_done()
        && this.data().target_pos.is_some_and(|t| targets.contains(&t))
    {
        return Some(path.clone());
    }
    let multiplier = this.data().max_visited_nodes_multiplier;
    let mut view = EntityMob::new(mob, level);
    create_path_with_finder(this.data_mut(), &mut view, level, targets, radius_offset, above, reach_range, max_path_length, multiplier)
}

/// The path-finding part of `createPath(targets, radiusOffset, above, reachRange, maxPathLength)` after the guards:
/// builds the `PathNavigationRegion`, runs the finder and records the target.
#[allow(clippy::too_many_arguments)]
pub fn create_path_with_finder(
    d: &mut NavigationData,
    mob: &mut dyn PathfindingMob,
    level: &ServerLevel,
    targets: &[BlockPos],
    radius_offset: i32,
    above: bool,
    reach_range: i32,
    max_path_length: f32,
    max_visited_nodes_multiplier: f32,
) -> Option<Path> {
    let block_pos = mob.block_position();
    let from_pos = if above { block_pos.above() } else { block_pos };
    let radius = (max_path_length + radius_offset as f32) as i32;
    let region = PathNavigationRegion::new(level, from_pos.offset(-radius, -radius, -radius).into(), from_pos.offset(radius, radius, radius).into());
    let path = d.path_finder.find_path(&region, mob, targets, max_path_length, reach_range, max_visited_nodes_multiplier);
    if let Some(path) = &path {
        d.target_pos = Some(path.get_target());
        d.reach_range = reach_range;
        reset_stuck_timeout(d);
    }
    path
}

pub fn default_move_to_entity<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel, target_pos: BlockPos, speed_modifier: f64) -> bool {
    if this.data().class == NavClass::WallClimber {
        return match this.create_path_entity(mob, level, target_pos, 0) {
            Some(path) => this.move_to_path(mob, level, Some(path), speed_modifier),
            None => {
                let d = this.data_mut();
                d.path_to_position = Some(target_pos);
                d.speed_modifier = speed_modifier;
                true
            }
        };
    }
    this.move_to_entity_reach(mob, level, target_pos, 1, speed_modifier)
}

pub fn default_move_to_path<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel, new_path: Option<Path>, speed_modifier: f64) -> bool {
    let Some(new_path) = new_path else {
        this.data_mut().path = None;
        return false;
    };
    if !new_path.same_as(this.data().path.as_ref()) {
        this.data_mut().path = Some(new_path);
    }
    if this.is_done() {
        return false;
    }
    this.trim_path(mob, level);
    if this.data().path.as_ref().is_none_or(|p| p.get_node_count() == 0) {
        return false;
    }
    this.data_mut().speed_modifier = speed_modifier;
    let mob_pos = this.get_temp_mob_pos(mob, level);
    let d = this.data_mut();
    d.last_stuck_check = d.tick;
    d.last_stuck_check_pos = mob_pos;
    true
}

pub fn default_tick<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    match this.data().class {
        NavClass::WallClimber => wall_climber_tick(this, mob, level),
        NavClass::Flying => flying_tick(this, mob, level),
        _ => navigation_tick(this, mob, level),
    }
}

/// `PathNavigation.tick()`.
pub fn navigation_tick<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    this.data_mut().tick += 1;
    if this.data().has_delayed_recomputation {
        this.recompute_path(mob, level);
    }
    if !this.is_done() {
        if this.can_update_path(mob, level) {
            this.follow_the_path(mob, level);
        } else if let Some(path) = this.data().path.clone()
            && !path.is_done()
        {
            let mob_pos = this.get_temp_mob_pos(mob, level);
            let pos = path.get_next_entity_pos(mob.base().get_bb_width());
            if mob_pos.y > pos.y && !mob.base().on_ground() && mth::floor_f64(mob_pos.x) == mth::floor_f64(pos.x) && mth::floor_f64(mob_pos.z) == mth::floor_f64(pos.z) {
                path.advance();
            }
        }
        if !this.is_done() {
            let path = this.data().path.clone().expect("in progress");
            let target = path.get_next_entity_pos(mob.base().get_bb_width());
            let ground_y = this.get_ground_y(level, target);
            let speed = this.data().speed_modifier;
            mob.set_wanted_position(target.x, ground_y, target.z, speed);
        }
    }
}

/// `FlyingPathNavigation.tick()`.
fn flying_tick<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    this.data_mut().tick += 1;
    if this.data().has_delayed_recomputation {
        this.recompute_path(mob, level);
    }
    if !this.is_done() {
        if this.can_update_path(mob, level) {
            this.follow_the_path(mob, level);
        } else if let Some(path) = this.data().path.clone()
            && !path.is_done()
        {
            let pos = path.get_next_entity_pos(mob.base().get_bb_width());
            let b = mob.base();
            if b.get_block_x() == mth::floor_f64(pos.x) && b.get_block_y() == mth::floor_f64(pos.y) && b.get_block_z() == mth::floor_f64(pos.z) {
                path.advance();
            }
        }
        if !this.is_done() {
            let path = this.data().path.clone().expect("in progress");
            let target = path.get_next_entity_pos(mob.base().get_bb_width());
            let speed = this.data().speed_modifier;
            mob.set_wanted_position(target.x, target.y, target.z, speed);
        }
    }
}

/// `WallClimberNavigation.tick()`.
fn wall_climber_tick<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    if !this.is_done() {
        navigation_tick(this, mob, level);
        return;
    }
    let Some(ptp) = this.data().path_to_position else { return };
    let (position, width, y) = (mob.base().position(), f64::from(mob.base().get_bb_width()), mob.base().y());
    if !ptp.closer_to_center_than(&position, width)
        && (!(y > f64::from(ptp.y())) || !BlockPos::containing(f64::from(ptp.x()), y, f64::from(ptp.z())).closer_to_center_than(&position, width))
    {
        let speed = this.data().speed_modifier;
        mob.set_wanted_position(f64::from(ptp.x()), f64::from(ptp.y()), f64::from(ptp.z()), speed);
    } else {
        this.data_mut().path_to_position = None;
    }
}

pub fn default_follow_the_path<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    let mob_pos = this.get_temp_mob_pos(mob, level);
    let width = mob.base().get_bb_width();
    this.data_mut().max_distance_to_waypoint = if width > 0.75 { width / 2.0 } else { 0.75 - width / 2.0 };
    let max_distance = f64::from(this.data().max_distance_to_waypoint);
    let path = this.data().path.clone().expect("following a path");
    let current_node_pos = path.get_next_node_pos();
    let b = mob.base();
    let x_distance = (b.x() - (f64::from(current_node_pos.x()) + 0.5)).abs();
    let y_distance = (b.y() - f64::from(current_node_pos.y())).abs();
    let z_distance = (b.z() - (f64::from(current_node_pos.z()) + 0.5)).abs();
    let close_enough = x_distance < max_distance && z_distance < max_distance && y_distance < f64::from(this.get_max_vertical_distance_to_waypoint());
    if close_enough || this.can_cut_corner(path.get_next_node().ty) && should_target_next_node_in_direction(this, mob, level, &path, mob_pos) {
        path.advance();
    }
    this.do_stuck_detection(mob, level, mob_pos);
}

fn should_target_next_node_in_direction<N: PathNavigation + ?Sized>(this: &N, mob: &dyn Entity, level: &ServerLevel, path: &Path, mob_position: Vec3) -> bool {
    if path.get_next_node_index() + 1 >= path.get_node_count() as i32 {
        return false;
    }
    let current_node = Vec3::at_bottom_center_of(&path.get_next_node_pos());
    if !mob_position.closer_than(current_node, 2.0) {
        return false;
    }
    if this.can_move_directly(mob, level, mob_position, path.get_next_entity_pos(mob.base().get_bb_width())) {
        return true;
    }
    let next_node = Vec3::at_bottom_center_of(&path.get_node_pos(path.get_next_node_index() as usize + 1));
    let mob_to_current = current_node.subtract(mob_position);
    let mob_to_next = next_node.subtract(mob_position);
    let mob_to_current_sqr = mob_to_current.length_sqr();
    let mob_to_next_sqr = mob_to_next.length_sqr();
    let closer_to_next_than_current = mob_to_next_sqr < mob_to_current_sqr;
    let within_current_block = mob_to_current_sqr < 0.5;
    if !closer_to_next_than_current && !within_current_block {
        return false;
    }
    let mob_direction = mob_to_current.normalize();
    let path_direction = mob_to_next.normalize();
    path_direction.dot(mob_direction) < 0.0
}

pub fn default_do_stuck_detection<N: PathNavigation + ?Sized>(this: &mut N, mob: &dyn Entity, level: &ServerLevel, mob_pos: Vec3) {
    let speed = mob.get_speed();
    if this.data().tick - this.data().last_stuck_check > STUCK_CHECK_INTERVAL {
        let effective_speed = if speed >= 1.0 { speed } else { speed * speed };
        let threshold_distance = effective_speed * 100.0 * STUCK_THRESHOLD_DISTANCE_FACTOR;
        if mob_pos.distance_to_sqr(this.data().last_stuck_check_pos) < f64::from(threshold_distance * threshold_distance) {
            this.data_mut().is_stuck = true;
            this.stop();
        } else {
            this.data_mut().is_stuck = false;
        }
        let d = this.data_mut();
        d.last_stuck_check = d.tick;
        d.last_stuck_check_pos = mob_pos;
    }
    if let Some(path) = this.data().path.clone()
        && !path.is_done()
    {
        let pos = path.get_next_node_pos();
        let time = level.game_time();
        let d = this.data_mut();
        if pos == d.timeout_cached_node {
            d.timeout_timer += time - d.last_timeout_check;
        } else {
            d.timeout_cached_node = pos;
            let dist_to_node = mob_pos.distance_to(Vec3::at_bottom_center_of(&d.timeout_cached_node));
            d.timeout_limit = if speed > 0.0 { dist_to_node / f64::from(speed) * 20.0 } else { 0.0 };
        }
        if d.timeout_limit > 0.0 && d.timeout_timer as f64 > d.timeout_limit * 3.0 {
            // timeoutPath()
            reset_stuck_timeout(d);
            this.stop();
        }
        this.data_mut().last_timeout_check = time;
    }
}

/// `resetStuckTimeout()`.
fn reset_stuck_timeout(d: &mut NavigationData) {
    d.timeout_cached_node = BlockPos::ZERO;
    d.timeout_timer = 0;
    d.timeout_limit = 0.0;
    d.is_stuck = false;
}

pub fn default_recompute_path<N: PathNavigation + ?Sized>(this: &mut N, mob: &mut dyn Entity, level: &ServerLevel) {
    if level.game_time() - this.data().time_last_recompute <= MAX_TIME_RECOMPUTE || !this.can_update_path(mob, level) {
        this.data_mut().has_delayed_recomputation = true;
    } else if let Some(target_pos) = this.data().target_pos {
        this.data_mut().path = None;
        let reach_range = this.data().reach_range;
        let path = this.create_path_pos(mob, level, target_pos, reach_range);
        let d = this.data_mut();
        d.path = path;
        d.time_last_recompute = level.game_time();
        d.has_delayed_recomputation = false;
    }
}

/// `Node.cloneAndMove`-based helpers are on [`Node`]; re-exported for override code.
pub type PathNode = Node;
/// Re-export for overrides that build their own mob view.
pub type MobView<'a> = EntityMob<'a>;
/// The `PathfindingMob` view trait (overrides of evaluators use it).
pub use crate::world::level::pathfinder::PathfindingMob as NavMob;
#[allow(unused_imports)]
use PathfindingMob as _;
