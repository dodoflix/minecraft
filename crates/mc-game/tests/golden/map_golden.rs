//! `MapItem.update` / `MapItemSavedData.HoldingPlayer` against the real vanilla classes: the fixture is produced by
//! `tests/oracle/MapGolden.java` (see its header): same terrain, same player, same ticks, every `map_item_data` packet and the
//! final colors/decorations must be identical.

use crate::common::feature_harness;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game::server::level::ServerLevel;
use mc_game::world::container::Container;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::ItemBehaviour;
use mc_game::world::item::map_item::{self, MapItem};
use mc_game::world::level::LevelAccessor;
use mc_protocol::codec::GameProfile;
use mc_protocol::components::{MAP_ID, MAP_POST_PROCESSING, MapPostProcessing};
use std::collections::BTreeMap;

const LO: i32 = -48;
const HI: i32 = 48;
const Y_MIN: i32 = 40;
const Y_MAX: i32 = 100;

fn mix(x: i32, z: i32, seed: i32) -> i32 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(z.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
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
    /// `POST` / `POSTDECO` lines of the crafting post-processing copies of this map.
    post: Vec<String>,
}

fn load() -> Vec<Case> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/map_golden.txt"
    ))
    .expect("fixture");
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(kind), Some(index)) = (it.next(), it.next()) else {
            continue;
        };
        let rest: Vec<&str> = it.collect();
        match kind {
            "CASE" => cases.push(Case {
                params: rest.iter().map(|s| s.parse().unwrap()).collect(),
                ..Default::default()
            }),
            "PLAYER" => {
                cases.last_mut().unwrap().player = rest.iter().map(|s| s.to_string()).collect()
            }
            "PKT" => {
                cases
                    .last_mut()
                    .unwrap()
                    .packets
                    .insert(rest[0].parse().unwrap(), rest[1].to_string());
            }
            "CENTER" => {
                cases.last_mut().unwrap().center = rest.iter().map(|s| s.parse().unwrap()).collect()
            }
            "COLORS" => cases.last_mut().unwrap().colors = rest[0].to_string(),
            "DECO" => cases.last_mut().unwrap().decos.push(rest.join(" ")),
            "POST" | "POSTDECO" => cases[index.parse::<usize>().unwrap()]
                .post
                .push(format!("{kind} {}", rest.join(" "))),
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
        let [scale, px, _py, pz, _yaw, ticks, seed] = case.params[..] else {
            panic!("params")
        };
        let mut level = ServerLevel::detached(overworld.clone());
        for x in LO..HI {
            for z in LO..HI {
                for y in Y_MIN..Y_MAX {
                    if let Some(block) = state(x, y, z, seed) {
                        level.set_block(
                            BlockPos::new(x, y, z),
                            block.default_state(),
                            18 | 256 | 512,
                            0,
                        );
                    }
                }
            }
        }
        // the oracle world allocates one map id per case, and two more for each case that post-processes copies of its map
        let earlier_post = cases[..i].iter().filter(|c| !c.post.is_empty()).count();
        for _ in 0..i + 2 * earlier_post {
            level.get_free_map_id();
        }
        let map = map_item::create(&level, px, pz, scale as i8, true, false);
        let id = *map.get(&MAP_ID).unwrap();
        let mut p = ServerPlayer::new(
            1,
            overworld.clone(),
            GameProfile {
                id: uuid::Uuid::from_u128(1),
                name: format!("p{i}"),
                properties: vec![],
            },
            Default::default(),
        );
        let (x, y, z, yaw): (f64, f64, f64, f32) = (
            case.player[0].parse().unwrap(),
            case.player[1].parse().unwrap(),
            case.player[2].parse().unwrap(),
            case.player[3].parse().unwrap(),
        );
        p.set_pos(x, y, z);
        p.base_mut().set_y_rot(yaw);
        p.player.inventory.set_item(0, map);
        let pid = p.base_id();
        let mut got: BTreeMap<usize, String> = BTreeMap::new();
        for t in 0..ticks as usize {
            p.tick_inventory_items(&mut level);
            if let Some(Some(packet)) =
                level.with_map_data(id, |_, data| data.get_update_packet(id, pid))
            {
                got.insert(
                    t,
                    hex(&mc_protocol::packet::encode_packet(&packet).unwrap()),
                );
            }
        }
        let (colors, center, decos) = level
            .with_map_data(id, |_, d| {
                (
                    hex(&d.colors),
                    vec![d.center_x, d.center_z, i32::from(d.scale)],
                    d.get_decorations()
                        .map(|d| format!("{} {} {} {}", d.ty.name(), d.x, d.y, d.rot))
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap();
        assert_eq!(center, case.center, "case {i}: center/scale");
        if colors != case.colors {
            let at = colors
                .as_bytes()
                .chunks(2)
                .zip(case.colors.as_bytes().chunks(2))
                .position(|(a, b)| a != b)
                .unwrap();
            let (ax, ay) = (at % 128, at / 128);
            let around = |c: &str| {
                (0..4)
                    .map(|k| {
                        c[(ax + (ay + k - 1) * 128) * 2..(ax + (ay + k - 1) * 128) * 2 + 2]
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let n = colors
                .as_bytes()
                .chunks(2)
                .zip(case.colors.as_bytes().chunks(2))
                .filter(|(a, b)| a != b)
                .count();
            panic!(
                "case {i}: {n} color mismatches, first at pixel x={ax} y={ay}; rust column {} java column {}",
                around(&colors),
                around(&case.colors)
            );
        }
        assert_eq!(decos, case.decos, "case {i}: decorations");
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            case.packets.keys().collect::<Vec<_>>(),
            "case {i}: ticks with a map packet"
        );
        for (t, expected) in &case.packets {
            assert_eq!(&got[t], expected, "case {i}: packet of tick {t}");
        }
        if !case.post.is_empty() {
            let stack = p.player.inventory.get_item(0).clone();
            let mut got_post = Vec::new();
            for (kind, name) in [
                (MapPostProcessing::Lock, "LOCK"),
                (MapPostProcessing::Scale, "SCALE"),
            ] {
                let mut copy = stack.clone();
                copy.set(MAP_POST_PROCESSING, kind);
                MapItem.on_crafted_post_process(&mut copy, &mut level);
                let tag = format!("{name} {}", copy.get(&MAP_POST_PROCESSING).is_some());
                let (head, decos) = level
                    .with_map_data(*copy.get(&MAP_ID).unwrap(), |_, d| {
                        (
                            format!(
                                "POST {tag} {} {} {} {} {}",
                                d.center_x,
                                d.center_z,
                                d.scale,
                                d.locked,
                                hex(&d.colors)
                            ),
                            d.get_decorations()
                                .map(|d| {
                                    format!(
                                        "POSTDECO {tag} {} {} {} {}",
                                        d.ty.name(),
                                        d.x,
                                        d.y,
                                        d.rot
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .unwrap();
                got_post.push(head);
                got_post.extend(decos);
            }
            assert_eq!(got_post, case.post, "case {i}: crafting post-processing");
        }
    }
}

/// `MapPreviewGolden`: the water outline `MapItem.renderBiomePreviewMap` draws over the seeded biome source of a normal
/// world.
#[test]
fn biome_preview_map_matches_vanilla() {
    const SEED: i64 = 4242;
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = crate::common::load_world(
        dir.path(),
        mc_world::level_storage::WorldDataConfiguration::DEFAULT,
        true,
    )
    .expect("default world loads");
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.registries = stem.registries.composite_access().clone();
    level.gen_env = Some(feature_harness::gen_env(SEED));
    level.tick_state.biomes = Some(feature_harness::WORLD.biomes.clone());
    level.tick_state.biome_zoom_seed = mc_worldgen::biome::BiomeManager::obfuscate_seed(SEED);
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/map_preview_golden.txt"
    ))
    .expect("fixture");
    let mut cases: Vec<(Vec<i32>, String)> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace().skip(2);
        if line.starts_with("CASE ") {
            cases.push((it.map(|s| s.parse().unwrap()).collect(), String::new()));
        } else if line.starts_with("COLORS ") {
            cases.last_mut().unwrap().1 = it.next().unwrap().to_string();
        }
    }
    assert_eq!(cases.len(), 8);
    let mut failures = Vec::new();
    for (i, (params, expected)) in cases.iter().enumerate() {
        let [scale, x, z] = params[..] else {
            panic!("params")
        };
        let map = map_item::create(&level, x, z, scale as i8, true, true);
        map_item::render_biome_preview_map(&level, &map);
        let colors = level
            .read_map_data(*map.get(&MAP_ID).unwrap(), |d| hex(&d.colors))
            .unwrap();
        let differing: Vec<usize> = colors
            .as_bytes()
            .chunks(2)
            .zip(expected.as_bytes().chunks(2))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(at, _)| at)
            .collect();
        if let Some(&first) = differing.first() {
            let pair = |at: usize| (&colors[at * 2..at * 2 + 2], &expected[at * 2..at * 2 + 2]);
            failures.push(format!(
                "case {i} (scale {scale} at {x} {z}): {} of 16384 colors differ, first at x={} y={} (rust, vanilla) {:?}, last at x={} y={}",
                differing.len(),
                first % 128,
                first / 128,
                pair(first),
                differing[differing.len() - 1] % 128,
                differing[differing.len() - 1] / 128,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `EmptyMapItem.use`, `MapItem.inventoryTick` and the banner toggle of `MapItem.useOn` in the replayed level.
#[test]
fn map_item_use_scenario_matches_vanilla() {
    crate::common::block_scenario::check("itm_map_use");
}
