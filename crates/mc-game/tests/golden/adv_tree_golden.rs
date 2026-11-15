//! The advancement tree of the default data pack against the real vanilla classes: the layout of every node (`TreeNodePosition`) and
//! the output of `AdvancementVisibilityEvaluator` for the progress states of `tests/oracle/scenarios/adv_tree/layout.txt`, recorded by
//! `scripts/adv-tree-oracle.sh`. The oracle shadows `AdvancementNode.children` (an identity-hashed set in vanilla, no reproducible order)
//! with an insertion-ordered set, which is the order our tree keeps.

use mc_game::advancements::advancement_tree::NodeId;
use mc_game::advancements::advancement_visibility_evaluator::evaluate_visibility;
use mc_game::advancements::server_advancement_manager::ServerAdvancementManager;
use mc_world::level_storage::WorldDataConfiguration;
use std::collections::HashSet;

fn manager() -> ServerAdvancementManager {
    let _serial = crate::common::SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_, stem) = crate::common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    ServerAdvancementManager::from_registries(stem.registries.composite_access())
}

#[test]
fn tree_layout_and_visibility_match_vanilla() {
    let manager = manager();
    let tree = manager.tree();
    let dir = env!("CARGO_MANIFEST_DIR");
    let scenario =
        std::fs::read_to_string(format!("{dir}/tests/oracle/scenarios/adv_tree/layout.txt"))
            .expect("scenario");
    let mut actual = Vec::new();
    for node in tree.nodes() {
        let parent = node
            .parent
            .map_or("-".to_owned(), |p| tree.node(p).holder.id.to_string());
        actual.push(format!(
            "NODE {} {parent} {} {}",
            node.holder.id,
            mc_core::math::java::java_float_to_string(node.x),
            mc_core::math::java::java_float_to_string(node.y)
        ));
    }
    for line in scenario.lines().filter(|l| l.starts_with("state ")) {
        let mut parts = line.split_whitespace().skip(1);
        let (name, spec) = (parts.next().unwrap(), parts.next().unwrap());
        let exact: HashSet<&str> = spec.split(',').collect();
        let prefixes: Vec<&str> = spec
            .split(',')
            .filter_map(|s| s.strip_prefix("prefix:"))
            .collect();
        let everything = exact.contains("*");
        actual.push(format!("STATE {name}"));
        let mut hidden = 0;
        let mut visible = Vec::new();
        for root in tree.roots().collect::<Vec<NodeId>>() {
            evaluate_visibility(
                tree,
                root,
                &mut |node| {
                    let id = tree.node(node).holder.id.to_string();
                    everything
                        || exact.contains(id.as_str())
                        || prefixes.iter().any(|p| id.starts_with(p))
                },
                &mut |node, shown| {
                    if shown {
                        visible.push(format!("VIS {}", tree.node(node).holder.id));
                    } else {
                        hidden += 1;
                    }
                },
            );
        }
        actual.extend(visible);
        actual.push(format!("HIDDEN {hidden}"));
    }
    let fixture = std::fs::read_to_string(format!("{dir}/tests/fixtures/adv_tree/layout.txt"))
        .expect("fixture");
    let expected: Vec<&str> = fixture.lines().collect();
    assert_eq!(actual.len(), expected.len(), "line count");
    let diffs: Vec<String> = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (a, e))| a.as_str() != **e)
        .take(20)
        .map(|(i, (a, e))| format!("line {}: replay {a}\n          vanilla {e}", i + 1))
        .collect();
    assert!(diffs.is_empty(), "differences:\n{}", diffs.join("\n"));
}

// Scenarios `tests/oracle/scenarios/adv_*.txt` (`scripts/block-oracle.sh NAME`): the content of `update_advancements` (`advpackets:`),
// `PlayerAdvancements.save()` bytes (`advfile:`), reload and the announcement rule against the vanilla level.

/// The first packet of a joining player, decoded, for players with and without progress.
#[test]
fn join_packet_matches_vanilla() {
    crate::common::block_scenario::check("adv_join_packet");
}

/// Grants along a tab: new nodes, progress of the changed advancements and the chat announcements.
#[test]
fn grant_chain_matches_vanilla() {
    crate::common::block_scenario::check("adv_grant_chain");
}

/// Revokes hide nodes again and send the changed progress.
#[test]
fn revoke_visibility_matches_vanilla() {
    crate::common::block_scenario::check("adv_revoke_visibility");
}

/// `show_advancement_messages` off: the packet is sent, nothing is announced.
#[test]
fn announce_rule_matches_vanilla() {
    crate::common::block_scenario::check("adv_announce_off");
}

/// The progress file `PlayerAdvancements.save()` writes, and the state read back from it.
#[test]
fn saved_progress_file_matches_vanilla() {
    crate::common::block_scenario::check("adv_save_load");
}

/// `/reload` restores progress from the saved file and resets the client.
#[test]
fn reload_matches_vanilla() {
    crate::common::block_scenario::check("adv_reload");
}
