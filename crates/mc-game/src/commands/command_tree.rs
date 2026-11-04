//! The command tree as the client receives it: `Commands.sendCommands`' pruned copy and
//! `ClientboundCommandsPacket`'s node enumeration.

use super::synchronization::{ArgumentTypeInfos, suggestion_providers};
use super::CommandSourceStack;
use crate::server::permissions::PermissionSet;
use mc_brigadier::tree::NodeKind;
use mc_brigadier::{ArgumentBuilder, CommandNode};
use mc_protocol::packets::play::{
    COMMAND_NODE_EXECUTABLE, COMMAND_NODE_RESTRICTED, ClientboundCommandsEntry, ClientboundCommandsPacket,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock};

type Node = Arc<CommandNode<CommandSourceStack>>;

fn ptr(node: &Node) -> usize {
    Arc::as_ptr(node) as usize
}

/// `Commands.fillUsableCommands`: copies the children of `source` the filter may use into `target`.
pub fn fill_usable_commands(source: &Node, target: &Node, command_filter: &CommandSourceStack, converted: &mut HashMap<usize, Node>) {
    for child in source.get_children() {
        if child.can_use(command_filter) {
            let builder = child.create_builder();
            let redirect = child.get_redirect().and_then(|r| converted.get(&ptr(r)).cloned());
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

/// `Commands.sendCommands` without the sending: the pruned tree for a source.
pub fn usable_root(dispatcher_root: &Node, source: &CommandSourceStack) -> Node {
    let root = CommandNode::new_root();
    let mut converted = HashMap::new();
    converted.insert(ptr(dispatcher_root), root.clone());
    fill_usable_commands(dispatcher_root, &root, source, &mut converted);
    root
}

static NO_PERMISSION_SOURCE: LazyLock<CommandSourceStack> =
    LazyLock::new(|| CommandSourceStack::compilation_context(PermissionSet::NoPermissions));

/// `new ClientboundCommandsPacket(root, COMMAND_NODE_INSPECTOR)`.
pub fn commands_packet(root: &Node) -> ClientboundCommandsPacket {
    // enumerateNodes: breadth first, children then redirect.
    let mut node_to_id: HashMap<usize, i32> = HashMap::new();
    let mut ordered: Vec<Node> = Vec::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    queue.push_back(root.clone());
    while let Some(node) = queue.pop_front() {
        if !node_to_id.contains_key(&ptr(&node)) {
            node_to_id.insert(ptr(&node), ordered.len() as i32);
            ordered.push(node.clone());
            queue.extend(node.get_children());
            if let Some(redirect) = node.get_redirect() {
                queue.push_back(redirect.clone());
            }
        }
    }
    let entries = ordered.iter().map(|node| create_entry(node, &node_to_id)).collect();
    ClientboundCommandsPacket { entries, root_index: node_to_id[&ptr(root)] }
}

fn create_entry(node: &Node, ids: &HashMap<usize, i32>) -> ClientboundCommandsEntry {
    let children: Vec<i32> = node.get_children().iter().map(|c| ids[&ptr(c)]).collect();
    let mut entry = match node.kind() {
        NodeKind::Root => ClientboundCommandsEntry::root(children),
        NodeKind::Literal { literal, .. } => ClientboundCommandsEntry::literal(literal.clone(), children),
        NodeKind::Argument { name, type_, custom_suggestions } => {
            let suggestion_id = custom_suggestions.as_ref().map(suggestion_providers::get_name);
            ClientboundCommandsEntry::argument(name.clone(), ArgumentTypeInfos::unpack(type_.as_ref()), suggestion_id, children)
        }
    };
    if let Some(redirect) = node.get_redirect() {
        entry = entry.redirect_to(ids[&ptr(redirect)]);
    }
    if node.get_command().is_some() {
        entry.flags |= COMMAND_NODE_EXECUTABLE;
    }
    if !node.can_use(&NO_PERMISSION_SOURCE) {
        entry.flags |= COMMAND_NODE_RESTRICTED;
    }
    entry
}
