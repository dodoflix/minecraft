//! The `function`, `return`, `schedule`, `reload`, `datapack` and `debug` subtrees encode to the bytes of the real vanilla
//! `ClientboundCommandsPacket` at every permission level (`fixtures/commands/CommandTreeGolden.java ... function,return,...`).

mod command_common;

#[test]
fn function_system_tree_matches_vanilla() {
    use mc_game::commands::{CommandSourceStack, command_tree};
    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel, PermissionSet};
    use mc_protocol::codec::{Decode, Encode};
    use mc_protocol::packets::play::ClientboundCommandsPacket;

    let commands = command_common::vanilla_commands();
    let subset_root = mc_brigadier::CommandNode::<CommandSourceStack>::new_root();
    for child in commands.get_dispatcher().get_root().get_children() {
        if ["function", "return", "schedule", "reload", "datapack", "debug"].contains(&child.get_name()) {
            subset_root.add_child(child);
        }
    }
    let mut checked = 0;
    for line in include_str!("fixtures/commands/function_tree_golden.txt").lines() {
        let mut it = line.split(' ');
        let (_, level, _, hex) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        let permissions = PermissionSet::from(LevelBasedPermissionSet::for_level(PermissionLevel::by_id(level.parse().unwrap())));
        // like the generator: only the listed top-level commands are copied, but `redirect root` still maps to the new root
        let root = mc_brigadier::CommandNode::<CommandSourceStack>::new_root();
        let mut converted = std::collections::HashMap::new();
        converted.insert(std::sync::Arc::as_ptr(commands.get_dispatcher().get_root()) as usize, root.clone());
        command_tree::fill_usable_commands(&subset_root, &root, &CommandSourceStack::compilation_context(permissions), &mut converted);
        let mut out = Vec::new();
        command_tree::commands_packet(&root).encode(&mut out);
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
