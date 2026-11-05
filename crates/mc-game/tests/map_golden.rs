//! `MapItem.update` / `MapItemSavedData.HoldingPlayer` against the real vanilla classes: the fixture is produced by
//! `tests/oracle/MapGolden.java` (see its header): same terrain, same player, same ticks, every `map_item_data` packet and the
//! final colors/decorations must be identical.

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::level_events::encode_packet;
use mc_game::world::container::Container;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::map_item;
use mc_game::world::level::LevelAccessor;
use mc_protocol::codec::GameProfile;
use mc_protocol::components::MAP_ID;
use std::collections::BTreeMap;

const LO: i32 = -48;
const HI: i32 = 48;
const Y_MIN: i32 = 40;
const Y_MAX: i32 = 100;

fn mix(x: i32, z: i32, seed: i32) -> i32 {
    let mut h = x.wrapping_mul(374761393).wrapping_add(z.wrapping_mul(668265263)).wrapping_add(seed.wrapping_mul(1274126177));
    h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1274126177);
    h ^ ((h as u32) >> 16) as i32
}

/// `MapGolden.state`.
fn state(x: i32, y: i32, z: i32, seed: i32) -> Option<Block> {
    let kind = mix(x >> 2, z >> 2, seed).rem_euclid(12);
    let h = 62 + ((mix(x >> 1, z >> 1, seed + 7) & 7) - 3);
    let d = 1 + (mix(x, z, seed + 3) & 7);
    match kind {
        0 | 1 => (y >= 50 && y <= h).then_some(Block::STONE),
        2 => {
            if y == h {
                Some(Block::GRASS_BLOCK)
            } else {
                (y >= h - 3 && y < h).then_some(Block::DIRT)
            }
        }
        3 => {
            if y == h {
                Some(Block::SAND)
            } else {
                (y >= 50 && y < h).then_some(Block::SANDSTONE)
            }
        }
        4 => {
            if y == h - d {
                Some(Block::SAND)
            } else if y > h - d && y <= h {
                Some(Block::WATER)
            } else {
                (y >= 50 && y < h - d).then_some(Block::STONE)
            }
        }
        5 => {
            if y == h {
                Some(Block::ICE)
            } else if y >= h - 3 && y < h {
                Some(Block::WATER)
            } else {
                (y == h - 4).then_some(Block::SAND)
            }
        }
        6 => {
            if y == h + 1 {
                Some(Block::SNOW)
            } else if y == h {
                Some(Block::SNOW_BLOCK)
            } else {
                (y >= 50 && y < h).then_some(Block::STONE)
            }
        }
        7 => {
            if y == h + 3 {
                Some(Block::OAK_LEAVES)
            } else {
                (y >= 50 && y <= h).then_some(Block::STONE)
            }
        }
        8 => {
            if y == h {
                Some(Block::GLASS)
            } else {
                (y >= 50 && y < h).then_some(Block::STONE)
            }
        }
        9 => {
            if y >= h - 2 && y <= h {
                Some(Block::LAVA)
            } else {
                (y >= 50 && y < h - 2).then_some(Block::STONE)
            }
        }
        10 => None,
        _ => (y >= 50 && y <= h).then_some(Block::TERRACOTTA),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Default)]
struct Case {
    params: Vec<i32>,
    player: Vec<String>,
    packets: BTreeMap<usize, String>,
    center: Vec<i32>,
    colors: String,
    decos: Vec<String>,
}

fn load() -> Vec<Case> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/map_golden.txt")).expect("fixture");
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(kind), Some(_)) = (it.next(), it.next()) else { continue };
        let rest: Vec<&str> = it.collect();
        match kind {
            "CASE" => cases.push(Case { params: rest.iter().map(|s| s.parse().unwrap()).collect(), ..Default::default() }),
            "PLAYER" => cases.last_mut().unwrap().player = rest.iter().map(|s| s.to_string()).collect(),
            "PKT" => {
                cases.last_mut().unwrap().packets.insert(rest[0].parse().unwrap(), rest[1].to_string());
            }
            "CENTER" => cases.last_mut().unwrap().center = rest.iter().map(|s| s.parse().unwrap()).collect(),
            "COLORS" => cases.last_mut().unwrap().colors = rest[0].to_string(),
            "DECO" => cases.last_mut().unwrap().decos.push(rest.join(" ")),
            _ => panic!("bad fixture line {line}"),
        }
    }
    cases
}

#[test]
fn map_update_and_packets_match_vanilla() {
    let overworld = Identifier::parse("minecraft:overworld").unwrap();
    let cases = load();
    assert_eq!(cases.len(), 6);
    for (i, case) in cases.iter().enumerate() {
        let [scale, px, _py, pz, _yaw, ticks, seed] = case.params[..] else { panic!("params") };
        let mut level = ServerLevel::detached(overworld.clone());
        for x in LO..HI {
            for z in LO..HI {
                for y in Y_MIN..Y_MAX {
                    if let Some(block) = state(x, y, z, seed) {
                        level.set_block(BlockPos::new(x, y, z), block.default_state(), 18 | 256 | 512, 0);
                    }
                }
            }
        }
        // the oracle world allocates one map id per case
        for _ in 0..i {
            level.get_free_map_id();
        }
        let map = map_item::create(&mut level, px, pz, scale as i8, true, false);
        let id = *map.get(&MAP_ID).unwrap();
        let mut p = ServerPlayer::new(1, overworld.clone(), GameProfile { id: uuid::Uuid::from_u128(1), name: format!("p{i}"), properties: vec![] }, Default::default());
        let (x, y, z, yaw): (f64, f64, f64, f32) = (case.player[0].parse().unwrap(), case.player[1].parse().unwrap(), case.player[2].parse().unwrap(), case.player[3].parse().unwrap());
        p.set_pos(x, y, z);
        p.base_mut().set_y_rot(yaw);
        p.player.inventory.set_item(0, map);
        let pid = p.base_id();
        let mut got: BTreeMap<usize, String> = BTreeMap::new();
        for t in 0..ticks as usize {
            p.tick_inventory_items(&mut level);
            if let Some(Some(packet)) = level.with_map_data(id, |_, data| data.get_update_packet(id, pid)) {
                got.insert(t, hex(&mc_protocol::packet::encode_packet(&packet).unwrap()));
            }
        }
        let (colors, center, decos) = level
            .with_map_data(id, |_, d| (hex(&d.colors), vec![d.center_x, d.center_z, i32::from(d.scale)], d.get_decorations().map(|d| format!("{} {} {} {}", d.ty.name(), d.x, d.y, d.rot)).collect::<Vec<_>>()))
            .unwrap();
        assert_eq!(center, case.center, "case {i}: center/scale");
        if colors != case.colors {
            let at = colors.as_bytes().chunks(2).zip(case.colors.as_bytes().chunks(2)).position(|(a, b)| a != b).unwrap();
            let (ax, ay) = (at % 128, at / 128);
            let around = |c: &str| (0..4).map(|k| format!("{}", &c[(ax + (ay + k - 1) * 128) * 2..(ax + (ay + k - 1) * 128) * 2 + 2])).collect::<Vec<_>>().join(",");
            let n = colors.as_bytes().chunks(2).zip(case.colors.as_bytes().chunks(2)).filter(|(a, b)| a != b).count();
            panic!("case {i}: {n} color mismatches, first at pixel x={ax} y={ay}; rust column {} java column {}", around(&colors), around(&case.colors));
        }
        assert_eq!(decos, case.decos, "case {i}: decorations");
        assert_eq!(got.keys().collect::<Vec<_>>(), case.packets.keys().collect::<Vec<_>>(), "case {i}: ticks with a map packet");
        for (t, expected) in &case.packets {
            assert_eq!(&got[t], expected, "case {i}: packet of tick {t}");
        }
    }
}
