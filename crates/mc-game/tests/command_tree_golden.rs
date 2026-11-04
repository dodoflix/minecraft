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
