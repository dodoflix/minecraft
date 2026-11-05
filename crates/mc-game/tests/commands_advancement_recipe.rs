//! `/advancement` and `/recipe` command trees against the real vanilla `ClientboundCommandsPacket` (golden `full` tree).

use mc_game::commands::{CommandSourceStack, command_tree};
use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel, PermissionSet};
use mc_protocol::codec::{Decode, Encode};
use mc_protocol::packets::play::{ClientboundCommandsNodeStub, ClientboundCommandsPacket};

mod command_common;
use command_common::vanilla_commands;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// A subtree rendered node by node (stub, flags, redirect, children in order).
fn render(packet: &ClientboundCommandsPacket, index: i32) -> String {
    let entry = &packet.entries[index as usize];
    let children: Vec<String> = entry.children.iter().map(|c| render(packet, *c)).collect();
    format!("({:?} {} {} [{}])", entry.stub, entry.flags, entry.redirect, children.join(","))
}

fn top_level(packet: &ClientboundCommandsPacket, name: &str) -> Option<String> {
    packet.entries[packet.root_index as usize].children.iter().find_map(|c| {
        let node = &packet.entries[*c as usize];
        matches!(&node.stub, Some(ClientboundCommandsNodeStub::Literal(n)) if n == name).then(|| render(packet, *c))
    })
}

#[test]
fn advancement_and_recipe_trees_match_vanilla() {
    let commands = vanilla_commands();
    let mut present = 0;
    for line in include_str!("fixtures/commands/tree_golden.txt").lines() {
        let mut it = line.split(' ');
        let (sel, level, kind, hex) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        if sel != "DEDICATED" || kind != "full" {
            continue;
        }
        let theirs = ClientboundCommandsPacket::decode(&mut unhex(hex).as_slice()).unwrap();
        let permissions = PermissionSet::from(LevelBasedPermissionSet::for_level(PermissionLevel::by_id(level.parse().unwrap())));
        let root = command_tree::usable_root(commands.get_dispatcher().get_root(), &CommandSourceStack::compilation_context(permissions));
        let mut out = Vec::new();
        command_tree::commands_packet(&root).encode(&mut out);
        let ours = ClientboundCommandsPacket::decode(&mut out.as_slice()).unwrap();
        for name in ["advancement", "recipe"] {
            let vanilla = top_level(&theirs, name);
            assert_eq!(top_level(&ours, name), vanilla, "{name} at permission level {level}");
            present += usize::from(vanilla.is_some());
        }
    }
    assert!(present >= 2, "the golden must contain the commands at an op level");
}
