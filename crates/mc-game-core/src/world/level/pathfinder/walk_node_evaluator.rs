//! `net.minecraft.world.level.pathfinder.WalkNodeEvaluator` (the class-`Walk` implementations dispatched from
//! [`NodeEvaluator`]; the amphibious and fly evaluators call into this module as their `super`).

use super::node::{Node, NodeId};
use super::node_evaluator::{EvaluatorState, NodeEvaluator, PathEnv, is_burning_block};
use super::path_type::PathType;
use super::pathfinding_context::{PathLevel, PathfindingContext};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState, PathComputationType};
use mc_data::props;

/// `SPACE_BETWEEN_WALL_POSTS`.
pub const SPACE_BETWEEN_WALL_POSTS: f64 = 0.5;
/// `DEFAULT_MOB_JUMP_HEIGHT`.
const DEFAULT_MOB_JUMP_HEIGHT: f64 = 1.125;

fn aabb_key(bb: &AABB) -> [u64; 6] {
    [
        bb.min_x.to_bits(),
        bb.min_y.to_bits(),
        bb.min_z.to_bits(),
        bb.max_x.to_bits(),
        bb.max_y.to_bits(),
        bb.max_z.to_bits(),
    ]
}

pub fn prepare<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    this.state_mut().base.prepare(&*env.mob);
    env.mob.on_pathfinding_start();
}

pub fn done<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    env.mob.on_pathfinding_done();
    let state: &mut EvaluatorState = this.state_mut();
    state.path_types_by_pos.clear();
    state.collision_cache.clear();
}

pub fn get_start<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) -> NodeId {
    let (mx, mz) = (env.mob.x(), env.mob.z());
    let mut start_y = env.mob.get_block_y();
    let mut block_state =
        env.context
            .get_block_state(BlockPos::containing(mx, f64::from(start_y), mz));
    if !env.mob.can_stand_on_fluid(block_state.get_fluid_state()) {
        if this.can_float() && env.mob.is_in_floatable_fluid() {
            while block_state.get_fluid_state().is_tag("entity_floatable") {
                start_y += 1;
                block_state =
                    env.context
                        .get_block_state(BlockPos::containing(mx, f64::from(start_y), mz));
            }
            start_y -= 1;
        } else if env.mob.on_ground() {
            start_y = mth::floor_f64(env.mob.y() + 0.5);
        } else {
            let mut reusable = BlockPos::containing(mx, env.mob.y() + 1.0, mz);
            while reusable.y() > env.context.level().get_min_y() {
                start_y = reusable.y();
                reusable = BlockPos::new(reusable.x(), reusable.y() - 1, reusable.z());
                let below = env.context.get_block_state(reusable);
                if !below.is_air() && !below.is_pathfindable_by(PathComputationType::Land) {
                    break;
                }
            }
        }
    } else {
        while env.mob.can_stand_on_fluid(block_state.get_fluid_state()) {
            start_y += 1;
            block_state =
                env.context
                    .get_block_state(BlockPos::containing(mx, f64::from(start_y), mz));
        }
        start_y -= 1;
    }
    let start_pos = env.mob.block_position();
    if !this.can_start_at(env, BlockPos::new(start_pos.x(), start_y, start_pos.z())) {
        let bb = env.mob.bounding_box();
        let y = f64::from(start_y);
        for (x, z) in [
            (bb.min_x, bb.min_z),
            (bb.min_x, bb.max_z),
            (bb.max_x, bb.min_z),
            (bb.max_x, bb.max_z),
        ] {
            let candidate = BlockPos::containing(x, y, z);
            if this.can_start_at(env, candidate) {
                return this.get_start_node(env, candidate);
            }
        }
    }
    this.get_start_node(env, BlockPos::new(start_pos.x(), start_y, start_pos.z()))
}

pub fn get_start_node<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: BlockPos,
) -> NodeId {
    let id = this.get_node(pos.x(), pos.y(), pos.z());
    let (x, y, z) = {
        let n = this.node(id);
        (n.x, n.y, n.z)
    };
    let ty = this.get_cached_path_type(env, x, y, z);
    let malus = env.mob.get_pathfinding_malus(ty);
    let node = this.node_mut(id);
    node.ty = ty;
    node.cost_malus = malus;
    id
}

pub fn can_start_at<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: BlockPos,
) -> bool {
    let ty = this.get_cached_path_type(env, pos.x(), pos.y(), pos.z());
    ty != PathType::Open && env.mob.get_pathfinding_malus(ty) >= 0.0
}

pub fn get_neighbors<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    neighbors: &mut [NodeId; 32],
    pos: NodeId,
) -> usize {
    let mut p = 0;
    let mut jump_size = 0;
    let (px, py, pz) = {
        let n = this.node(pos);
        (n.x, n.y, n.z)
    };
    let above = this.get_cached_path_type(env, px, py + 1, pz);
    let current = this.get_cached_path_type(env, px, py, pz);
    if env.mob.get_pathfinding_malus(above) >= 0.0 && current != PathType::StickyHoney {
        jump_size = mth::floor_f32(1.0f32.max(env.mob.max_up_step()));
    }
    let pos_height = this.get_floor_level(env, BlockPos::new(px, py, pz));
    for direction in Direction::HORIZONTAL {
        let node = this.find_accepted_node(
            env,
            px + direction.get_step_x(),
            py,
            pz + direction.get_step_z(),
            jump_size,
            pos_height,
            direction,
            current,
        );
        this.state_mut().reusable_neighbors[direction.get_2d_data_value() as usize] = node;
        if this.is_neighbor_valid(env, node, pos) {
            neighbors[p] = node.expect("valid neighbor");
            p += 1;
        }
    }
    for direction in Direction::HORIZONTAL {
        let second = direction.get_clockwise();
        let (ew, ns) = {
            let rn = &this.state().reusable_neighbors;
            (
                rn[direction.get_2d_data_value() as usize],
                rn[second.get_2d_data_value() as usize],
            )
        };
        if this.is_diagonal_valid_between(env, pos, ew, ns) {
            let diagonal = this.find_accepted_node(
                env,
                px + direction.get_step_x() + second.get_step_x(),
                py,
                pz + direction.get_step_z() + second.get_step_z(),
                jump_size,
                pos_height,
                direction,
                current,
            );
            if this.is_diagonal_valid(diagonal) {
                neighbors[p] = diagonal.expect("valid diagonal");
                p += 1;
            }
        }
    }
    p
}

pub fn is_neighbor_valid<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    _env: &mut PathEnv<'_>,
    neighbor: Option<NodeId>,
    current: NodeId,
) -> bool {
    match neighbor {
        None => false,
        Some(n) => {
            let (n, c) = (this.node(n), this.node(current));
            !n.closed && (n.cost_malus >= 0.0 || c.cost_malus < 0.0)
        }
    }
}

pub fn is_diagonal_valid_between<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: NodeId,
    ew: Option<NodeId>,
    ns: Option<NodeId>,
) -> bool {
    let (Some(ew), Some(ns)) = (ew, ns) else {
        return false;
    };
    let (pos, ew, ns) = (this.node(pos), this.node(ew), this.node(ns));
    if ns.y > pos.y || ew.y > pos.y {
        return false;
    }
    if ew.ty == PathType::WalkableDoor || ns.ty == PathType::WalkableDoor {
        return false;
    }
    if env.mob.bb_width() > 1.0 && (ew.cost_malus > 0.0 || ns.cost_malus > 0.0) {
        return false;
    }
    let can_pass_between_posts =
        ns.ty == PathType::Fence && ew.ty == PathType::Fence && f64::from(env.mob.bb_width()) < 0.5;
    (ns.y < pos.y || ns.cost_malus >= 0.0 || can_pass_between_posts)
        && (ew.y < pos.y || ew.cost_malus >= 0.0 || can_pass_between_posts)
}

pub fn is_diagonal_valid<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    diagonal: Option<NodeId>,
) -> bool {
    match diagonal {
        None => false,
        Some(d) => {
            let d = this.node(d);
            if d.closed {
                false
            } else if d.ty == PathType::WalkableDoor {
                false
            } else {
                d.cost_malus >= 0.0
            }
        }
    }
}

fn does_block_have_partial_collision(ty: PathType) -> bool {
    ty == PathType::Fence || ty == PathType::DoorWoodClosed || ty == PathType::DoorIronClosed
}

fn can_reach_without_collision<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos_to: NodeId,
) -> bool {
    let mut bb = env.mob.bounding_box();
    let to = this.node(pos_to);
    let mut delta = Vec3::new(
        f64::from(to.x) - env.mob.x() + bb.get_xsize() / 2.0,
        f64::from(to.y) - env.mob.y() + bb.get_ysize() / 2.0,
        f64::from(to.z) - env.mob.z() + bb.get_zsize() / 2.0,
    );
    let steps = mth::ceil_f64(delta.length() / bb.get_size());
    delta = delta.scale(f64::from(1.0f32 / steps as f32));
    for _ in 1..=steps {
        bb = bb.move_vec(delta);
        if has_collisions(this, env, &bb) {
            return false;
        }
    }
    true
}

pub fn get_floor_level<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: BlockPos,
) -> f64 {
    let level = env.context.level();
    if (this.can_float() || this.is_amphibious())
        && level.get_fluid_state(pos).is_tag("entity_floatable")
    {
        f64::from(pos.y()) + 0.5
    } else {
        get_floor_level_static(level, pos)
    }
}

/// `getFloorLevel(level, pos)`: the top of the collision shape below `pos`.
pub fn get_floor_level_static(level: &dyn BlockGetter, pos: BlockPos) -> f64 {
    let target = pos.below();
    let shape = level
        .get_block_state(target)
        .get_collision_shape(level, target);
    f64::from(target.y())
        + if shape.is_empty() {
            0.0
        } else {
            shape.max(mc_core::direction::Axis::Y)
        }
}

#[allow(clippy::too_many_arguments)]
pub fn find_accepted_node<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
    jump_size: i32,
    node_height: f64,
    travel_direction: Direction,
    block_path_type_current: PathType,
) -> Option<NodeId> {
    let mut best: Option<NodeId> = None;
    let max_y_target = this.get_floor_level(env, BlockPos::new(x, y, z));
    if max_y_target - node_height > get_mob_jump_height(env) {
        return None;
    }
    let path_type = this.get_cached_path_type(env, x, y, z);
    let path_cost = env.mob.get_pathfinding_malus(path_type);
    if path_cost >= 0.0 {
        best = Some(get_node_and_update_cost_to_max(
            this, x, y, z, path_type, path_cost,
        ));
    }
    if does_block_have_partial_collision(block_path_type_current)
        && let Some(b) = best
        && this.node(b).cost_malus >= 0.0
        && !can_reach_without_collision(this, env, b)
    {
        best = None;
    }
    if path_type != PathType::Walkable && (!this.is_amphibious() || path_type != PathType::Water) {
        if best.is_none_or(|b| this.node(b).cost_malus < 0.0)
            && jump_size > 0
            && (path_type != PathType::Fence || this.can_walk_over_fences())
            && path_type != PathType::UnpassableRail
            && path_type != PathType::Trapdoor
            && path_type != PathType::PowderSnow
        {
            best = try_jump_on(
                this,
                env,
                x,
                y,
                z,
                jump_size,
                node_height,
                travel_direction,
                block_path_type_current,
            );
        } else if !this.is_amphibious() && path_type == PathType::Water && !this.can_float() {
            best = try_find_first_non_water_below(this, env, x, y, z, best);
        } else if path_type == PathType::Open {
            best = Some(try_find_first_ground_node_below(this, env, x, y, z));
        } else if does_block_have_partial_collision(path_type) && best.is_none() {
            best = Some(get_closed_node(this, x, y, z, path_type));
        }
    }
    best
}

fn get_mob_jump_height(env: &PathEnv<'_>) -> f64 {
    DEFAULT_MOB_JUMP_HEIGHT.max(f64::from(env.mob.max_up_step()))
}

fn get_node_and_update_cost_to_max<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    x: i32,
    y: i32,
    z: i32,
    path_type: PathType,
    cost: f32,
) -> NodeId {
    let id = this.get_node(x, y, z);
    let node = this.node_mut(id);
    node.ty = path_type;
    node.cost_malus = node.cost_malus.max(cost);
    id
}

fn get_blocked_node<N: NodeEvaluator + ?Sized>(this: &mut N, x: i32, y: i32, z: i32) -> NodeId {
    let id = this.get_node(x, y, z);
    let node = this.node_mut(id);
    node.ty = PathType::Blocked;
    node.cost_malus = -1.0;
    id
}

fn get_closed_node<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    x: i32,
    y: i32,
    z: i32,
    path_type: PathType,
) -> NodeId {
    let id = this.get_node(x, y, z);
    let node = this.node_mut(id);
    node.closed = true;
    node.ty = path_type;
    node.cost_malus = path_type.get_malus();
    id
}

#[allow(clippy::too_many_arguments)]
fn try_jump_on<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
    jump_size: i32,
    node_height: f64,
    travel_direction: Direction,
    block_path_type_current: PathType,
) -> Option<NodeId> {
    let node_above = this.find_accepted_node(
        env,
        x,
        y + 1,
        z,
        jump_size - 1,
        node_height,
        travel_direction,
        block_path_type_current,
    )?;
    if env.mob.bb_width() >= 1.0 {
        return Some(node_above);
    }
    let above = this.node(node_above).clone();
    if above.ty != PathType::Open && above.ty != PathType::Walkable {
        return Some(node_above);
    }
    let center_x = f64::from(x - travel_direction.get_step_x()) + 0.5;
    let center_z = f64::from(z - travel_direction.get_step_z()) + 0.5;
    let half_width = f64::from(env.mob.bb_width()) / 2.0;
    let min_y = this.get_floor_level(
        env,
        BlockPos::containing(center_x, f64::from(y + 1), center_z),
    ) + 0.001;
    let max_y = f64::from(env.mob.bb_height())
        + this.get_floor_level(env, BlockPos::new(above.x, above.y, above.z))
        - 0.002;
    let grow = AABB::new(
        center_x - half_width,
        min_y,
        center_z - half_width,
        center_x + half_width,
        max_y,
        center_z + half_width,
    );
    if has_collisions(this, env, &grow) {
        None
    } else {
        Some(node_above)
    }
}

fn try_find_first_non_water_below<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    mut y: i32,
    z: i32,
    mut best: Option<NodeId>,
) -> Option<NodeId> {
    y -= 1;
    while y > env.context.level().get_min_y() {
        let local = this.get_cached_path_type(env, x, y, z);
        if local != PathType::Water {
            return best;
        }
        let malus = env.mob.get_pathfinding_malus(local);
        best = Some(get_node_and_update_cost_to_max(this, x, y, z, local, malus));
        y -= 1;
    }
    best
}

fn try_find_first_ground_node_below<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> NodeId {
    let min_y = env.context.level().get_min_y();
    let mut current_y = y - 1;
    while current_y >= min_y {
        if y - current_y > env.mob.get_max_fall_distance() {
            return get_blocked_node(this, x, current_y, z);
        }
        let path_type = this.get_cached_path_type(env, x, current_y, z);
        let path_cost = env.mob.get_pathfinding_malus(path_type);
        if path_type != PathType::Open {
            if path_cost >= 0.0 {
                return get_node_and_update_cost_to_max(
                    this, x, current_y, z, path_type, path_cost,
                );
            }
            return get_blocked_node(this, x, current_y, z);
        }
        current_y -= 1;
    }
    get_blocked_node(this, x, y, z)
}

fn has_collisions<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    aabb: &AABB,
) -> bool {
    let key = aabb_key(aabb);
    if let Some(&cached) = this.state().collision_cache.get(&key) {
        return cached;
    }
    let collides = !env.context.level().no_collision(&*env.mob, aabb);
    this.state_mut().collision_cache.insert(key, collides);
    collides
}

pub fn get_cached_path_type<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> PathType {
    let key = BlockPos::as_long_xyz(x, y, z);
    if let Some(&ty) = this.state().path_types_by_pos.get(&key) {
        return ty;
    }
    let ty = this.get_path_type_of_mob(env, x, y, z);
    this.state_mut().path_types_by_pos.insert(key, ty);
    ty
}

pub fn get_path_type_of_mob<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> PathType {
    let block_types = get_path_type_within_mob_bb(this, env, x, y, z);
    let count = block_types.iter().filter(|&&b| b).count();
    let contains = |ty: PathType| block_types[ty.ordinal()];
    if count == 1 {
        return PathType::ALL[block_types.iter().position(|&b| b).expect("one type")];
    }
    if contains(PathType::Fence) {
        return PathType::Fence;
    }
    if contains(PathType::UnpassableRail) {
        return PathType::UnpassableRail;
    }
    let mut highest_malus_type = PathType::Blocked;
    let mut highest_malus = env.mob.get_pathfinding_malus(highest_malus_type);
    for ty in PathType::ALL {
        if !contains(ty) {
            continue;
        }
        let malus = env.mob.get_pathfinding_malus(ty);
        if malus < 0.0 {
            return ty;
        }
        if malus >= highest_malus {
            highest_malus = malus;
            highest_malus_type = ty;
        }
    }
    let current_node_type = this.get_path_type(env, x, y, z);
    let is_large_mob = this.state().base.entity_width > 1;
    if is_large_mob {
        let is_current_cheaper = env.mob.get_pathfinding_malus(current_node_type) < highest_malus;
        let cap = is_current_cheaper
            && env
                .mob
                .get_pathfinding_malus(PathType::BigMobsCloseToDanger)
                < highest_malus;
        if cap {
            PathType::BigMobsCloseToDanger
        } else {
            highest_malus_type
        }
    } else if current_node_type == PathType::Open
        && highest_malus_type != PathType::Open
        && highest_malus == 0.0
    {
        PathType::Open
    } else {
        highest_malus_type
    }
}

/// `getPathTypeWithinMobBB(context, x, y, z)` as a set (indexed by `PathType::ordinal`).
pub fn get_path_type_within_mob_bb<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> [bool; PathType::COUNT] {
    let mut block_types = [false; PathType::COUNT];
    let (width, height, depth) = {
        let b = &this.state().base;
        (b.entity_width, b.entity_height, b.entity_depth)
    };
    for dx in 0..width {
        for dy in 0..height {
            for dz in 0..depth {
                let (xx, yy, zz) = (dx + x, dy + y, dz + z);
                let mut block_type = this.get_path_type(env, xx, yy, zz);
                let mob_position = env.mob.block_position();
                let can_pass_doors = this.can_pass_doors();
                if block_type == PathType::DoorWoodClosed && this.can_open_doors() && can_pass_doors
                {
                    block_type = PathType::WalkableDoor;
                }
                if block_type == PathType::DoorOpen && !can_pass_doors {
                    block_type = PathType::Blocked;
                }
                if block_type == PathType::Rail
                    && this.get_path_type(env, mob_position.x(), mob_position.y(), mob_position.z())
                        != PathType::Rail
                    && this.get_path_type(
                        env,
                        mob_position.x(),
                        mob_position.y() - 1,
                        mob_position.z(),
                    ) != PathType::Rail
                {
                    block_type = PathType::UnpassableRail;
                }
                block_types[block_type.ordinal()] = true;
            }
        }
    }
    block_types
}

/// `getPathType(context, x, y, z)` = `getPathTypeStatic(context, pos)`.
pub fn get_path_type(env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
    get_path_type_static(&env.context, BlockPos::new(x, y, z))
}

/// `getPathTypeStatic(context, pos)`.
pub fn get_path_type_static(context: &PathfindingContext<'_>, pos: BlockPos) -> PathType {
    let (x, y, z) = (pos.x(), pos.y(), pos.z());
    let block_path_type = context.get_path_type_from_state(x, y, z);
    if block_path_type == PathType::Open && y >= context.level().get_min_y() + 1 {
        match context.get_path_type_from_state(x, y - 1, z) {
            PathType::Open | PathType::Water | PathType::Lava | PathType::Walkable => {
                PathType::Open
            }
            PathType::Fire => PathType::Fire,
            PathType::Damaging => PathType::Damaging,
            PathType::StickyHoney => PathType::StickyHoney,
            PathType::PowderSnow => PathType::OnTopOfPowderSnow,
            PathType::DamageCautious => PathType::DamageCautious,
            PathType::Trapdoor => PathType::OnTopOfTrapdoor,
            _ => check_neighbour_blocks(context, x, y, z, PathType::Walkable),
        }
    } else {
        block_path_type
    }
}

/// `checkNeighbourBlocks(context, x, y, z, blockPathType)`.
pub fn check_neighbour_blocks(
    context: &PathfindingContext<'_>,
    x: i32,
    y: i32,
    z: i32,
    block_path_type: PathType,
) -> PathType {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx != 0 || dz != 0 {
                    let path_type = context.get_path_type_from_state(x + dx, y + dy, z + dz);
                    if path_type == PathType::Damaging {
                        return PathType::DamagingInNeighbor;
                    }
                    if path_type == PathType::Fire || path_type == PathType::Lava {
                        return PathType::FireInNeighbor;
                    }
                    if path_type == PathType::Water {
                        return PathType::WaterBorder;
                    }
                    if path_type == PathType::DamageCautious {
                        return PathType::DamageCautious;
                    }
                }
            }
        }
    }
    block_path_type
}

/// `getPathTypeFromState(level, pos)`: the classification of the block at `pos` alone.
pub fn get_path_type_from_state<L: BlockGetter + ?Sized>(level: &L, pos: BlockPos) -> PathType {
    let block_state = level.get_block_state(pos);
    let block = block_state.block();
    if block_state.is_air() {
        PathType::Open
    } else if block_state.is_tag("trapdoors")
        || block == Block::LILY_PAD
        || block == Block::BIG_DRIPLEAF
    {
        PathType::Trapdoor
    } else if block == Block::POWDER_SNOW {
        PathType::PowderSnow
    } else if block == Block::CACTUS || block == Block::SWEET_BERRY_BUSH {
        PathType::Damaging
    } else if block == Block::HONEY_BLOCK {
        PathType::StickyHoney
    } else if block == Block::COCOA {
        PathType::Cocoa
    } else if block == Block::WITHER_ROSE || block_state.is_tag("speleothems") {
        PathType::DamageCautious
    } else {
        let fluid_state = block_state.get_fluid_state();
        if fluid_state.is_tag("lava") {
            PathType::Lava
        } else if is_burning_block(block_state) {
            PathType::Fire
        } else if is_instance_of(block_state, "DoorBlock") {
            if block_state.get(props::OPEN) {
                PathType::DoorOpen
            } else if can_open_by_hand(block) {
                PathType::DoorWoodClosed
            } else {
                PathType::DoorIronClosed
            }
        } else if is_instance_of(block_state, "BaseRailBlock") {
            PathType::Rail
        } else if is_instance_of(block_state, "LeavesBlock") {
            PathType::Leaves
        } else if block_state.is_tag("fences")
            || block_state.is_tag("walls")
            || (is_instance_of(block_state, "FenceGateBlock") && !block_state.get(props::OPEN))
        {
            PathType::Fence
        } else if !block_state.is_pathfindable_by(PathComputationType::Land) {
            PathType::Blocked
        } else if fluid_state.is_tag("water") {
            PathType::Water
        } else {
            PathType::Open
        }
    }
}

fn is_instance_of(state: BlockState, class: &str) -> bool {
    crate::world::level::block::helpers::is_instance_of(state, class)
}

/// `door.type().canOpenByHand()`: every `BlockSetType` but iron (only the iron door has it).
fn can_open_by_hand(door: Block) -> bool {
    door != Block::IRON_DOOR
}
