//! The trees of `/clear`, `/give`, `/item` and `/loot` against the real vanilla `ClientboundCommandsPacket`
//! (`tests/fixtures/commands/CommandTreeGolden.java clear,give,item,loot`), at every permission level; includes the
//! `item_predicate`, `loot_table`, `item_modifier`, `slot_source`, `item_slot(s)` argument templates.

use crate::command_common;

use mc_protocol::codec::{Decode, Encode};
use mc_protocol::packets::play::ClientboundCommandsPacket;


const SUBSET: &[&str] = &["clear", "give", "item", "loot"];

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn item_command_trees_match_vanilla() {
    use mc_game::commands::{CommandSourceStack, command_tree};
    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel, PermissionSet};

    let commands = command_common::vanilla_commands();
    let subset_root = mc_brigadier::CommandNode::<CommandSourceStack>::new_root();
    for child in commands.get_dispatcher().get_root().get_children() {
        if SUBSET.contains(&child.get_name()) {
            subset_root.add_child(child);
        }
    }
    let mut checked = 0;
    for line in include_str!("../fixtures/commands/tree_golden_items.txt").lines() {
        let mut it = line.split(' ');
        let (_sel, level, _kind, hex) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        let level: i32 = level.parse().unwrap();
        let bytes = unhex(hex);
        let permissions = PermissionSet::from(LevelBasedPermissionSet::for_level(PermissionLevel::by_id(level)));
        let root = command_tree::usable_root(&subset_root, &CommandSourceStack::compilation_context(permissions));
        let mut out = Vec::new();
        command_tree::commands_packet(&root).encode(&mut out).unwrap();
        let ours = ClientboundCommandsPacket::decode(&mut out.as_slice()).unwrap();
        let theirs = ClientboundCommandsPacket::decode(&mut bytes.as_slice()).unwrap();
        for (i, (a, b)) in ours.entries.iter().zip(&theirs.entries).enumerate() {
            assert_eq!(a, b, "permission level {level}: node {i}");
        }
        assert_eq!(ours.entries.len(), theirs.entries.len(), "permission level {level}: node count");
        assert_eq!(out, bytes, "permission level {level}");
        checked += 1;
    }
    assert_eq!(checked, 5);
}
