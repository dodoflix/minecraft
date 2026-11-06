//! `net.minecraft.world.level.pathfinder.SwimNodeEvaluator` (class `Swim` of [`NodeEvaluator`]).

use super::node::NodeId;
use super::node_evaluator::{NodeEvaluator, PathEnv};
use super::path_type::PathType;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::material::FluidStateExt;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::PathComputationType;

pub fn prepare<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    this.state_mut().base.prepare(&*env.mob);
    this.state_mut().path_types_by_pos.clear();
}

pub fn done<N: NodeEvaluator + ?Sized>(this: &mut N, _env: &mut PathEnv<'_>) {
    this.state_mut().path_types_by_pos.clear();
}

pub fn get_start<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) -> NodeId {
    let bb = env.mob.bounding_box();
    this.get_node(
        mth::floor_f64(bb.min_x),
        mth::floor_f64(bb.min_y + 0.5),
        mth::floor_f64(bb.min_z),
    )
}

pub fn get_neighbors<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    neighbors: &mut [NodeId; 32],
    pos: NodeId,
) -> usize {
    let mut count = 0;
    let (px, py, pz) = {
        let n = this.node(pos);
        (n.x, n.y, n.z)
    };
    // `Maps.newEnumMap(Direction.class)`: indexed by `Direction::values()` order (ordinal).
    let mut nodes: [Option<NodeId>; 6] = [None; 6];
    for direction in Direction::ALL {
        let node = find_accepted_node(
            this,
            env,
            px + direction.get_step_x(),
            py + direction.get_step_y(),
            pz + direction.get_step_z(),
        );
        nodes[direction as usize] = node;
        if is_node_valid(this, node) {
            neighbors[count] = node.expect("valid node");
            count += 1;
        }
    }
    for direction in Direction::HORIZONTAL {
        let second = direction.get_clockwise();
        if has_malus(this, nodes[direction as usize]) && has_malus(this, nodes[second as usize]) {
            let diagonal = find_accepted_node(
                this,
                env,
                px + direction.get_step_x() + second.get_step_x(),
                py,
                pz + direction.get_step_z() + second.get_step_z(),
            );
            if is_node_valid(this, diagonal) {
                neighbors[count] = diagonal.expect("valid node");
                count += 1;
            }
        }
    }
    count
}

fn is_node_valid<N: NodeEvaluator + ?Sized>(this: &N, node: Option<NodeId>) -> bool {
    node.is_some_and(|n| !this.node(n).closed)
}

fn has_malus<N: NodeEvaluator + ?Sized>(this: &N, node: Option<NodeId>) -> bool {
    node.is_some_and(|n| this.node(n).cost_malus >= 0.0)
}

fn find_accepted_node<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> Option<NodeId> {
    let mut best = None;
    let path_type = get_cached_block_type(this, env, x, y, z);
    if this.state().allow_breaching && path_type == PathType::Breach || path_type == PathType::Water
    {
        let path_cost = env.mob.get_pathfinding_malus(path_type);
        if path_cost >= 0.0 {
            let id = this.get_node(x, y, z);
            let node = this.node_mut(id);
            node.ty = path_type;
            node.cost_malus = node.cost_malus.max(path_cost);
            if env
                .context
                .level()
                .get_fluid_state(BlockPos::new(x, y, z))
                .is_empty()
            {
                this.node_mut(id).cost_malus += 8.0;
            }
            best = Some(id);
        }
    }
    best
}

fn get_cached_block_type<N: NodeEvaluator + ?Sized>(
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
    let ty = this.get_path_type(env, x, y, z);
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
    let (width, height, depth) = {
        let b = &this.state().base;
        (b.entity_width, b.entity_height, b.entity_depth)
    };
    // a fresh `MutableBlockPos` is (0, 0, 0): only matters when the entity box is empty
    let mut pos = BlockPos::new(0, 0, 0);
    for xx in x..x + width {
        for yy in y..y + height {
            for zz in z..z + depth {
                pos = BlockPos::new(xx, yy, zz);
                let block_state = env.context.get_block_state(pos);
                let fluid_state = block_state.get_fluid_state();
                let below_state = env.context.get_block_state(pos.below());
                if fluid_state.is_empty()
                    && below_state.is_pathfindable_by(PathComputationType::Water)
                    && block_state.is_air()
                {
                    return PathType::Breach;
                }
                if !fluid_state.is_tag("water") {
                    return PathType::Blocked;
                }
            }
        }
    }
    if env
        .context
        .get_block_state(pos)
        .is_pathfindable_by(PathComputationType::Water)
    {
        PathType::Water
    } else {
        PathType::Blocked
    }
}
