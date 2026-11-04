//! The command tree packet against the real vanilla `ClientboundCommandsPacket`
//! (`tests/fixtures/commands/CommandTreeGolden.java`).

use mc_protocol::codec::{Decode, Encode};
use mc_protocol::packets::play::ClientboundCommandsPacket;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn golden() -> Vec<(String, u32, String, Vec<u8>)> {
    include_str!("fixtures/commands/tree_golden.txt")
        .lines()
        .map(|l| {
            let mut it = l.split(' ');
            let (sel, level, kind, hex) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
            (sel.to_owned(), level.parse().unwrap(), kind.to_owned(), unhex(hex))
        })
        .collect()
}

/// Every argument type vanilla's own tree uses survives decode + re-encode byte for byte (templates, flags, ids).
#[test]
fn vanilla_full_trees_roundtrip() {
    for (sel, level, kind, bytes) in golden() {
        let packet = ClientboundCommandsPacket::decode(&mut bytes.as_slice()).unwrap_or_else(|e| panic!("{sel} {level} {kind}: {e:?}"));
        let mut out = Vec::new();
        packet.encode(&mut out);
        assert_eq!(out, bytes, "{sel} {level} {kind}");
    }
}


/// The vanilla command tree: all features enabled like the golden generator (`createValidationContext`), built-in
/// registries plus empty data-driven ones (the tree only needs registry existence).
fn vanilla_commands() -> mc_game::commands::Commands {
    use mc_game::commands::HolderLookupProvider as _;
    use mc_game::commands::command_build_context::StaticRegistries;
    use mc_game::commands::{CommandBuildContext, CommandSelection, Commands};
    use std::sync::Arc;

    struct WithEmptyDynamic;
    struct Empty;
    impl mc_game::commands::RegistryView for Empty {
        fn contains(&self, _: &mc_core::ident::Identifier) -> bool {
            false
        }
        fn element_ids(&self) -> Vec<mc_core::ident::Identifier> {
            Vec::new()
        }
        fn tag_ids(&self) -> Vec<mc_core::ident::Identifier> {
            Vec::new()
        }
        fn has_tag(&self, _: &mc_core::ident::Identifier) -> bool {
            false
        }
        fn tag_elements(&self, _: &mc_core::ident::Identifier) -> Option<Vec<mc_core::ident::Identifier>> {
            None
        }
    }
    impl mc_game::commands::HolderLookupProvider for WithEmptyDynamic {
        fn lookup(&self, key: &str) -> Option<Arc<dyn mc_game::commands::RegistryView>> {
            StaticRegistries.lookup(key).or_else(|| Some(Arc::new(Empty) as Arc<dyn mc_game::commands::RegistryView>))
        }
        fn list_registry_keys(&self) -> Vec<String> {
            StaticRegistries.list_registry_keys()
        }
    }
    mc_game::commands::BrigadierExceptions::install();
    let commands = Commands::new(
        CommandSelection::Dedicated,
        &CommandBuildContext::simple(Arc::new(WithEmptyDynamic), mc_world::level_storage::FeatureFlagSet::ALL),
    );
    commands
}

const SUBSET: &[&str] = &[
    "stop", "save-all", "save-on", "save-off", "say", "list", "op", "deop", "kick", "ban", "ban-ip", "pardon", "pardon-ip", "banlist",
    "whitelist", "tp", "teleport", "gamemode", "time", "weather", "difficulty", "help", "seed", "msg", "tell", "w", "me",
    "tellraw", "setblock",
];

/// (`gamerule` is left out: vanilla iterates its rules in `Reference2ObjectOpenHashMap` (identity hash) order, which
/// differs between JVM runs; `gamerule_tree_has_vanilla_literals` checks its content.)
///
/// The Rust command tree of the implemented top-level commands encodes to the bytes of the real vanilla
/// `ClientboundCommandsPacket` for the same commands, at every permission level.
#[test]
fn subset_tree_matches_vanilla() {
    use mc_game::commands::HolderLookupProvider as _;
    use mc_game::commands::command_build_context::StaticRegistries;
    use mc_game::commands::{CommandBuildContext, CommandSelection, CommandSourceStack, Commands, command_tree};
    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel, PermissionSet};
    use std::sync::Arc;

    let commands = vanilla_commands();
    let subset_root = mc_brigadier::CommandNode::<CommandSourceStack>::new_root();
    for child in commands.get_dispatcher().get_root().get_children() {
        if SUBSET.contains(&child.get_name()) {
            subset_root.add_child(child);
        }
    }
    let mut checked = 0;
    for (sel, level, kind, bytes) in golden() {
        if sel != "DEDICATED" || kind != "subset" {
            continue;
        }
        let permissions = PermissionSet::from(LevelBasedPermissionSet::for_level(PermissionLevel::by_id(level as i32)));
        let root = command_tree::usable_root(&subset_root, &CommandSourceStack::compilation_context(permissions));
        let mut out = Vec::new();
        command_tree::commands_packet(&root).encode(&mut out);
        let ours = ClientboundCommandsPacket::decode(&mut out.as_slice()).unwrap();
        let theirs = ClientboundCommandsPacket::decode(&mut bytes.as_slice()).unwrap();
        for (i, (a, b)) in ours.entries.iter().zip(&theirs.entries).enumerate() {
            assert_eq!(a, b, "permission level {level}: node {i}");
        }
        assert_eq!(ours.entries.len(), theirs.entries.len(), "permission level {level}: node count");
        assert_eq!(ours, theirs, "permission level {level}");
        assert_eq!(out, bytes, "permission level {level}");
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// `/gamerule` has one unqualified and one qualified literal per enabled rule (order is arbitrary in vanilla).
#[test]
fn gamerule_tree_has_vanilla_literals() {
    use mc_game::world::level::gamerules::AnyGameRule;

    let features = mc_world::level_storage::FeatureFlagSet::ALL;
    let commands = vanilla_commands();
    let root = commands.get_dispatcher().get_root();
    let gamerule = root.get_children().into_iter().find(|c| c.get_name() == "gamerule").expect("gamerule registered");
    let mut names: Vec<String> = gamerule.get_children().iter().map(|c| c.get_name().to_owned()).collect();
    names.sort();
    let mut expected: Vec<String> =
        AnyGameRule::all().filter(|r| r.is_enabled(features)).flat_map(|r| [r.id(), r.get_identifier().to_string()]).collect();
    expected.sort();
    assert_eq!(names, expected);
}
