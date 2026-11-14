//! The flight graph of the ender dragon: `EnderDragon.nodes`, `nodeAdjacency`, `findClosestNode` and the A* of `findPath`.
//!
//! Twenty-four nodes on three rings around the island (twelve at radius 60, eight at 40, four at 20), joined by a fixed adjacency
//! bitmask. Vanilla creates the nodes on the first query, from the heightmap at that time, and keeps them for the dragon's life.

use crate::server::level::ServerLevel;
use crate::world::level::pathfinder::binary_heap::BinaryHeap;
use crate::world::level::pathfinder::node::Node;
use crate::world::level::pathfinder::path::Path;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_world::heightmap::HeightmapType;

const NODE_COUNT: usize = 24;

/// `nodeAdjacency`.
const NODE_ADJACENCY: [i32; NODE_COUNT] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057,
    3166214, 2138120, 6373424, 4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712,
    8257536,
];

#[derive(Default)]
pub struct DragonNodes {
    /// `nodes`: empty until the first query (`nodes[0] == null`).
    nodes: Vec<Node>,
    /// `openSet`.
    open_set: BinaryHeap,
}

impl DragonNodes {
    /// The first half of `findClosestNode()`: creates the nodes.
    pub fn ensure_nodes(&mut self, level: &ServerLevel) {
        if !self.nodes.is_empty() {
            return;
        }
        for i in 0..NODE_COUNT as i32 {
            let mut y_adjustment = 5;
            let (node_x, node_z);
            if i < 12 {
                let angle = 2.0 * (-std::f32::consts::PI + 0.2617994_f32 * i as f32);
                node_x = mth::floor_f32(60.0 * mth::cos(f64::from(angle)));
                node_z = mth::floor_f32(60.0 * mth::sin(f64::from(angle)));
            } else if i < 20 {
                let multiplier = i - 12;
                let angle =
                    2.0 * (-std::f32::consts::PI + std::f32::consts::FRAC_PI_8 * multiplier as f32);
                node_x = mth::floor_f32(40.0 * mth::cos(f64::from(angle)));
                node_z = mth::floor_f32(40.0 * mth::sin(f64::from(angle)));
                y_adjustment += 10;
            } else {
                let multiplier = i - 20;
                let angle =
                    2.0 * (-std::f32::consts::PI + std::f32::consts::FRAC_PI_4 * multiplier as f32);
                node_x = mth::floor_f32(20.0 * mth::cos(f64::from(angle)));
                node_z = mth::floor_f32(20.0 * mth::sin(f64::from(angle)));
            }
            let height = level
                .get_heightmap_pos(
                    HeightmapType::MotionBlockingNoLeaves,
                    BlockPos::new(node_x, 0, node_z),
                )
                .y();
            let node_y = 73.max(height + y_adjustment);
            self.nodes.push(Node::new(node_x, node_y, node_z));
        }
    }

    /// `findClosestNode(tX, tY, tZ)`: the node closest to the position, among the island's twelve outer ones when no end crystal
    /// is left.
    pub fn find_closest_node(&self, x: f64, y: f64, z: f64, no_crystals: bool) -> i32 {
        let mut closest_dist = 10000.0_f32;
        let mut closest_index = 0;
        let current_pos = Node::new(mth::floor_f64(x), mth::floor_f64(y), mth::floor_f64(z));
        let start_index = if no_crystals { 12 } else { 0 };
        for i in start_index..NODE_COUNT {
            let dist = self.nodes[i].distance_to_sqr(&current_pos);
            if dist < closest_dist {
                closest_dist = dist;
                closest_index = i as i32;
            }
        }
        closest_index
    }

    /// `findPath(startIndex, endIndex, finalNode)`.
    pub fn find_path(
        &mut self,
        start_index: i32,
        end_index: i32,
        final_node: Option<Node>,
        no_crystals: bool,
    ) -> Option<Path> {
        self.nodes.truncate(NODE_COUNT);
        for node in &mut self.nodes {
            node.closed = false;
            node.f = 0.0;
            node.g = 0.0;
            node.h = 0.0;
            node.came_from = None;
            node.heap_idx = -1;
        }
        let from = start_index as usize;
        let mut to = end_index as usize;
        let to_node = self.nodes[to].clone();
        self.nodes[from].g = 0.0;
        self.nodes[from].h = self.nodes[from].distance_to(&to_node);
        self.nodes[from].f = self.nodes[from].h;
        self.open_set.clear();
        self.open_set.insert(&mut self.nodes, from);
        let mut closest = from;
        let minimum_node_index = if no_crystals { 12 } else { 0 };
        while self.open_set.size() > 0 {
            let open_node = self.open_set.pop(&mut self.nodes);
            if self.nodes[open_node] == to_node {
                if let Some(final_node) = final_node {
                    self.nodes.push(final_node);
                    let final_id = self.nodes.len() - 1;
                    self.nodes[final_id].came_from = Some(to);
                    to = final_id;
                }
                return Some(self.reconstruct_path(to));
            }
            if self.nodes[open_node].distance_to(&to_node)
                < self.nodes[closest].distance_to(&to_node)
            {
                closest = open_node;
            }
            self.nodes[open_node].closed = true;
            // the pool index of the node is its position in `nodes`
            let x_index = open_node;
            for ix in minimum_node_index..NODE_COUNT {
                if NODE_ADJACENCY[x_index] & (1 << ix) > 0 {
                    if self.nodes[ix].closed {
                        continue;
                    }
                    let tentative_g_score = self.nodes[open_node].g
                        + self.nodes[open_node].distance_to(&self.nodes[ix]);
                    if !self.nodes[ix].in_open_set() || tentative_g_score < self.nodes[ix].g {
                        self.nodes[ix].came_from = Some(open_node);
                        self.nodes[ix].g = tentative_g_score;
                        self.nodes[ix].h = self.nodes[ix].distance_to(&to_node);
                        if self.nodes[ix].in_open_set() {
                            let cost = self.nodes[ix].g + self.nodes[ix].h;
                            self.open_set.change_cost(&mut self.nodes, ix, cost);
                        } else {
                            self.nodes[ix].f = self.nodes[ix].g + self.nodes[ix].h;
                            self.open_set.insert(&mut self.nodes, ix);
                        }
                    }
                }
            }
        }
        if closest == from {
            None
        } else {
            if let Some(final_node) = final_node {
                self.nodes.push(final_node);
                let final_id = self.nodes.len() - 1;
                self.nodes[final_id].came_from = Some(closest);
                closest = final_id;
            }
            Some(self.reconstruct_path(closest))
        }
    }

    /// `reconstructPath(from, to)`.
    fn reconstruct_path(&self, to: usize) -> Path {
        let mut nodes = vec![self.nodes[to].clone()];
        let mut node = to;
        while let Some(came_from) = self.nodes[node].came_from {
            node = came_from;
            nodes.insert(0, self.nodes[node].clone());
        }
        let target = BlockPos::new(self.nodes[to].x, self.nodes[to].y, self.nodes[to].z);
        for node in &mut nodes {
            node.came_from = None;
        }
        Path::new(nodes, target, true)
    }
}
