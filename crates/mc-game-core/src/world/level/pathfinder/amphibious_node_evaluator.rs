//! `net.minecraft.world.level.pathfinder.AmphibiousNodeEvaluator` (class `Amphibious` of [`NodeEvaluator`]; extends the
//! walk evaluator).

use super::node::{NodeId, Target};
use super::node_evaluator::{NodeEvaluator, PathEnv};
use super::path_type::PathType;
use super::walk_node_evaluator as walk;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;

pub fn prepare<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    walk::prepare(this, env);
    env.mob.set_pathfinding_malus(PathType::Water, 0.0);
    let old_walkable = env.mob.get_pathfinding_malus(PathType::Walkable);
    this.state_mut().old_walkable_cost = old_walkable;
    env.mob.set_pathfinding_malus(PathType::Walkable, 6.0);
    let old_water_border = env.mob.get_pathfinding_malus(PathType::WaterBorder);
    this.state_mut().old_water_border_cost = old_water_border;
    env.mob.set_pathfinding_malus(PathType::WaterBorder, 4.0);
}

pub fn done<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) {
    let (walkable, water_border) = (this.state().old_walkable_cost, this.state().old_water_border_cost);
    env.mob.set_pathfinding_malus(PathType::Walkable, walkable);
    env.mob.set_pathfinding_malus(PathType::WaterBorder, water_border);
    walk::done(this, env);
}

pub fn get_start<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>) -> NodeId {
    if !env.mob.is_in_water() {
        walk::get_start(this, env)
    } else {
        let bb = env.mob.bounding_box();
        this.get_start_node(env, BlockPos::new(mth::floor_f64(bb.min_x), mth::floor_f64(bb.min_y + 0.5), mth::floor_f64(bb.min_z)))
    }
}

pub fn get_target<N: NodeEvaluator + ?Sized>(this: &mut N, x: f64, y: f64, z: f64) -> Target {
    this.state_mut().base.get_target_node_at(x, y + 0.5, z)
}

pub fn get_neighbors<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>, neighbors: &mut [NodeId; 32], pos: NodeId) -> usize {
    let mut num_valid = walk::get_neighbors(this, env, neighbors, pos);
    let (px, py, pz) = {
        let n = this.node(pos);
        (n.x, n.y, n.z)
    };
    let above = this.get_cached_path_type(env, px, py + 1, pz);
    let current = this.get_cached_path_type(env, px, py, pz);
    let jump_size = if env.mob.get_pathfinding_malus(above) >= 0.0 && current != PathType::StickyHoney {
        mth::floor_f32(1.0f32.max(env.mob.max_up_step()))
    } else {
        0
    };
    let pos_height = this.get_floor_level(env, BlockPos::new(px, py, pz));
    let up = this.find_accepted_node(env, px, py + 1, pz, 0.max(jump_size - 1), pos_height, Direction::Up, current);
    let down = this.find_accepted_node(env, px, py - 1, pz, jump_size, pos_height, Direction::Down, current);
    if is_vertical_neighbor_valid(this, env, up, pos) {
        neighbors[num_valid] = up.expect("valid neighbor");
        num_valid += 1;
    }
    if is_vertical_neighbor_valid(this, env, down, pos) && current != PathType::Trapdoor {
        neighbors[num_valid] = down.expect("valid neighbor");
        num_valid += 1;
    }
    let sea_level = env.context.level().sea_level();
    let prefers_shallow = this.state().prefers_shallow_swimming;
    for &id in &neighbors[..num_valid] {
        let node = this.node_mut(id);
        if node.ty == PathType::Water && prefers_shallow && node.y < sea_level - 10 {
            node.cost_malus += 1.0;
        }
    }
    num_valid
}

fn is_vertical_neighbor_valid<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>, vertical: Option<NodeId>, pos: NodeId) -> bool {
    this.is_neighbor_valid(env, vertical, pos) && vertical.is_some_and(|v| this.node(v).ty == PathType::Water)
}

pub fn get_path_type<N: NodeEvaluator + ?Sized>(this: &mut N, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
    let block_path_type = env.context.get_path_type_from_state(x, y, z);
    if block_path_type == PathType::Water {
        for direction in Direction::ALL {
            let p = BlockPos::new(x, y, z).relative(direction);
            if env.context.get_path_type_from_state(p.x(), p.y(), p.z()) == PathType::Blocked {
                return PathType::WaterBorder;
            }
        }
        PathType::Water
    } else {
        walk_get_path_type(this, env, x, y, z)
    }
}

/// `super.getPathType(context, x, y, z)` (walk).
fn walk_get_path_type<N: NodeEvaluator + ?Sized>(_this: &mut N, env: &mut PathEnv<'_>, x: i32, y: i32, z: i32) -> PathType {
    walk::get_path_type(env, x, y, z)
}
