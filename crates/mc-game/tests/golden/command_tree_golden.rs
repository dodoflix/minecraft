//! The command tree packet against the real vanilla `ClientboundCommandsPacket`
//! (`tests/fixtures/commands/CommandTreeGolden.java`).

use mc_protocol::codec::{Decode, Encode};
use mc_protocol::packets::play::ClientboundCommandsPacket;

use crate::command_common::vanilla_commands;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn golden() -> Vec<(String, u32, String, Vec<u8>)> {
    include_str!("../fixtures/commands/tree_golden.txt")
        .lines()
        .map(|l| {
            let mut it = l.split(' ');
            let (sel, level, kind, hex) = (
                it.next().unwrap(),
                it.next().unwrap(),
                it.next().unwrap(),
                it.next().unwrap(),
            );
            (
                sel.to_owned(),
                level.parse().unwrap(),
                kind.to_owned(),
                unhex(hex),
            )
        })
        .collect()
}

/// Every argument type vanilla's own tree uses survives decode + re-encode byte for byte (templates, flags, ids).
#[test]
fn vanilla_full_trees_roundtrip() {
    for (sel, level, kind, bytes) in golden() {
        let packet = ClientboundCommandsPacket::decode(&mut bytes.as_slice())
            .unwrap_or_else(|e| panic!("{sel} {level} {kind}: {e:?}"));
        let mut out = Vec::new();
        packet.encode(&mut out).unwrap();
        assert_eq!(out, bytes, "{sel} {level} {kind}");
    }
}

/// Our pruned command tree for a permission level, as the client would receive it (encoded, decoded again).
fn ours_packet(commands: &mc_game::commands::Commands, level: u32) -> ClientboundCommandsPacket {
    use mc_game::commands::{CommandSourceStack, command_tree};
    use mc_game::server::permissions::{LevelBasedPermissionSet, PermissionLevel, PermissionSet};

    let permissions = PermissionSet::from(LevelBasedPermissionSet::for_level(
        PermissionLevel::by_id(level as i32),
    ));
    let root = command_tree::usable_root(
        commands.get_dispatcher().get_root(),
        &CommandSourceStack::compilation_context(permissions),
    );
    let mut out = Vec::new();
    command_tree::commands_packet(&root)
        .encode(&mut out)
        .unwrap();
    ClientboundCommandsPacket::decode(&mut out.as_slice()).unwrap()
}

/// The vanilla packet of the DEDICATED `full` golden (`CommandTreeGolden.java`) for a permission level.
fn vanilla_packet(level: u32) -> ClientboundCommandsPacket {
    let (.., bytes) = golden()
        .into_iter()
        .find(|(sel, l, kind, _)| sel == "DEDICATED" && *l == level && kind == "full")
        .expect("golden entry");
    ClientboundCommandsPacket::decode(&mut bytes.as_slice()).unwrap()
}

/// EVERY top-level command our dispatcher registers equals vanilla's subtree for the same command at every permission
/// level (name, flags, parser template, suggestion provider, children order, redirect target by path; `gamerule`'s
/// children as a set). Commands we do not register are only coverage (`tree_coverage_report`), but a command we DO
/// register must be visible at exactly the permission levels where vanilla shows it.
#[test]
fn tree_matches_vanilla_per_command() {
    use mc_protocol::command_tree_diff::diff_trees;

    let commands = vanilla_commands();
    let registered: Vec<String> = commands
        .get_dispatcher()
        .get_root()
        .get_children()
        .iter()
        .map(|c| c.get_name().to_owned())
        .collect();
    let mut failures = String::new();
    for level in 0..=4 {
        let (ours, vanilla) = (ours_packet(&commands, level), vanilla_packet(level));
        let report = diff_trees(&vanilla, &ours);
        let hidden: Vec<&String> = report
            .missing_in_b
            .iter()
            .filter(|n| registered.contains(n))
            .collect();
        if !report.common_equal() || !hidden.is_empty() {
            failures += &format!("--- permission level {level}\n{}", report.render());
            if !hidden.is_empty() {
                failures +=
                    &format!("registered but not sent where vanilla sends them: {hidden:?}\n");
            }
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

/// Prints the vanilla top-level commands that are not registered yet (per permission level 4 = everything).
/// Run: `cargo test -p mc-game --test command_tree_golden tree_coverage_report -- --ignored --nocapture`.
#[test]
#[ignore = "report only"]
fn tree_coverage_report() {
    use mc_protocol::command_tree_diff::{diff_trees, top_level};

    let commands = vanilla_commands();
    let (ours, vanilla) = (ours_packet(&commands, 4), vanilla_packet(4));
    let report = diff_trees(&vanilla, &ours);
    let total = top_level(&vanilla).len();
    println!(
        "registered {}/{} vanilla top-level commands (permission level 4)",
        report.equal.len() + report.differing.len(),
        total
    );
    println!(
        "not registered yet ({}): {}",
        report.missing_in_b.len(),
        report.missing_in_b.join(" ")
    );
    if !report.differing.is_empty() || !report.extra_in_b.is_empty() {
        println!("{}", report.render());
    }
}

/// The whole packet equals vanilla's byte for byte (DEDICATED, permission level 0..=4); `gamerule`'s children are put
/// in name order on both sides first (identity-hash order in vanilla, see `gamerule_tree_has_vanilla_literals`).
#[test]
#[ignore = "enable when all commands land"]
fn tree_equals_vanilla_full() {
    use mc_protocol::command_tree_diff::{canonical, diff_trees};

    let commands = vanilla_commands();
    for level in 0..=4 {
        let (ours, vanilla) = (ours_packet(&commands, level), vanilla_packet(level));
        let (mut a, mut b) = (Vec::new(), Vec::new());
        canonical(&vanilla).encode(&mut a).unwrap();
        canonical(&ours).encode(&mut b).unwrap();
        assert!(
            a == b,
            "permission level {level}: tree differs from vanilla\n{}",
            diff_trees(&vanilla, &ours).render()
        );
    }
}

/// `/gamerule` has one unqualified and one qualified literal per enabled rule (order is arbitrary in vanilla).
#[test]
fn gamerule_tree_has_vanilla_literals() {
    use mc_game::world::level::gamerules::AnyGameRule;

    let features = mc_world::level_storage::FeatureFlagSet::ALL;
    let commands = vanilla_commands();
    let root = commands.get_dispatcher().get_root();
    let gamerule = root
        .get_children()
        .into_iter()
        .find(|c| c.get_name() == "gamerule")
        .expect("gamerule registered");
    let mut names: Vec<String> = gamerule
        .get_children()
        .iter()
        .map(|c| c.get_name().to_owned())
        .collect();
    names.sort();
    let mut expected: Vec<String> = AnyGameRule::all()
        .filter(|r| r.is_enabled(features))
        .flat_map(|r| [r.id(), r.get_identifier().to_string()])
        .collect();
    expected.sort();
    assert_eq!(names, expected);
}
