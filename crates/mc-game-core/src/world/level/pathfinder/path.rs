//! `net.minecraft.world.level.pathfinder.Path`.
//!
//! A `Path` is a *shared handle* (like a Java object reference): `clone()` aliases the same path, so a mob's brain
//! memory (`MemoryModuleType.PATH`) and its navigation observe each other's `advance()`/`truncateNodes`. Use
//! [`Path::copy`] for vanilla's `copy()` (a new path object with its own `nextNodeIndex`).

use super::node::{Node, Target};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use parking_lot::Mutex;
use std::sync::Arc;

/// `Path.DebugData`.
#[derive(Clone, Debug)]
pub struct DebugData {
    pub open_set: Vec<Node>,
    pub closed_set: Vec<Node>,
    pub target_nodes: Vec<Target>,
}

#[derive(Clone, Debug)]
struct PathData {
    nodes: Vec<Node>,
    debug_data: Option<Arc<DebugData>>,
    next_node_index: i32,
    target: BlockPos,
    dist_to_target: f32,
    reached: bool,
}

#[derive(Clone, Debug)]
pub struct Path(Arc<Mutex<PathData>>);

impl Path {
    /// `new Path(nodes, target, reached)`.
    pub fn new(nodes: Vec<Node>, target: BlockPos, reached: bool) -> Path {
        let dist_to_target = nodes
            .last()
            .map_or(f32::MAX, |n| n.distance_manhattan_pos(target));
        Path(Arc::new(Mutex::new(PathData {
            nodes,
            debug_data: None,
            next_node_index: 0,
            target,
            dist_to_target,
            reached,
        })))
    }

    /// `advance()`.
    pub fn advance(&self) {
        self.0.lock().next_node_index += 1;
    }

    /// `notStarted()`.
    pub fn not_started(&self) -> bool {
        self.0.lock().next_node_index <= 0
    }

    /// `isDone()`.
    pub fn is_done(&self) -> bool {
        let d = self.0.lock();
        d.next_node_index >= d.nodes.len() as i32
    }

    /// `getEndNode()`.
    pub fn get_end_node(&self) -> Option<Node> {
        self.0.lock().nodes.last().cloned()
    }

    /// `getNode(i)`.
    pub fn get_node(&self, i: usize) -> Node {
        self.0.lock().nodes[i].clone()
    }

    /// `truncateNodes(index)`.
    pub fn truncate_nodes(&self, index: usize) {
        let mut d = self.0.lock();
        if d.nodes.len() > index {
            d.nodes.truncate(index);
        }
    }

    /// `replaceNode(index, replaceWith)`.
    pub fn replace_node(&self, index: usize, replace_with: Node) {
        self.0.lock().nodes[index] = replace_with;
    }

    /// `getNodeCount()`.
    pub fn get_node_count(&self) -> usize {
        self.0.lock().nodes.len()
    }

    /// `getNextNodeIndex()`.
    pub fn get_next_node_index(&self) -> i32 {
        self.0.lock().next_node_index
    }

    /// `setNextNodeIndex(index)`.
    pub fn set_next_node_index(&self, next_node_index: i32) {
        self.0.lock().next_node_index = next_node_index;
    }

    /// `getEntityPosAtNode(entity, index)` for an entity of width `bb_width` (`getBbWidth()`).
    pub fn get_entity_pos_at_node(&self, bb_width: f32, index: usize) -> Vec3 {
        let node = self.get_node(index);
        let half = f64::from((bb_width + 1.0) as i32) * 0.5;
        Vec3::new(
            f64::from(node.x) + half,
            f64::from(node.y),
            f64::from(node.z) + half,
        )
    }

    /// `getNodePos(index)`.
    pub fn get_node_pos(&self, index: usize) -> BlockPos {
        self.0.lock().nodes[index].as_block_pos()
    }

    /// `getNextEntityPos(entity)`.
    pub fn get_next_entity_pos(&self, bb_width: f32) -> Vec3 {
        let next = self.get_next_node_index();
        self.get_entity_pos_at_node(bb_width, next as usize)
    }

    /// `getNextNodePos()`.
    pub fn get_next_node_pos(&self) -> BlockPos {
        let d = self.0.lock();
        d.nodes[d.next_node_index as usize].as_block_pos()
    }

    /// `getNextNode()`.
    pub fn get_next_node(&self) -> Node {
        let d = self.0.lock();
        d.nodes[d.next_node_index as usize].clone()
    }

    /// `getPreviousNode()`.
    pub fn get_previous_node(&self) -> Option<Node> {
        let d = self.0.lock();
        (d.next_node_index > 0).then(|| d.nodes[d.next_node_index as usize - 1].clone())
    }

    /// `sameAs(path)`: the same node list (`List.equals`).
    pub fn same_as(&self, path: Option<&Path>) -> bool {
        match path {
            None => false,
            Some(other) => {
                if Arc::ptr_eq(&self.0, &other.0) {
                    return true;
                }
                let (a, b) = (self.0.lock(), other.0.lock());
                a.nodes == b.nodes
            }
        }
    }

    /// Identity (`==`) of two path handles.
    pub fn ptr_eq(&self, other: &Path) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// `canReach()`.
    pub fn can_reach(&self) -> bool {
        self.0.lock().reached
    }

    /// `setDebug(openSet, closedSet, targets)`.
    pub fn set_debug(&self, open_set: Vec<Node>, closed_set: Vec<Node>, targets: Vec<Target>) {
        self.0.lock().debug_data = Some(Arc::new(DebugData {
            open_set,
            closed_set,
            target_nodes: targets,
        }));
    }

    /// `debugData()`.
    pub fn debug_data(&self) -> Option<Arc<DebugData>> {
        self.0.lock().debug_data.clone()
    }

    /// `getTarget()`.
    pub fn get_target(&self) -> BlockPos {
        self.0.lock().target
    }

    /// `getDistToTarget()`.
    pub fn get_dist_to_target(&self) -> f32 {
        self.0.lock().dist_to_target
    }

    /// `copy()`.
    pub fn copy(&self) -> Path {
        let d = self.0.lock();
        let mut result = Path::new(d.nodes.clone(), d.target, d.reached);
        let inner = Arc::get_mut(&mut result.0).expect("fresh path").get_mut();
        inner.debug_data = d.debug_data.clone();
        inner.next_node_index = d.next_node_index;
        result
    }

    /// A snapshot of every node (the node list is otherwise only reachable under the lock).
    pub fn nodes(&self) -> Vec<Node> {
        self.0.lock().nodes.clone()
    }
}

/// `Path.equals`: same progress, debug data (by identity), reachability, target and node list.
impl PartialEq for Path {
    fn eq(&self, other: &Path) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        let (a, b) = (self.0.lock(), other.0.lock());
        a.next_node_index == b.next_node_index
            && match (&a.debug_data, &b.debug_data) {
                (None, None) => true,
                (Some(x), Some(y)) => Arc::ptr_eq(x, y),
                _ => false,
            }
            && a.reached == b.reached
            && a.target == b.target
            && a.nodes == b.nodes
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Path(length={})", self.get_node_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> Path {
        Path::new(
            vec![Node::new(0, 0, 0), Node::new(1, 0, 0), Node::new(2, 0, 0)],
            BlockPos::new(2, 0, 3),
            false,
        )
    }

    #[test]
    fn progress_and_dist() {
        let p = path();
        assert!(p.not_started() && !p.is_done());
        assert_eq!(p.get_dist_to_target(), 3.0);
        p.advance();
        p.advance();
        assert_eq!(p.get_previous_node().map(|n| n.x), Some(1));
        p.advance();
        assert!(p.is_done());
    }

    #[test]
    fn shared_handle_and_copy() {
        let p = path();
        let alias = p.clone();
        alias.advance();
        assert_eq!(p.get_next_node_index(), 1);
        let copy = p.copy();
        copy.advance();
        assert_eq!(p.get_next_node_index(), 1);
        assert!(p.same_as(Some(&copy)));
    }

    #[test]
    fn entity_pos_uses_int_width() {
        let p = path();
        let v = p.get_entity_pos_at_node(0.6, 1);
        assert_eq!((v.x, v.y, v.z), (1.5, 0.0, 0.5));
    }
}
