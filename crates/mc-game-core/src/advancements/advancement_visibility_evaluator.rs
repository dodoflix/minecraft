//! `net.minecraft.server.advancements.AdvancementVisibilityEvaluator`.

use super::advancement::Advancement;
use super::advancement_tree::{AdvancementTree, NodeId};

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisibilityRule {
    Show,
    Hide,
    NoChange,
}

fn evaluate_visibility_rule(advancement: &Advancement, is_done: bool) -> VisibilityRule {
    match &advancement.display {
        None => VisibilityRule::Hide,
        Some(_) if is_done => VisibilityRule::Show,
        Some(display) => {
            if display.hidden {
                VisibilityRule::Hide
            } else {
                VisibilityRule::NoChange
            }
        }
    }
}

fn evaluate_visibility_for_unfinished_node(ascendants: &[VisibilityRule]) -> bool {
    for i in 0..=2 {
        match ascendants[ascendants.len() - 1 - i] {
            VisibilityRule::Show => return true,
            VisibilityRule::Hide => return false,
            VisibilityRule::NoChange => {}
        }
    }
    false
}

fn evaluate(
    tree: &AdvancementTree,
    node: NodeId,
    ascendants: &mut Vec<VisibilityRule>,
    is_done: &mut dyn FnMut(NodeId) -> bool,
    output: &mut dyn FnMut(NodeId, bool),
) -> bool {
    let is_self_done = is_done(node);
    let descendant_visibility =
        evaluate_visibility_rule(tree.node(node).advancement(), is_self_done);
    let mut is_self_or_descendant_done = is_self_done;
    ascendants.push(descendant_visibility);
    for child in tree.node(node).children.clone() {
        is_self_or_descendant_done |= evaluate(tree, child, ascendants, is_done, output);
    }
    let visibility =
        is_self_or_descendant_done || evaluate_visibility_for_unfinished_node(ascendants);
    ascendants.pop();
    output(node, visibility);
    is_self_or_descendant_done
}

/// `evaluateVisibility(node, isDone, output)`: the whole tab of `node`, children before their parent.
pub fn evaluate_visibility(
    tree: &AdvancementTree,
    node: NodeId,
    is_done: &mut dyn FnMut(NodeId) -> bool,
    output: &mut dyn FnMut(NodeId, bool),
) {
    let root = tree.root_of(node);
    let mut stack = vec![VisibilityRule::NoChange; 3];
    evaluate(tree, root, &mut stack, is_done, output);
}
