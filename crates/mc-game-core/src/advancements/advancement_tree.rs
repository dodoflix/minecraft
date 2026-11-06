//! `net.minecraft.advancements.AdvancementTree` + `AdvancementNode`.
//!
//! Nodes live in an arena (`NodeId` = index in insertion order, like vanilla's linked hash map). The children of a
//! node are kept in insertion order (vanilla uses a `ReferenceOpenHashSet`, identity-hash ordered, which cannot be
//! reproduced; the layout of siblings follows insertion order here).

use std::collections::HashMap;

use mc_core::ident::Identifier;

use super::advancement::{Advancement, AdvancementHolder};
use super::tree_node_position;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Clone)]
pub struct AdvancementNode {
    pub holder: AdvancementHolder,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub x: f32,
    pub y: f32,
}

impl AdvancementNode {
    pub fn advancement(&self) -> &Advancement {
        &self.holder.value
    }

    pub fn is_task(&self) -> bool {
        self.parent.is_some()
    }

    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }
}

#[derive(Clone, Default)]
pub struct AdvancementTree {
    nodes: Vec<AdvancementNode>,
    by_id: HashMap<Identifier, NodeId>,
}

impl AdvancementTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// `addAll(advancements)`: repeatedly inserts every advancement whose parent is known; stops (logging) when a
    /// pass inserts nothing.
    pub fn add_all(&mut self, advancements: impl IntoIterator<Item = AdvancementHolder>) {
        let mut to_add: Vec<AdvancementHolder> = advancements.into_iter().collect();
        while !to_add.is_empty() {
            let before = to_add.len();
            to_add.retain(|h| !self.try_insert(h));
            if to_add.len() == before {
                log::error!("Couldn't load advancements: {:?}", to_add);
                break;
            }
        }
        log::info!("Loaded {} advancements", self.nodes.len());
    }

    fn try_insert(&mut self, holder: &AdvancementHolder) -> bool {
        let parent = match &holder.value.parent {
            Some(parent_id) => match self.by_id.get(parent_id) {
                Some(p) => Some(*p),
                None => return false,
            },
            None => None,
        };
        let id = NodeId(self.nodes.len());
        self.nodes.push(AdvancementNode {
            holder: holder.clone(),
            parent,
            children: Vec::new(),
            x: 0.0,
            y: 0.0,
        });
        if let Some(p) = parent {
            self.nodes[p.0].children.push(id);
        }
        self.by_id.insert(holder.id.clone(), id);
        true
    }

    pub fn nodes(&self) -> &[AdvancementNode] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> &AdvancementNode {
        &self.nodes[id.0]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// `roots()`.
    pub fn roots(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len())
            .map(NodeId)
            .filter(|n| self.nodes[n.0].is_root())
    }

    /// `tasks()`.
    pub fn tasks(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len())
            .map(NodeId)
            .filter(|n| self.nodes[n.0].is_task())
    }

    /// `get(id)`.
    pub fn get(&self, id: &Identifier) -> Option<NodeId> {
        self.by_id.get(id).copied()
    }

    /// `get(holder)`.
    pub fn get_holder(&self, holder: &AdvancementHolder) -> Option<NodeId> {
        self.get(&holder.id)
    }

    /// `AdvancementNode.getRoot`.
    pub fn root_of(&self, mut node: NodeId) -> NodeId {
        while let Some(parent) = self.nodes[node.0].parent {
            node = parent;
        }
        node
    }

    /// `repositionNodes()`: lays out every visible root.
    pub fn reposition_nodes(&mut self) {
        let roots: Vec<NodeId> = self.roots().collect();
        for root in roots {
            if self.nodes[root.0].advancement().display.is_some() {
                tree_node_position::run(self, root);
            }
        }
    }

    pub(crate) fn set_location(&mut self, node: NodeId, x: f32, y: f32) {
        self.nodes[node.0].x = x;
        self.nodes[node.0].y = y;
    }
}
