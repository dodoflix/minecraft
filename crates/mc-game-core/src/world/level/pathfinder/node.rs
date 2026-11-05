//! `net.minecraft.world.level.pathfinder.Node` and `Target`.
//!
//! Vanilla links nodes by object reference (`cameFrom`, the open-set heap). Here every node lives in the node pool of
//! the [`NodeEvaluator`](super::node_evaluator::NodeEvaluator) (`Vec<Node>`) and is referred to by its [`NodeId`]
//! (index into that pool); `came_from` and the heap hold ids. Nodes copied into a [`Path`](super::path::Path) drop their
//! `came_from` (an id is meaningless outside the pool).

use super::path_type::PathType;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// Index of a [`Node`] in its evaluator's node pool.
pub type NodeId = usize;

#[derive(Clone, Debug)]
pub struct Node {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    hash: i32,
    pub heap_idx: i32,
    pub g: f32,
    pub h: f32,
    pub f: f32,
    pub came_from: Option<NodeId>,
    pub closed: bool,
    pub walked_distance: f32,
    pub cost_malus: f32,
    pub ty: PathType,
}

impl Node {
    pub fn new(x: i32, y: i32, z: i32) -> Node {
        Node {
            x,
            y,
            z,
            hash: Self::create_hash(x, y, z),
            heap_idx: -1,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            came_from: None,
            closed: false,
            walked_distance: 0.0,
            cost_malus: 0.0,
            ty: PathType::Blocked,
        }
    }

    /// `cloneAndMove(x, y, z)`.
    pub fn clone_and_move(&self, x: i32, y: i32, z: i32) -> Node {
        let mut node = Node::new(x, y, z);
        node.heap_idx = self.heap_idx;
        node.g = self.g;
        node.h = self.h;
        node.f = self.f;
        node.came_from = self.came_from;
        node.closed = self.closed;
        node.walked_distance = self.walked_distance;
        node.cost_malus = self.cost_malus;
        node.ty = self.ty;
        node
    }

    /// `createHash(x, y, z)` (not injective: also the key of the evaluator's node map).
    pub fn create_hash(x: i32, y: i32, z: i32) -> i32 {
        (y & 0xFF) | ((x & 32767) << 8) | ((z & 32767) << 24) | if x < 0 { i32::MIN } else { 0 } | if z < 0 { 32768 } else { 0 }
    }

    /// `hashCode()`.
    pub fn hash_code(&self) -> i32 {
        self.hash
    }

    pub fn distance_to(&self, to: &Node) -> f32 {
        let xd = to.x.wrapping_sub(self.x) as f32;
        let yd = to.y.wrapping_sub(self.y) as f32;
        let zd = to.z.wrapping_sub(self.z) as f32;
        mth::sqrt(xd * xd + yd * yd + zd * zd)
    }

    pub fn distance_to_xz(&self, to: &Node) -> f32 {
        let xd = to.x.wrapping_sub(self.x) as f32;
        let zd = to.z.wrapping_sub(self.z) as f32;
        mth::sqrt(xd * xd + zd * zd)
    }

    pub fn distance_to_pos(&self, pos: BlockPos) -> f32 {
        let xd = pos.x().wrapping_sub(self.x) as f32;
        let yd = pos.y().wrapping_sub(self.y) as f32;
        let zd = pos.z().wrapping_sub(self.z) as f32;
        mth::sqrt(xd * xd + yd * yd + zd * zd)
    }

    pub fn distance_to_sqr(&self, to: &Node) -> f32 {
        let xd = to.x.wrapping_sub(self.x) as f32;
        let yd = to.y.wrapping_sub(self.y) as f32;
        let zd = to.z.wrapping_sub(self.z) as f32;
        xd * xd + yd * yd + zd * zd
    }

    pub fn distance_to_sqr_pos(&self, pos: BlockPos) -> f32 {
        let xd = pos.x().wrapping_sub(self.x) as f32;
        let yd = pos.y().wrapping_sub(self.y) as f32;
        let zd = pos.z().wrapping_sub(self.z) as f32;
        xd * xd + yd * yd + zd * zd
    }

    pub fn distance_manhattan(&self, to: &Node) -> f32 {
        let xd = to.x.wrapping_sub(self.x).wrapping_abs() as f32;
        let yd = to.y.wrapping_sub(self.y).wrapping_abs() as f32;
        let zd = to.z.wrapping_sub(self.z).wrapping_abs() as f32;
        xd + yd + zd
    }

    pub fn distance_manhattan_pos(&self, pos: BlockPos) -> f32 {
        let xd = pos.x().wrapping_sub(self.x).wrapping_abs() as f32;
        let yd = pos.y().wrapping_sub(self.y).wrapping_abs() as f32;
        let zd = pos.z().wrapping_sub(self.z).wrapping_abs() as f32;
        xd + yd + zd
    }

    pub fn as_block_pos(&self) -> BlockPos {
        BlockPos::new(self.x, self.y, self.z)
    }

    pub fn as_vec3(&self) -> Vec3 {
        Vec3::new(f64::from(self.x), f64::from(self.y), f64::from(self.z))
    }

    /// `inOpenSet()`.
    pub fn in_open_set(&self) -> bool {
        self.heap_idx >= 0
    }
}

/// `Node.equals`: same hash and coordinates (a `Target` equals the `Node` at its position).
impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.hash == other.hash && self.x == other.x && self.y == other.y && self.z == other.z
    }
}

impl Eq for Node {}

/// `Target`: a goal node with the best node found so far (`updateBest`).
#[derive(Clone, Debug)]
pub struct Target {
    pub node: Node,
    best_heuristic: f32,
    best_node: Option<NodeId>,
    reached: bool,
}

impl Target {
    pub fn new(node: &Node) -> Target {
        Target::at(node.x, node.y, node.z)
    }

    pub fn at(x: i32, y: i32, z: i32) -> Target {
        Target { node: Node::new(x, y, z), best_heuristic: f32::MAX, best_node: None, reached: false }
    }

    /// `updateBest(heuristic, node)`.
    pub fn update_best(&mut self, heuristic: f32, node: NodeId) {
        if heuristic < self.best_heuristic {
            self.best_heuristic = heuristic;
            self.best_node = Some(node);
        }
    }

    pub fn get_best_node(&self) -> Option<NodeId> {
        self.best_node
    }

    pub fn set_reached(&mut self) {
        self.reached = true;
    }

    pub fn is_reached(&self) -> bool {
        self.reached
    }
}

impl std::ops::Deref for Target {
    type Target = Node;

    fn deref(&self) -> &Node {
        &self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_vanilla_formula() {
        assert_eq!(Node::create_hash(0, 0, 0), 0);
        assert_eq!(Node::create_hash(1, 2, 3), 2 | (1 << 8) | (3 << 24));
        assert_eq!(Node::create_hash(-1, 70, -1), 70 | (32767 << 8) | (32767 << 24) | i32::MIN | 32768);
    }

    #[test]
    fn distances_use_f32() {
        let a = Node::new(0, 0, 0);
        let b = Node::new(3, 4, 0);
        assert_eq!(a.distance_to(&b), 5.0);
        assert_eq!(a.distance_manhattan(&b), 7.0);
        assert_eq!(a.distance_to_xz(&b), 3.0);
    }
}
