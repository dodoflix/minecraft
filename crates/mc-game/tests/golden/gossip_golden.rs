//! `GossipContainer` vs vanilla: random op scripts (`oracle/GossipOracle.java`, fixture `fixtures/brain/gossip.txt`):
//! adds/removes/decay/transfers (seeded `LegacyRandomSource`), reputation queries, per-target contents, the HashMap order of the targets
//! (as seen by the NBT encoding) and the NBT round trip.

use mc_core::random::LegacyRandomSource;
use mc_core::uuid_util::uuid_to_int_array;
use mc_game::world::entity::ai::gossip::{GossipContainer, GossipType};
use mc_nbt::Tag;
use std::collections::BTreeMap;
use uuid::Uuid;

fn ty(name: &str) -> GossipType {
    GossipType::by_id(name).unwrap_or_else(|| panic!("gossip type {name}"))
}

fn dump(c: &GossipContainer) -> BTreeMap<String, Vec<(String, i32)>> {
    let mut m: BTreeMap<String, Vec<(String, i32)>> = BTreeMap::new();
    for (target, entries) in c.get_gossip_entries() {
        m.insert(
            target.to_string(),
            entries
                .into_iter()
                .map(|(t, v)| (t.id().to_string(), v))
                .collect(),
        );
    }
    m
}

fn parse_dump(line: &str) -> BTreeMap<String, Vec<(String, i32)>> {
    let mut m = BTreeMap::new();
    let mut rest = line.trim();
    while let Some(open) = rest.find('[') {
        let target = rest[..open].trim().to_string();
        let close = rest.find(']').expect("]");
        let entries: Vec<(String, i32)> = rest[open + 1..close]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("k=v");
                (k.to_string(), v.parse().expect("int"))
            })
            .collect();
        m.insert(target, entries);
        rest = rest[close + 1..].trim();
    }
    m
}

fn order(c: &GossipContainer) -> String {
    let mut seen: Vec<String> = Vec::new();
    for e in c.unpack() {
        let s = format!("{:?}", uuid_to_int_array(&e.target));
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    seen.join(" ")
}

#[test]
fn gossip_container_matches_vanilla() {
    let golden = include_str!("../fixtures/brain/gossip.txt");
    let mut c = GossipContainer::new();
    let mut ops = 0;
    let mut unordered = false;
    for (n, line) in golden.lines().enumerate() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let ctx = format!("line {} `{line}`", n + 1);
        match w[0] {
            "scenario" => {
                c = GossipContainer::new();
                unordered = false;
            }
            "unordered" => unordered = true,
            "add" => c.add(
                w[1].parse::<Uuid>().unwrap(),
                ty(w[2]),
                w[3].parse().unwrap(),
            ),
            "remove_amount" => c.remove_amount(
                w[1].parse::<Uuid>().unwrap(),
                ty(w[2]),
                w[3].parse().unwrap(),
            ),
            "remove" => c.remove(&w[1].parse::<Uuid>().unwrap(), ty(w[2])),
            "decay" => c.decay(),
            "remove_type" => c.remove_type(ty(w[1])),
            "transfer" => {
                let seed: i64 = w[1].parse().unwrap();
                let max: i32 = w[2].parse().unwrap();
                let n_src: usize = w[3].parse().unwrap();
                let mut s = GossipContainer::new();
                for i in 0..n_src {
                    s.add(
                        w[4 + 3 * i].parse::<Uuid>().unwrap(),
                        ty(w[5 + 3 * i]),
                        w[6 + 3 * i].parse().unwrap(),
                    );
                }
                let moved = c.transfer_from(&s, &mut LegacyRandomSource::new(seed), max);
                assert_eq!(moved.to_string(), *w.last().unwrap(), "{ctx}");
            }
            "query" => {
                let mask: u32 = w[2].parse().unwrap();
                let rep = c.get_reputation(&w[1].parse::<Uuid>().unwrap(), |t| {
                    mask >> t.ordinal() & 1 != 0
                });
                let cnt = c.get_count_for_type(ty(w[3]), |v| v > 3.0);
                assert_eq!(
                    format!("{rep} {cnt}"),
                    format!("{} {}", w[5], w[6]),
                    "{ctx}"
                );
            }
            "dump" => assert_eq!(
                dump(&c),
                parse_dump(line.strip_prefix("dump").unwrap()),
                "{ctx}"
            ),
            "copy" => assert_eq!(
                dump(&c.copy()),
                parse_dump(line.strip_prefix("copy").unwrap()),
                "{ctx}"
            ),
            "order" => {
                let (mut got, mut want): (Vec<String>, Vec<String>) = (
                    order(&c).split("] ").map(String::from).collect(),
                    line.strip_prefix("order ")
                        .unwrap_or("")
                        .split("] ")
                        .map(String::from)
                        .collect(),
                );
                if unordered {
                    got.sort();
                    want.sort();
                }
                assert_eq!(got, want, "{ctx}");
            }
            "nbt_roundtrip" => {
                let back = GossipContainer::from_nbt(&c.to_nbt());
                assert_eq!(
                    dump(&back),
                    parse_dump(line.strip_prefix("nbt_roundtrip").unwrap()),
                    "{ctx}"
                );
            }
            other => panic!("unknown op {other}"),
        }
        ops += 1;
    }
    assert!(ops > 700);
    // an invalid entry is dropped, valid ones kept
    let mut list = mc_nbt::ListTag::new();
    let mut ok = mc_nbt::CompoundTag::new();
    ok.put("Target", Tag::IntArray(vec![1, 2, 3, 4]));
    ok.put_string("Type", "trading");
    ok.put_int("Value", 5);
    let mut bad = ok.clone();
    bad.put_int("Value", 0);
    list.push(Tag::Compound(bad));
    list.push(Tag::Compound(ok));
    assert_eq!(GossipContainer::from_nbt(&list).unpack().len(), 1);
}
