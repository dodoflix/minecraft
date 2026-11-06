//! `net.minecraft.world.level.pathfinder.BinaryHeap`: the open set, a min-heap on `Node.f` that stores the heap index
//! in the node (`heap_idx`). The heap holds [`NodeId`]s into the evaluator's node pool, which every operation takes.

use super::node::{Node, NodeId};

pub struct BinaryHeap {
    heap: Vec<Option<NodeId>>,
    size: usize,
}

impl Default for BinaryHeap {
    fn default() -> Self {
        BinaryHeap {
            heap: vec![None; 128],
            size: 0,
        }
    }
}

impl BinaryHeap {
    /// `insert(node)`.
    pub fn insert(&mut self, nodes: &mut [Node], id: NodeId) -> NodeId {
        assert!(nodes[id].heap_idx < 0, "OW KNOWS!");
        if self.size == self.heap.len() {
            let new_len = self.size << 1;
            self.heap.resize(new_len, None);
        }
        self.heap[self.size] = Some(id);
        nodes[id].heap_idx = self.size as i32;
        let idx = self.size;
        self.size += 1;
        self.up_heap(nodes, idx);
        id
    }

    pub fn clear(&mut self) {
        self.size = 0;
    }

    /// `peek()`.
    pub fn peek(&self) -> Option<NodeId> {
        self.heap[0]
    }

    /// `pop()`.
    pub fn pop(&mut self, nodes: &mut [Node]) -> NodeId {
        let popped = self.heap[0].expect("pop on empty heap");
        self.size -= 1;
        self.heap[0] = self.heap[self.size];
        self.heap[self.size] = None;
        if self.size > 0 {
            self.down_heap(nodes, 0);
        }
        nodes[popped].heap_idx = -1;
        popped
    }

    /// `remove(node)`.
    pub fn remove(&mut self, nodes: &mut [Node], id: NodeId) {
        let at = nodes[id].heap_idx as usize;
        self.size -= 1;
        self.heap[at] = self.heap[self.size];
        self.heap[self.size] = None;
        if self.size > at {
            let moved = self.heap[at].expect("moved node");
            if nodes[moved].f < nodes[id].f {
                self.up_heap(nodes, at);
            } else {
                self.down_heap(nodes, at);
            }
        }
        nodes[id].heap_idx = -1;
    }

    /// `changeCost(node, newCost)`.
    pub fn change_cost(&mut self, nodes: &mut [Node], id: NodeId, new_cost: f32) {
        let old_cost = nodes[id].f;
        nodes[id].f = new_cost;
        let at = nodes[id].heap_idx as usize;
        if new_cost < old_cost {
            self.up_heap(nodes, at);
        } else {
            self.down_heap(nodes, at);
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    fn up_heap(&mut self, nodes: &mut [Node], mut idx: usize) {
        let id = self.heap[idx].expect("heap slot");
        let cost = nodes[id].f;
        while idx > 0 {
            let parent_idx = (idx - 1) >> 1;
            let parent = self.heap[parent_idx].expect("heap slot");
            if !(cost < nodes[parent].f) {
                break;
            }
            self.heap[idx] = Some(parent);
            nodes[parent].heap_idx = idx as i32;
            idx = parent_idx;
        }
        self.heap[idx] = Some(id);
        nodes[id].heap_idx = idx as i32;
    }

    fn down_heap(&mut self, nodes: &mut [Node], mut idx: usize) {
        let id = self.heap[idx].expect("heap slot");
        let cost = nodes[id].f;
        loop {
            let left_idx = 1 + (idx << 1);
            let right_idx = left_idx + 1;
            if left_idx >= self.size {
                break;
            }
            let left = self.heap[left_idx].expect("heap slot");
            let left_cost = nodes[left].f;
            let (right, right_cost) = if right_idx >= self.size {
                (None, f32::INFINITY)
            } else {
                let right = self.heap[right_idx].expect("heap slot");
                (Some(right), nodes[right].f)
            };
            if left_cost < right_cost {
                if !(left_cost < cost) {
                    break;
                }
                self.heap[idx] = Some(left);
                nodes[left].heap_idx = idx as i32;
                idx = left_idx;
            } else {
                if !(right_cost < cost) {
                    break;
                }
                let right = right.expect("right child");
                self.heap[idx] = Some(right);
                nodes[right].heap_idx = idx as i32;
                idx = right_idx;
            }
        }
        self.heap[idx] = Some(id);
        nodes[id].heap_idx = idx as i32;
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `getHeap()`.
    pub fn get_heap(&self) -> Vec<NodeId> {
        self.heap[..self.size]
            .iter()
            .map(|n| n.expect("heap slot"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pops_in_f_order() {
        let mut nodes: Vec<Node> = (0..6).map(|i| Node::new(i, 0, 0)).collect();
        let fs = [5.0, 1.0, 4.0, 2.0, 3.0, 0.5];
        let mut heap = BinaryHeap::default();
        for (i, f) in fs.iter().enumerate() {
            nodes[i].f = *f;
            heap.insert(&mut nodes, i);
        }
        heap.change_cost(&mut nodes, 0, 0.1);
        let order: Vec<_> =
            std::iter::from_fn(|| (!heap.is_empty()).then(|| heap.pop(&mut nodes))).collect();
        assert_eq!(order, vec![0, 5, 1, 3, 4, 2]);
    }
}
