//! The command tree as the client receives it: `Commands.sendCommands`' pruned copy and
//! `ClientboundCommandsPacket`'s node enumeration.

use super::CommandSourceStack;
use super::synchronization::{ArgumentTypeInfos, suggestion_providers};
use crate::server::permissions::PermissionSet;
use mc_brigadier::tree::NodeKind;
use mc_brigadier::{ArgumentBuilder, CommandNode};
use mc_protocol::packets::play::{
    COMMAND_NODE_EXECUTABLE, COMMAND_NODE_RESTRICTED, ClientboundCommandsEntry,
    ClientboundCommandsPacket,
};
use std::collections::{HashMap, VecDeque};
use std::ops::Deref;
use std::sync::{Arc, LazyLock};

type Node = Arc<CommandNode<CommandSourceStack>>;

fn ptr(node: &Node) -> usize {
    Arc::as_ptr(node) as usize
}

/// `Commands.fillUsableCommands`: copies the children of `source` the filter may use into `target`.
pub fn fill_usable_commands(
    source: &Node,
    target: &Node,
    command_filter: &CommandSourceStack,
    converted: &mut HashMap<usize, Node>,
) {
    for child in source.get_children() {
        if child.can_use(command_filter) {
            let builder = child.create_builder();
            let redirect = child
                .get_redirect()
                .and_then(|r| converted.get(&ptr(r)).cloned());
            // `builder.redirect(converted.get(redirect))` (a missing target clears the redirect).
            let builder = builder.forward_shared(redirect, None, false);
            let node = builder.build();
            converted.insert(ptr(&child), node.clone());
            target.add_child(node.clone());
            if child.has_children() {
                fill_usable_commands(&child, &node, command_filter, converted);
            }
        }
    }
}

/// A pruned command tree. Redirects to the root (the `execute` family) make it a cycle of `Arc`s, so dropping it
/// detaches the children to let reference counting free the nodes and the argument types they captured.
pub struct UsableTree(Node);

impl Deref for UsableTree {
    type Target = Node;

    fn deref(&self) -> &Node {
        &self.0
    }
}

impl Drop for UsableTree {
    fn drop(&mut self) {
        self.0.dismantle();
    }
}

/// `Commands.sendCommands` without the sending: the pruned tree for a source.
pub fn usable_root(dispatcher_root: &Node, source: &CommandSourceStack) -> UsableTree {
    let root = CommandNode::new_root();
    let mut converted = HashMap::new();
    converted.insert(ptr(dispatcher_root), root.clone());
    fill_usable_commands(dispatcher_root, &root, source, &mut converted);
    UsableTree(root)
}

static NO_PERMISSION_SOURCE: LazyLock<CommandSourceStack> =
    LazyLock::new(|| CommandSourceStack::compilation_context(PermissionSet::NoPermissions));

/// Node ids keyed by Java `equals` (`Object2IntMap<CommandNode>`): structurally equal nodes (same name and argument type,
/// same command instance, equal children; redirects, requirements and suggestions are not compared) share one id.
#[derive(Default)]
struct NodeIds {
    buckets: HashMap<u64, Vec<(Node, i32)>>,
    hashes: HashMap<usize, u64>,
    count: i32,
}

impl NodeIds {
    /// A hash consistent with `CommandNode.equals` (children are an unordered map there).
    fn hash(&mut self, node: &Node) -> u64 {
        if let Some(h) = self.hashes.get(&ptr(node)) {
            return *h;
        }
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match node.kind() {
            NodeKind::Root => 0u8.hash(&mut hasher),
            NodeKind::Literal { literal, .. } => (1u8, literal).hash(&mut hasher),
            NodeKind::Argument { name, .. } => (2u8, name).hash(&mut hasher),
        }
        node.get_command()
            .map(|c| Arc::as_ptr(&c) as *const () as usize)
            .hash(&mut hasher);
        let mut children = 0u64;
        for child in node.get_children() {
            children = children.wrapping_add(self.hash(&child));
        }
        children.hash(&mut hasher);
        let h = hasher.finish();
        self.hashes.insert(ptr(node), h);
        h
    }

    fn get(&mut self, node: &Node) -> Option<i32> {
        let h = self.hash(node);
        self.buckets
            .get(&h)?
            .iter()
            .find(|(n, _)| **n == **node)
            .map(|(_, id)| *id)
    }

    /// `nodeToId.put(node, nodeToId.size())`.
    fn insert(&mut self, node: &Node) {
        let h = self.hash(node);
        self.buckets
            .entry(h)
            .or_default()
            .push((node.clone(), self.count));
        self.count += 1;
    }
}

/// `new ClientboundCommandsPacket(root, COMMAND_NODE_INSPECTOR)`.
pub fn commands_packet(root: &Node) -> ClientboundCommandsPacket {
    // enumerateNodes: breadth first, children then redirect; equal nodes get one id.
    let mut ids = NodeIds::default();
    let mut ordered: Vec<Node> = Vec::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    queue.push_back(root.clone());
    while let Some(node) = queue.pop_front() {
        if ids.get(&node).is_none() {
            ids.insert(&node);
            ordered.push(node.clone());
            queue.extend(node.get_children());
            if let Some(redirect) = node.get_redirect() {
                queue.push_back(redirect.clone());
            }
        }
    }
    let entries = ordered
        .iter()
        .map(|node| create_entry(node, &mut ids))
        .collect();
    ClientboundCommandsPacket {
        entries,
        root_index: ids.get(root).expect("the root is enumerated"),
    }
}

fn create_entry(node: &Node, ids: &mut NodeIds) -> ClientboundCommandsEntry {
    let children: Vec<i32> = node
        .get_children()
        .iter()
        .map(|c| ids.get(c).expect("children are enumerated"))
        .collect();
    let mut entry = match node.kind() {
        NodeKind::Root => ClientboundCommandsEntry::root(children),
        NodeKind::Literal { literal, .. } => {
            ClientboundCommandsEntry::literal(literal.clone(), children)
        }
        NodeKind::Argument {
            name,
            type_,
            custom_suggestions,
        } => {
            let suggestion_id = custom_suggestions
                .as_ref()
                .map(suggestion_providers::get_name);
            ClientboundCommandsEntry::argument(
                name.clone(),
                ArgumentTypeInfos::unpack(type_.as_ref()),
                suggestion_id,
                children,
            )
        }
    };
    if let Some(redirect) = node.get_redirect() {
        entry = entry.redirect_to(ids.get(redirect).expect("redirects are enumerated"));
    }
    if node.get_command().is_some() {
        entry.flags |= COMMAND_NODE_EXECUTABLE;
    }
    if !node.can_use(&NO_PERMISSION_SOURCE) {
        entry.flags |= COMMAND_NODE_RESTRICTED;
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_brigadier::{CommandDispatcher, literal};

    #[test]
    fn usable_tree_is_freed_on_drop() {
        let dispatcher = CommandDispatcher::<CommandSourceStack>::new();
        dispatcher.register(literal("say"));
        let root = dispatcher.get_root().clone();
        dispatcher.register(literal("execute").then(literal("as").redirect(root)));
        let source = CommandSourceStack::compilation_context(PermissionSet::NoPermissions);

        let tree = usable_root(dispatcher.get_root(), &source);
        let execute = tree
            .get_children()
            .into_iter()
            .find(|c| c.get_name() == "execute")
            .unwrap();
        let redirecting = execute.get_children().into_iter().next().unwrap();
        assert!(Arc::ptr_eq(redirecting.get_redirect().unwrap(), &*tree));
        let probes = (Arc::downgrade(&execute), Arc::downgrade(&redirecting));
        let root_probe = Arc::downgrade(&*tree);
        drop((execute, redirecting));

        drop(tree);
        assert!(probes.0.upgrade().is_none());
        assert!(probes.1.upgrade().is_none());
        assert!(root_probe.upgrade().is_none());
    }
}
