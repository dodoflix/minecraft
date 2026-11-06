//! `net.minecraft.world.level.pathfinder.NodeEvaluator` and the virtual-dispatch glue for the evaluator family
//! (`WalkNodeEvaluator` ⊃ `AmphibiousNodeEvaluator`, `FlyNodeEvaluator`; `SwimNodeEvaluator`).
//!
//! Vanilla's class hierarchy is flattened: [`EvaluatorState`] holds the fields of every class and an
//! [`EvaluatorClass`] tag, and every overridable vanilla method is a [`NodeEvaluator`] trait method whose default
//! dispatches on the tag to the class implementation (`walk::*`, `swim::*`, `amphibious::*`, `fly::*`). A vanilla
//! *subclass defined by a mob* (`Frog.FrogNodeEvaluator extends AmphibiousNodeEvaluator`, `Creaking.HomeNodeEvaluator
//! extends WalkNodeEvaluator`) is a struct that embeds an [`EvaluatorState`] of the extended class, implements the trait
//! with `state()`/`state_mut()` and overrides what vanilla overrides; `super.method(..)` is the `default_*` function of
//! the same name here. Because every internal call goes through the trait (`this.get_path_type(..)`), overrides are
//! seen by the evaluator's own logic exactly like Java's virtual dispatch.
//!
//! Mob and level are not fields (vanilla keeps `mob`/`currentContext` between `prepare` and `done`): they travel in
//! the [`PathEnv`] passed to every method.

use super::mob_view::PathfindingMob;
use super::node::{Node, NodeId, Target};
use super::path_type::PathType;
use super::pathfinding_context::PathfindingContext;
use super::{
    amphibious_node_evaluator as amphibious, fly_node_evaluator as fly,
    swim_node_evaluator as swim, walk_node_evaluator as walk,
};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use std::collections::HashMap;

/// The mob + context of one `prepare(..)`..`done()` span (vanilla's `mob` and `currentContext` fields).
pub struct PathEnv<'a> {
    pub mob: &'a mut dyn PathfindingMob,
    pub context: PathfindingContext<'a>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvaluatorClass {
    Walk,
    Swim,
    Amphibious,
    Fly,
}

/// The `NodeEvaluator` fields.
pub struct NodeEvaluatorBase {
    /// `nodes` (`Int2ObjectMap<Node>` keyed by `Node.createHash`): the pool; ids index it.
    pub nodes: Vec<Node>,
    by_hash: HashMap<i32, NodeId>,
    pub entity_width: i32,
    pub entity_height: i32,
    pub entity_depth: i32,
    pub can_pass_doors: bool,
    pub can_open_doors: bool,
    pub can_float: bool,
    pub can_walk_over_fences: bool,
}

impl Default for NodeEvaluatorBase {
    fn default() -> Self {
        NodeEvaluatorBase {
            nodes: Vec::new(),
            by_hash: HashMap::new(),
            entity_width: 0,
            entity_height: 0,
            entity_depth: 0,
            can_pass_doors: true,
            can_open_doors: false,
            can_float: false,
            can_walk_over_fences: false,
        }
    }
}

impl NodeEvaluatorBase {
    /// `getNode(x, y, z)`: the pooled node of the position (keyed by `createHash`, so far-apart positions may alias).
    pub fn get_node(&mut self, x: i32, y: i32, z: i32) -> NodeId {
        let hash = Node::create_hash(x, y, z);
        if let Some(&id) = self.by_hash.get(&hash) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Node::new(x, y, z));
        self.by_hash.insert(hash, id);
        id
    }

    pub fn get_node_pos(&mut self, pos: BlockPos) -> NodeId {
        self.get_node(pos.x(), pos.y(), pos.z())
    }

    /// `getTargetNodeAt(x, y, z)`.
    pub fn get_target_node_at(&mut self, x: f64, y: f64, z: f64) -> Target {
        let id = self.get_node(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z));
        Target::new(&self.nodes[id])
    }

    /// `prepare` of the base class (minus the context, which the caller built): resets the pool and the entity size.
    pub fn prepare(&mut self, mob: &dyn PathfindingMob) {
        self.nodes.clear();
        self.by_hash.clear();
        self.entity_width = mth::floor_f32(mob.bb_width() + 1.0);
        self.entity_height = mth::floor_f32(mob.bb_height() + 1.0);
        self.entity_depth = mth::floor_f32(mob.bb_width() + 1.0);
    }
}

/// Key of `Object2BooleanMap<AABB>` (`AABB.equals` compares the six doubles).
pub type AabbKey = [u64; 6];

/// All evaluator fields (see the module docs).
pub struct EvaluatorState {
    pub base: NodeEvaluatorBase,
    pub class: EvaluatorClass,
    /// `pathTypesByPosCacheByMob` (walk) / `pathTypeByPosCache` (fly) / `pathTypesByPosCache` (swim).
    pub path_types_by_pos: HashMap<i64, PathType>,
    /// `collisionCache` (walk).
    pub collision_cache: HashMap<AabbKey, bool>,
    /// `reusableNeighbors` (walk), indexed by `get2DDataValue`.
    pub reusable_neighbors: [Option<NodeId>; 4],
    /// `SwimNodeEvaluator.allowBreaching`.
    pub allow_breaching: bool,
    /// `AmphibiousNodeEvaluator.prefersShallowSwimming` / `oldWalkableCost` / `oldWaterBorderCost`.
    pub prefers_shallow_swimming: bool,
    pub old_walkable_cost: f32,
    pub old_water_border_cost: f32,
}

impl EvaluatorState {
    fn new(class: EvaluatorClass) -> Self {
        EvaluatorState {
            base: NodeEvaluatorBase::default(),
            class,
            path_types_by_pos: HashMap::new(),
            collision_cache: HashMap::new(),
            reusable_neighbors: [None; 4],
            allow_breaching: false,
            prefers_shallow_swimming: false,
            old_walkable_cost: 0.0,
            old_water_border_cost: 0.0,
        }
    }

    /// `new WalkNodeEvaluator()`.
    pub fn walk() -> Self {
        Self::new(EvaluatorClass::Walk)
    }

    /// `new SwimNodeEvaluator(allowBreaching)`.
    pub fn swim(allow_breaching: bool) -> Self {
        EvaluatorState {
            allow_breaching,
            ..Self::new(EvaluatorClass::Swim)
        }
    }

    /// `new AmphibiousNodeEvaluator(prefersShallowSwimming)`.
    pub fn amphibious(prefers_shallow_swimming: bool) -> Self {
        EvaluatorState {
            prefers_shallow_swimming,
            ..Self::new(EvaluatorClass::Amphibious)
        }
    }

    /// `new FlyNodeEvaluator()`.
    pub fn fly() -> Self {
        Self::new(EvaluatorClass::Fly)
    }
}

/// `NodeEvaluator` (+ the protected members of `WalkNodeEvaluator` that subclasses call or override).
pub trait NodeEvaluator {
    fn state(&self) -> &EvaluatorState;
    fn state_mut(&mut self) -> &mut EvaluatorState;

    /// `prepare(level, entity)` (the context is already in `env`).
    fn prepare(&mut self, env: &mut PathEnv<'_>) {
        default_prepare(self, env)
    }
    /// `done()`.
    fn done(&mut self, env: &mut PathEnv<'_>) {
        default_done(self, env)
    }
    /// `getStart()`.
    fn get_start(&mut self, env: &mut PathEnv<'_>) -> NodeId {
        default_get_start(self, env)
    }
    /// `getTarget(x, y, z)`.
    fn get_target(&mut self, _env: &mut PathEnv<'_>, x: f64, y: f64, z: f64) -> Target {
        default_get_target(self, x, y, z)
    }
    /// `getNeighbors(neighbors, pos)`: fills `neighbors`, returns how many.
    fn get_neighbors(
        &mut self,
        env: &mut PathEnv<'_>,
        neighbors: &mut [NodeId; 32],
        pos: NodeId,
    ) -> usize {
        default_get_neighbors(self, env, neighbors, pos)
    }
    /// `getPathTypeOfMob(context, x, y, z, mob)`.
    fn get_path_type_of_mob(&mut self, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
        default_get_path_type_of_mob(self, env, x, y, z)
    }
    /// `getPathType(context, x, y, z)`.
    fn get_path_type(&mut self, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
        default_get_path_type(self, env, x, y, z)
    }

    // ---- WalkNodeEvaluator protected members -----------------------------------------------------------------

    /// `getStartNode(pos)`.
    fn get_start_node(&mut self, env: &mut PathEnv<'_>, pos: BlockPos) -> NodeId {
        walk::get_start_node(self, env, pos)
    }
    /// `canStartAt(pos)`.
    fn can_start_at(&mut self, env: &mut PathEnv<'_>, pos: BlockPos) -> bool {
        default_can_start_at(self, env, pos)
    }
    /// `getCachedPathType(x, y, z)`.
    fn get_cached_path_type(&mut self, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
        walk::get_cached_path_type(self, env, x, y, z)
    }
    /// `isAmphibious()`.
    fn is_amphibious(&self) -> bool {
        self.state().class == EvaluatorClass::Amphibious
    }
    /// `getFloorLevel(pos)`.
    fn get_floor_level(&mut self, env: &mut PathEnv<'_>, pos: BlockPos) -> f64 {
        walk::get_floor_level(self, env, pos)
    }
    /// `findAcceptedNode(x, y, z, jumpSize, nodeHeight, travelDirection, blockPathTypeCurrent)`.
    fn find_accepted_node(
        &mut self,
        env: &mut PathEnv<'_>,
        x: i32,
        y: i32,
        z: i32,
        jump_size: i32,
        node_height: f64,
        travel_direction: Direction,
        block_path_type_current: PathType,
    ) -> Option<NodeId> {
        walk::find_accepted_node(
            self,
            env,
            x,
            y,
            z,
            jump_size,
            node_height,
            travel_direction,
            block_path_type_current,
        )
    }
    /// `isNeighborValid(neighbor, current)`.
    fn is_neighbor_valid(
        &mut self,
        env: &mut PathEnv<'_>,
        neighbor: Option<NodeId>,
        current: NodeId,
    ) -> bool {
        walk::is_neighbor_valid(self, env, neighbor, current)
    }
    /// `isDiagonalValid(pos, ew, ns)`.
    fn is_diagonal_valid_between(
        &mut self,
        env: &mut PathEnv<'_>,
        pos: NodeId,
        ew: Option<NodeId>,
        ns: Option<NodeId>,
    ) -> bool {
        walk::is_diagonal_valid_between(self, env, pos, ew, ns)
    }
    /// `isDiagonalValid(diagonal)`.
    fn is_diagonal_valid(&mut self, diagonal: Option<NodeId>) -> bool {
        walk::is_diagonal_valid(self, diagonal)
    }

    // ---- setters/getters ---------------------------------------------------------------------------------------

    fn set_can_pass_doors(&mut self, can_pass_doors: bool) {
        self.state_mut().base.can_pass_doors = can_pass_doors;
    }
    fn set_can_open_doors(&mut self, can_open_doors: bool) {
        self.state_mut().base.can_open_doors = can_open_doors;
    }
    fn set_can_float(&mut self, can_float: bool) {
        self.state_mut().base.can_float = can_float;
    }
    fn set_can_walk_over_fences(&mut self, can_walk_over_fences: bool) {
        self.state_mut().base.can_walk_over_fences = can_walk_over_fences;
    }
    fn can_pass_doors(&self) -> bool {
        self.state().base.can_pass_doors
    }
    fn can_open_doors(&self) -> bool {
        self.state().base.can_open_doors
    }
    fn can_float(&self) -> bool {
        self.state().base.can_float
    }
    fn can_walk_over_fences(&self) -> bool {
        self.state().base.can_walk_over_fences
    }

    /// `getNode(x, y, z)`.
    fn get_node(&mut self, x: i32, y: i32, z: i32) -> NodeId {
        self.state_mut().base.get_node(x, y, z)
    }
    /// The node behind an id.
    fn node(&self, id: NodeId) -> &Node {
        &self.state().base.nodes[id]
    }
    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.state_mut().base.nodes[id]
    }
}

// ---- class dispatch ("super" implementations) -----------------------------------------------------------------

/// `NodeEvaluator.prepare` + the class's own `prepare`.
pub fn default_prepare<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    match this.state().class {
        EvaluatorClass::Walk => walk::prepare(this, env),
        EvaluatorClass::Swim => swim::prepare(this, env),
        EvaluatorClass::Amphibious => amphibious::prepare(this, env),
        EvaluatorClass::Fly => fly::prepare(this, env),
    }
}

pub fn default_done<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    match this.state().class {
        EvaluatorClass::Walk => walk::done(this, env),
        EvaluatorClass::Swim => swim::done(this, env),
        EvaluatorClass::Amphibious => amphibious::done(this, env),
        EvaluatorClass::Fly => fly::done(this, env),
    }
}

pub fn default_get_start<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) -> NodeId {
    match this.state().class {
        EvaluatorClass::Walk => walk::get_start(this, env),
        EvaluatorClass::Swim => swim::get_start(this, env),
        EvaluatorClass::Amphibious => amphibious::get_start(this, env),
        EvaluatorClass::Fly => fly::get_start(this, env),
    }
}

pub fn default_get_target<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    x: f64,
    y: f64,
    z: f64,
) -> Target {
    match this.state().class {
        EvaluatorClass::Amphibious => amphibious::get_target(this, x, y, z),
        _ => this.state_mut().base.get_target_node_at(x, y, z),
    }
}

pub fn default_get_neighbors<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    neighbors: &mut [NodeId; 32],
    pos: NodeId,
) -> usize {
    match this.state().class {
        EvaluatorClass::Walk => walk::get_neighbors(this, env, neighbors, pos),
        EvaluatorClass::Swim => swim::get_neighbors(this, env, neighbors, pos),
        EvaluatorClass::Amphibious => amphibious::get_neighbors(this, env, neighbors, pos),
        EvaluatorClass::Fly => fly::get_neighbors(this, env, neighbors, pos),
    }
}

pub fn default_get_path_type_of_mob<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> PathType {
    match this.state().class {
        EvaluatorClass::Swim => swim::get_path_type_of_mob(this, env, x, y, z),
        _ => walk::get_path_type_of_mob(this, env, x, y, z),
    }
}

pub fn default_get_path_type<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> PathType {
    match this.state().class {
        EvaluatorClass::Walk => walk::get_path_type(env, x, y, z),
        EvaluatorClass::Swim => this.get_path_type_of_mob(env, x, y, z),
        EvaluatorClass::Amphibious => amphibious::get_path_type(this, env, x, y, z),
        EvaluatorClass::Fly => fly::get_path_type(env, x, y, z),
    }
}

pub fn default_can_start_at<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: BlockPos,
) -> bool {
    match this.state().class {
        EvaluatorClass::Fly => fly::can_start_at(this, env, pos),
        _ => walk::can_start_at(this, env, pos),
    }
}

/// `NodeEvaluator.isBurningBlock(state)`.
pub fn is_burning_block(state: BlockState) -> bool {
    use crate::world::level::block::impls::campfire_block::CampfireBlock;
    use mc_data::block::Block;
    state.block().is_in_tag("fire")
        || state.block() == Block::LAVA
        || state.block() == Block::MAGMA_BLOCK
        || CampfireBlock::is_lit_campfire(state)
        || state.block() == Block::LAVA_CAULDRON
}
