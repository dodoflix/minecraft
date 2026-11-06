//! `net.minecraft.world.level.pathfinder.PathFinder`: A* over the nodes a [`NodeEvaluator`] produces.

use super::binary_heap::BinaryHeap;
use super::mob_view::PathfindingMob;
use super::node::{Node, NodeId, Target};
use super::node_evaluator::{NodeEvaluator, PathEnv};
use super::path::Path;
use super::pathfinding_context::PathfindingContext;
use crate::world::level::path_navigation_region::PathNavigationRegion;
use mc_core::pos::BlockPos;

/// `FUDGING`.
const FUDGING: f32 = 1.5;

/// `PathFinder` (`Warden`'s anonymous subclass overrides `distance` with the horizontal distance: [`PathFinder::with_distance`]).
pub struct PathFinder {
    neighbors: [NodeId; 32],
    max_visited_nodes: i32,
    node_evaluator: Box<dyn NodeEvaluator>,
    open_set: BinaryHeap,
    /// `captureDebug` (`ServerDebugSubscribers.hasAnySubscriberFor(ENTITY_PATHS)`), set by the owner.
    pub capture_debug: bool,
    distance: fn(&Node, &Node) -> f32,
}

impl PathFinder {
    /// `new PathFinder(nodeEvaluator, maxVisitedNodes)`.
    pub fn new(node_evaluator: Box<dyn NodeEvaluator>, max_visited_nodes: i32) -> PathFinder {
        PathFinder {
            neighbors: [0; 32],
            max_visited_nodes,
            node_evaluator,
            open_set: BinaryHeap::default(),
            capture_debug: false,
            distance: Node::distance_to,
        }
    }

    /// A path finder whose `distance(from, to)` is overridden.
    pub fn with_distance(mut self, distance: fn(&Node, &Node) -> f32) -> PathFinder {
        self.distance = distance;
        self
    }

    /// `setMaxVisitedNodes(n)`.
    pub fn set_max_visited_nodes(&mut self, max_visited_nodes: i32) {
        self.max_visited_nodes = max_visited_nodes;
    }

    pub fn node_evaluator(&self) -> &dyn NodeEvaluator {
        &*self.node_evaluator
    }

    pub fn node_evaluator_mut(&mut self) -> &mut dyn NodeEvaluator {
        &mut *self.node_evaluator
    }

    /// `findPath(level, entity, targets, maxPathLength, reachRange, maxVisitedNodesMultiplier)`; `targets` are in the
    /// iteration order of the vanilla `Set<BlockPos>` the caller passes.
    pub fn find_path(
        &mut self,
        level: &PathNavigationRegion<'_>,
        entity: &mut dyn PathfindingMob,
        targets: &[BlockPos],
        max_path_length: f32,
        reach_range: i32,
        max_visited_nodes_multiplier: f32,
    ) -> Option<Path> {
        self.open_set.clear();
        let context = PathfindingContext::new(level, Some(level.path_type_cache()), &*entity);
        let mut env = PathEnv {
            mob: entity,
            context,
        };
        self.node_evaluator.prepare(&mut env);
        let from = self.node_evaluator.get_start(&mut env);
        let target_nodes: Vec<(Target, BlockPos)> = targets
            .iter()
            .map(|p| {
                (
                    self.node_evaluator.get_target(
                        &mut env,
                        f64::from(p.x()),
                        f64::from(p.y()),
                        f64::from(p.z()),
                    ),
                    *p,
                )
            })
            .collect();
        let path = self.search(
            &mut env,
            from,
            target_nodes,
            max_path_length,
            reach_range,
            max_visited_nodes_multiplier,
        );
        self.node_evaluator.done(&mut env);
        path
    }

    fn search(
        &mut self,
        env: &mut PathEnv<'_>,
        from: NodeId,
        targets: Vec<(Target, BlockPos)>,
        max_path_length: f32,
        reach_range: i32,
        max_visited_nodes_multiplier: f32,
    ) -> Option<Path> {
        let (mut targets, positions): (Vec<Target>, Vec<BlockPos>) = targets.into_iter().unzip();
        // `Collectors.toMap(..)`: a `HashMap<Target, BlockPos>`; every loop below iterates it in hash order.
        let order = java_hash_iteration_order(
            &targets.iter().map(|t| t.hash_code()).collect::<Vec<_>>(),
            0,
        );
        {
            let nodes = &mut self.node_evaluator.state_mut().base.nodes;
            nodes[from].g = 0.0;
            let h = best_h(nodes, from, &mut targets, &order);
            nodes[from].h = h;
            nodes[from].f = h;
            self.open_set.clear();
            self.open_set.insert(nodes, from);
        }
        let capture_debug = self.capture_debug;
        let mut closed_set: Vec<NodeId> = Vec::new();
        let mut count = 0i32;
        // `reachedTargets`: insertion order (iteration order is derived at the end)
        let mut reached_targets: Vec<usize> = Vec::new();
        let max_visited_nodes_adjusted =
            (self.max_visited_nodes as f32 * max_visited_nodes_multiplier) as i32;
        while !self.open_set.is_empty() {
            count += 1;
            if count >= max_visited_nodes_adjusted {
                break;
            }
            let nodes = &mut self.node_evaluator.state_mut().base.nodes;
            let current = self.open_set.pop(nodes);
            nodes[current].closed = true;
            for &ti in &order {
                if nodes[current].distance_manhattan(&targets[ti]) <= reach_range as f32 {
                    targets[ti].set_reached();
                    if !reached_targets.contains(&ti) {
                        reached_targets.push(ti);
                    }
                }
            }
            if !reached_targets.is_empty() {
                break;
            }
            if capture_debug {
                closed_set.push(current);
            }
            if !(nodes[current].distance_to(&nodes[from]) >= max_path_length) {
                let neighbor_count =
                    self.node_evaluator
                        .get_neighbors(env, &mut self.neighbors, current);
                for i in 0..neighbor_count {
                    let neighbor = self.neighbors[i];
                    let nodes = &mut self.node_evaluator.state_mut().base.nodes;
                    let distance = (self.distance)(&nodes[current], &nodes[neighbor]);
                    nodes[neighbor].walked_distance = nodes[current].walked_distance + distance;
                    let tentative_g_score =
                        nodes[current].g + distance + nodes[neighbor].cost_malus;
                    if nodes[neighbor].walked_distance < max_path_length
                        && (!nodes[neighbor].in_open_set() || tentative_g_score < nodes[neighbor].g)
                    {
                        nodes[neighbor].came_from = Some(current);
                        nodes[neighbor].g = tentative_g_score;
                        let h = best_h(nodes, neighbor, &mut targets, &order) * FUDGING;
                        nodes[neighbor].h = h;
                        if nodes[neighbor].in_open_set() {
                            let new_cost = nodes[neighbor].g + nodes[neighbor].h;
                            self.open_set.change_cost(nodes, neighbor, new_cost);
                        } else {
                            nodes[neighbor].f = nodes[neighbor].g + nodes[neighbor].h;
                            self.open_set.insert(nodes, neighbor);
                        }
                    }
                }
            }
        }
        let nodes = &self.node_evaluator.state().base.nodes;
        let path = if !reached_targets.is_empty() {
            // `reachedTargets.stream().map(..).min(comparingInt(Path::getNodeCount))`: the first minimum wins
            let capacity = guava_hash_set_capacity(targets.len());
            let hashes: Vec<i32> = reached_targets
                .iter()
                .map(|&ti| targets[ti].hash_code())
                .collect();
            let iteration = java_hash_iteration_order(&hashes, capacity);
            iteration
                .into_iter()
                .map(|k| {
                    let ti = reached_targets[k];
                    reconstruct_path(nodes, targets[ti].get_best_node(), positions[ti], true)
                })
                .reduce(|a, b| {
                    if a.get_node_count() <= b.get_node_count() {
                        a
                    } else {
                        b
                    }
                })
        } else {
            order
                .iter()
                .map(|&ti| {
                    reconstruct_path(nodes, targets[ti].get_best_node(), positions[ti], false)
                })
                .reduce(|a, b| {
                    // `comparingDouble(getDistToTarget).thenComparingInt(getNodeCount)`
                    let ord = f64::from(a.get_dist_to_target())
                        .total_cmp(&f64::from(b.get_dist_to_target()))
                        .then(a.get_node_count().cmp(&b.get_node_count()));
                    if ord.is_le() { a } else { b }
                })
        };
        let path = path?;
        if capture_debug {
            let open: Vec<Node> = self
                .open_set
                .get_heap()
                .into_iter()
                .map(|id| nodes[id].clone())
                .collect();
            let closed: Vec<Node> = closed_set.into_iter().map(|id| nodes[id].clone()).collect();
            path.set_debug(open, closed, targets.clone());
        }
        Some(path)
    }
}

/// `getBestH(from, targets)`.
fn best_h(nodes: &[Node], from: NodeId, targets: &mut [Target], order: &[usize]) -> f32 {
    let mut best = f32::MAX;
    for &ti in order {
        let h = nodes[from].distance_to(&targets[ti]);
        targets[ti].update_best(h, from);
        best = h.min(best);
    }
    best
}

/// `reconstructPath(closest, target, reached)`: the chain of `cameFrom` links as a path (nodes are copied, without links).
fn reconstruct_path(
    nodes: &[Node],
    closest: Option<NodeId>,
    target: BlockPos,
    reached: bool,
) -> Path {
    let mut path_nodes = Vec::new();
    let mut cursor = closest;
    while let Some(id) = cursor {
        let mut node = nodes[id].clone();
        cursor = node.came_from;
        node.came_from = None;
        path_nodes.push(node);
    }
    path_nodes.reverse();
    Path::new(path_nodes, target, reached)
}

/// `Maps.capacity(expectedSize)` (Guava `Sets.newHashSetWithExpectedSize`).
fn guava_hash_set_capacity(expected: usize) -> usize {
    if expected < 3 {
        expected + 1
    } else {
        (expected as f32 / 0.75 + 1.0) as usize
    }
}

/// Iteration order (as indices into `hashes`, the insertion order) of a `java.util.HashMap`/`HashSet` whose keys have
/// these `hashCode()`s; `initial_capacity == 0` = the default capacity. Bins keep insertion order across resizes; bins
/// that would treeify (8+ colliding keys) are not modelled.
pub(crate) fn java_hash_iteration_order(hashes: &[i32], initial_capacity: usize) -> Vec<usize> {
    let mut capacity = if initial_capacity == 0 {
        16
    } else {
        initial_capacity.next_power_of_two()
    };
    let mut size = 0usize;
    for _ in hashes {
        size += 1;
        if size > capacity * 3 / 4 {
            capacity *= 2;
        }
    }
    let mut order: Vec<usize> = (0..hashes.len()).collect();
    let spread = |h: i32| (h ^ ((h as u32) >> 16) as i32) as u32 as usize & (capacity - 1);
    order.sort_by_key(|&i| spread(hashes[i]));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_order_buckets_then_insertion() {
        // capacity 16: bucket = spread(h) & 15
        assert_eq!(
            java_hash_iteration_order(&[5, 3, 21, 3], 0),
            vec![1, 3, 0, 2]
        ); // 3,3 | 5,21 (21&15 = 5)
        // 13 entries resize to 32
        let hashes: Vec<i32> = (0..13).rev().collect();
        let order = java_hash_iteration_order(&hashes, 0);
        assert_eq!(order.first(), Some(&12));
    }
}
