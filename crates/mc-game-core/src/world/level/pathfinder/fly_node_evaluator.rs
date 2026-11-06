//! `net.minecraft.world.level.pathfinder.FlyNodeEvaluator` (class `Fly` of [`NodeEvaluator`]; extends the walk evaluator).

use super::node::NodeId;
use super::node_evaluator::{NodeEvaluator, PathEnv};
use super::path_type::PathType;
use super::walk_node_evaluator::{self as walk, check_neighbour_blocks};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::Block;

/// `SMALL_MOB_INFLATED_START_NODE_BOUNDING_BOX`.
const SMALL_MOB_INFLATED_START_NODE_BOUNDING_BOX: f32 = 1.1;
/// `MAX_START_NODE_CANDIDATES`.
const MAX_START_NODE_CANDIDATES: i32 = 10;

pub fn prepare<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    walk::prepare(this, env);
    this.state_mut().path_types_by_pos.clear();
    env.mob.on_pathfinding_start();
}

pub fn done<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    env.mob.on_pathfinding_done();
    this.state_mut().path_types_by_pos.clear();
    walk::done(this, env);
}

pub fn get_start<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) -> NodeId {
    let start_y;
    if this.can_float() && env.mob.is_in_water() {
        let mut y = env.mob.get_block_y();
        let (mx, mz) = (env.mob.x(), env.mob.z());
        let mut state = env
            .context
            .get_block_state(BlockPos::containing(mx, f64::from(y), mz));
        while state.block() == Block::WATER {
            y += 1;
            state = env
                .context
                .get_block_state(BlockPos::containing(mx, f64::from(y), mz));
        }
        start_y = y;
    } else {
        start_y = mth::floor_f64(env.mob.y() + 0.5);
    }
    let start_pos = BlockPos::containing(env.mob.x(), f64::from(start_y), env.mob.z());
    if !this.can_start_at(env, start_pos) {
        let bb = env.mob.bounding_box();
        let is_small_mob = bb.get_size() < 1.0;
        if !is_small_mob {
            let y = f64::from(env.mob.get_block_y());
            for (x, z) in [
                (bb.min_x, bb.min_z),
                (bb.min_x, bb.max_z),
                (bb.max_x, bb.min_z),
                (bb.max_x, bb.max_z),
            ] {
                let tested = BlockPos::containing(x, y, z);
                if this.can_start_at(env, tested) {
                    return walk::get_start_node(this, env, tested);
                }
            }
        } else {
            let inflate = f64::from(SMALL_MOB_INFLATED_START_NODE_BOUNDING_BOX);
            let z_padding = 0.0f64.max(inflate - bb.get_zsize());
            let x_padding = 0.0f64.max(inflate - bb.get_xsize());
            let y_padding = 0.0f64.max(inflate - bb.get_ysize());
            let inflated = bb.inflate(x_padding, y_padding, z_padding);
            let (min_x, min_y, min_z) = (
                mth::floor_f64(inflated.min_x),
                mth::floor_f64(inflated.min_y),
                mth::floor_f64(inflated.min_z),
            );
            let (max_x, max_y, max_z) = (
                mth::floor_f64(inflated.max_x),
                mth::floor_f64(inflated.max_y),
                mth::floor_f64(inflated.max_z),
            );
            let width = max_x.wrapping_sub(min_x).wrapping_add(1);
            let height = max_y.wrapping_sub(min_y).wrapping_add(1);
            let depth = max_z.wrapping_sub(min_z).wrapping_add(1);
            // `BlockPos.randomBetweenClosed`: lazily drawn, so a hit stops the RNG consumption.
            for _ in 0..MAX_START_NODE_CANDIDATES {
                let x = min_x.wrapping_add(env.mob.random().next_int_bounded(width));
                let y = min_y.wrapping_add(env.mob.random().next_int_bounded(height));
                let z = min_z.wrapping_add(env.mob.random().next_int_bounded(depth));
                let tested = BlockPos::new(x, y, z);
                if this.can_start_at(env, tested) {
                    return walk::get_start_node(this, env, tested);
                }
            }
        }
    }
    walk::get_start_node(this, env, start_pos)
}

pub fn can_start_at<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    pos: BlockPos,
) -> bool {
    let ty = this.get_cached_path_type(env, pos.x(), pos.y(), pos.z());
    env.mob.get_pathfinding_malus(ty) >= 0.0
}

/// The 26 neighbor offsets of `getNeighbors` in vanilla's evaluation order, with the indices (into this same table) of
/// the nodes that must also have a non-negative malus. Vanilla evaluates `findAcceptedNode` for every entry (its side
/// effects on the node pool are observable), then checks `isOpen(node)` and the dependencies.
const NEIGHBORS: [([i32; 3], &[usize]); 26] = [
    ([0, 0, 1], &[]),                       // 0 south
    ([-1, 0, 0], &[]),                      // 1 west
    ([1, 0, 0], &[]),                       // 2 east
    ([0, 0, -1], &[]),                      // 3 north
    ([0, 1, 0], &[]),                       // 4 up
    ([0, -1, 0], &[]),                      // 5 down
    ([0, 1, 1], &[0, 4]),                   // 6 southUp
    ([-1, 1, 0], &[1, 4]),                  // 7 westUp
    ([1, 1, 0], &[2, 4]),                   // 8 eastUp
    ([0, 1, -1], &[3, 4]),                  // 9 northUp
    ([0, -1, 1], &[0, 5]),                  // 10 southDown
    ([-1, -1, 0], &[1, 5]),                 // 11 westDown
    ([1, -1, 0], &[2, 5]),                  // 12 eastDown
    ([0, -1, -1], &[3, 5]),                 // 13 northDown
    ([1, 0, -1], &[3, 2]),                  // 14 northEast
    ([1, 0, 1], &[0, 2]),                   // 15 southEast
    ([-1, 0, -1], &[3, 1]),                 // 16 northWest
    ([-1, 0, 1], &[0, 1]),                  // 17 southWest
    ([1, 1, -1], &[14, 3, 2, 4, 9, 8]),     // 18 northEastUp
    ([1, 1, 1], &[15, 0, 2, 4, 6, 8]),      // 19 southEastUp
    ([-1, 1, -1], &[16, 3, 1, 4, 9, 7]),    // 20 northWestUp
    ([-1, 1, 1], &[17, 0, 1, 4, 6, 7]),     // 21 southWestUp
    ([1, -1, -1], &[14, 3, 2, 5, 13, 12]),  // 22 northEastDown
    ([1, -1, 1], &[15, 0, 2, 5, 10, 12]),   // 23 southEastDown
    ([-1, -1, -1], &[16, 3, 1, 5, 13, 11]), // 24 northWestDown
    ([-1, -1, 1], &[17, 0, 1, 5, 10, 11]),  // 25 southWestDown
];

pub fn get_neighbors<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    neighbors: &mut [NodeId; 32],
    pos: NodeId,
) -> usize {
    let (px, py, pz) = {
        let n = this.node(pos);
        (n.x, n.y, n.z)
    };
    let mut count = 0;
    let mut found: [Option<NodeId>; 26] = [None; 26];
    for (i, ([dx, dy, dz], deps)) in NEIGHBORS.iter().enumerate() {
        let node = find_accepted_node(this, env, px + dx, py + dy, pz + dz);
        found[i] = node;
        let open = node.is_some_and(|n| !this.node(n).closed);
        // the first six (direct neighbors) have no dependency; the others need every dependency's malus >= 0
        if open
            && deps
                .iter()
                .all(|&d| found[d].is_some_and(|n| this.node(n).cost_malus >= 0.0))
        {
            neighbors[count] = node.expect("open node");
            count += 1;
        }
    }
    count
}

fn find_accepted_node<N: NodeEvaluator + ?Sized>(
    this: &mut N,
    env: &mut PathEnv<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> Option<NodeId> {
    let path_type = this.get_cached_path_type(env, x, y, z);
    let path_cost = env.mob.get_pathfinding_malus(path_type);
    if path_cost >= 0.0 {
        let id = this.get_node(x, y, z);
        let node = this.node_mut(id);
        node.ty = path_type;
        node.cost_malus = node.cost_malus.max(path_cost);
        if path_type == PathType::Walkable {
            node.cost_malus += 1.0;
        }
        Some(id)
    } else {
        None
    }
}

pub fn get_path_type(env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
    let mut block_path_type = env.context.get_path_type_from_state(x, y, z);
    if block_path_type == PathType::Open && y > env.context.level().get_min_y() {
        let below_pos = BlockPos::new(x, y - 1, z);
        let below_type =
            env.context
                .get_path_type_from_state(below_pos.x(), below_pos.y(), below_pos.z());
        if below_type == PathType::Fire || below_type == PathType::Lava {
            block_path_type = PathType::Fire;
        } else if below_type == PathType::Damaging {
            block_path_type = PathType::Damaging;
        } else if below_type == PathType::Cocoa {
            block_path_type = PathType::Cocoa;
        } else if below_type == PathType::Fence {
            if below_pos != env.context.mob_position() {
                block_path_type = PathType::Fence;
            }
        } else {
            block_path_type = if below_type != PathType::Walkable
                && below_type != PathType::Open
                && below_type != PathType::Water
            {
                PathType::Walkable
            } else {
                PathType::Open
            };
        }
    }
    if block_path_type == PathType::Walkable || block_path_type == PathType::Open {
        block_path_type = check_neighbour_blocks(&env.context, x, y, z, block_path_type);
    }
    block_path_type
}
