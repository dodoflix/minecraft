//! `level` play packets vs the vanilla `GameProtocols` codecs (`fixtures/play/GenLevel.java` -> `level.golden`).
use crate::common::*;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;
use mc_data::block::{BLOCK_COUNT, BlockState, STATE_COUNT};
use mc_data::{Block, BlockEntityType, SoundEvent};
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_protocol::codec::{BitSet, DecodeError, GlobalPos, from_bytes, to_bytes};
use mc_protocol::packets::play::*;

fn positions() -> [BlockPos; 4] {
    [
        BlockPos::new(0, 0, 0),
        BlockPos::new(1, 64, -1),
        BlockPos::new(-30000000, -64, 29999999),
        BlockPos::new(33554431, 2047, -33554432),
    ]
}

fn state(id: usize) -> BlockState {
    BlockState::from_id(id as u32).unwrap()
}

fn bytes(n: usize, seed: i32) -> Vec<u8> {
    (0..n as i32)
        .map(|i| (i * 31 + seed * 7 + (i >> 8) * 13) as u8)
        .collect()
}

fn longs(n: usize, seed: i64) -> Vec<i64> {
    (0..n as i64)
        .map(|i| {
            seed.wrapping_mul(0x9E3779B97F4A7C15u64 as i64)
                .wrapping_add(i.wrapping_mul(0x632BE59BD9B4E019u64 as i64))
        })
        .collect()
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

fn sign() -> CompoundTag {
    let mut c = CompoundTag::new();
    c.put_boolean("is_waxed", true);
    c.put_string("name", "\u{1F600} sign \u{e9}");
    let mut lines = ListTag::new();
    lines.push("a");
    lines.push("");
    c.put("lines", Tag::List(lines));
    c.put_long("big", i64::MIN);
    c.put_double("d", -0.0);
    c
}

fn blocks(g: &Golden) {
    let pos = positions();
    for s in [0, 1, 127, 128, 16384, i32::MAX, -1] {
        g.check(
            &format!("block_changed_ack/{s}"),
            &ClientboundBlockChangedAckPacket { sequence: s },
        );
    }
    for (i, &block_pos) in pos.iter().enumerate() {
        let progress = if i == 3 { 255 } else { i as u8 * 3 };
        g.check(
            &format!("block_destruction/{i}"),
            &ClientboundBlockDestructionPacket {
                id: i as i32 * 1000 - 1,
                pos: block_pos,
                progress,
            },
        );
    }
    g.check(
        "block_destruction/progress_clear",
        &ClientboundBlockDestructionPacket {
            id: 7,
            pos: pos[1],
            progress: 255,
        },
    );

    let last_state = STATE_COUNT - 1;
    for id in [0, 1, 100, 5000, 20000] {
        g.check(
            &format!("block_update/{id}"),
            &ClientboundBlockUpdatePacket {
                pos: pos[id % 4],
                block_state: state(id),
            },
        );
    }
    g.check(
        "block_update/last",
        &ClientboundBlockUpdatePacket {
            pos: pos[2],
            block_state: state(last_state),
        },
    );
    for id in [0, 1, 99, last_state] {
        g.check(
            &format!("add_transient_block/{id}"),
            &ClientboundAddTransientBlockPacket {
                pos: pos[id % 4],
                block_state: state(id),
            },
        );
    }
    for id in [0, 1, 200, BLOCK_COUNT - 1] {
        g.check(
            &format!("block_event/{id}"),
            &ClientboundBlockEventPacket {
                pos: pos[id % 4],
                block: Block::from_id(id as u32).unwrap(),
                b0: (id % 256) as u8,
                b1: (255 - id % 256) as u8,
            },
        );
    }
    g.check(
        "block_event/max",
        &ClientboundBlockEventPacket {
            pos: pos[1],
            block: Block::from_id(5).unwrap(),
            b0: 255,
            b1: 255,
        },
    );

    for id in [0, 7, BlockEntityType::registry().len() - 1] {
        let t = BlockEntityType::from_id(id as u32).unwrap();
        g.check(
            &format!("block_entity_data/{id}"),
            &ClientboundBlockEntityDataPacket {
                pos: pos[id % 4],
                r#type: t,
                tag: sign(),
            },
        );
        g.check(
            &format!("block_entity_data/{id}_empty"),
            &ClientboundBlockEntityDataPacket {
                pos: pos[id % 4],
                r#type: t,
                tag: CompoundTag::new(),
            },
        );
    }
}

fn section_blocks_update(g: &Golden) {
    let last_state = STATE_COUNT - 1;
    let pkt = |section_pos, changes| ClientboundSectionBlocksUpdatePacket {
        section_pos,
        changes,
    };
    g.check(
        "section_blocks_update/empty",
        &pkt(SectionPos::of(0, 0, 0), vec![]),
    );
    g.check(
        "section_blocks_update/one",
        &pkt(SectionPos::of(-5, 3, 2000000), vec![(0x123, state(1))]),
    );
    let pos = [0u16, 0xfff, 0x0f0, 0x00f, 0xf00, 0x456];
    let ids = [0, 1, 127, 128, last_state, 30000];
    g.check(
        "section_blocks_update/many",
        &pkt(
            SectionPos::of(-2097152, -2048, 2097151),
            pos.iter().zip(ids).map(|(&p, i)| (p, state(i))).collect(),
        ),
    );
    let large = (0..300usize)
        .map(|i| ((i * 13 % 4096) as u16, state(i * 37 % last_state)))
        .collect();
    g.check(
        "section_blocks_update/large",
        &pkt(SectionPos::of(10, -4, -10), large),
    );
}

fn chunks_and_light(g: &Golden) {
    for d in [
        Difficulty::Peaceful,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
    ] {
        let name = format!("{d:?}").to_uppercase();
        g.check(
            &format!("change_difficulty/{name}/open"),
            &ClientboundChangeDifficultyPacket {
                difficulty: d,
                locked: false,
            },
        );
        g.check(
            &format!("change_difficulty/{name}/locked"),
            &ClientboundChangeDifficultyPacket {
                difficulty: d,
                locked: true,
            },
        );
    }
    for s in [0, 1, 5, 300, i32::MAX] {
        g.check(
            &format!("chunk_batch_finished/{s}"),
            &ClientboundChunkBatchFinishedPacket { batch_size: s },
        );
    }
    g.check("chunk_batch_start", &ClientboundChunkBatchStartPacket);
    g.check(
        "low_disk_space_warning",
        &ClientboundLowDiskSpaceWarningPacket,
    );
    g.check("start_configuration", &ClientboundStartConfigurationPacket);

    let biome = |x, z, n, seed| ChunkBiomeData {
        pos: ChunkPos::new(x, z),
        buffer: bytes(n, seed),
    };
    g.check(
        "chunks_biomes/empty",
        &ClientboundChunksBiomesPacket {
            chunk_biome_data: vec![],
        },
    );
    g.check(
        "chunks_biomes/one_empty_buffer",
        &ClientboundChunksBiomesPacket {
            chunk_biome_data: vec![biome(0, 0, 0, 0)],
        },
    );
    g.check(
        "chunks_biomes/many",
        &ClientboundChunksBiomesPacket {
            chunk_biome_data: vec![
                biome(-1, 1, 40, 1),
                biome(i32::MIN, i32::MAX, 300, 2),
                biome(1875016, -1875016, 70000, 3),
            ],
        },
    );
    for (x, z) in [(0, 0), (-1, 1), (i32::MIN, i32::MAX)] {
        g.check(
            &format!("forget_level_chunk/{x}_{z}"),
            &ClientboundForgetLevelChunkPacket {
                pos: ChunkPos::new(x, z),
            },
        );
    }

    let be = |packed_xz, y, t: u32, tag| BlockEntityInfo {
        packed_xz,
        y,
        r#type: BlockEntityType::from_id(t).unwrap(),
        tag,
    };
    let last_be = BlockEntityType::registry().len() as u32 - 1;
    let bes = vec![
        be(0x00, 0, 0, None),
        be(0xf5u8 as i8, -64, 3, Some(sign())),
        be(0x7f, 319, last_be, Some(CompoundTag::new())),
        be(0x81u8 as i8, 32767, 9, Some(sign())),
    ];
    let all_maps: Vec<_> = [
        HeightmapTypes::WorldSurfaceWg,
        HeightmapTypes::WorldSurface,
        HeightmapTypes::OceanFloorWg,
        HeightmapTypes::OceanFloor,
        HeightmapTypes::MotionBlocking,
        HeightmapTypes::MotionBlockingNoLeaves,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, t)| (t, longs(37 + i * 3, 100 + i as i64)))
    .collect();
    let client_maps = vec![
        (HeightmapTypes::WorldSurface, longs(37, 7)),
        (HeightmapTypes::MotionBlocking, vec![]),
    ];
    let data = |heightmaps, buffer, block_entities_data| ClientboundLevelChunkPacketData {
        heightmaps,
        buffer,
        block_entities_data,
    };
    let bits = |w: &[i64]| BitSet::from_longs(w);
    let layers = |n, seed| vec![bytes(2048, seed); n];

    let empty_light = ClientboundLightUpdatePacketData {
        sky_y_mask: BitSet::new(),
        block_y_mask: BitSet::new(),
        empty_sky_y_mask: BitSet::new(),
        empty_block_y_mask: BitSet::new(),
        sky_updates: vec![],
        block_updates: vec![],
    };
    let light = ClientboundLightUpdatePacketData {
        sky_y_mask: bits(&[0b1011]),
        block_y_mask: bits(&[0xff00ff00ff00ff00u64 as i64, 3]),
        empty_sky_y_mask: bits(&[1 << 63]),
        empty_block_y_mask: bits(&[0x10]),
        sky_updates: vec![bytes(2048, 11), bytes(2048, 12), bytes(2048, 13)],
        block_updates: vec![bytes(2048, 14), bytes(2048, 15)],
    };
    let short_layers = ClientboundLightUpdatePacketData {
        sky_y_mask: bits(&[0b1]),
        block_y_mask: bits(&[0b10]),
        empty_sky_y_mask: BitSet::new(),
        empty_block_y_mask: bits(&[1 << 20]),
        sky_updates: vec![vec![]],
        block_updates: vec![bytes(5, 1), bytes(2048, 2)],
    };
    let many_light = ClientboundLightUpdatePacketData {
        sky_y_mask: bits(&[-1, -1]),
        block_y_mask: bits(&[-1]),
        empty_sky_y_mask: bits(&[0b110]),
        empty_block_y_mask: bits(&[0b1]),
        sky_updates: layers(20, 21),
        block_updates: layers(24, 22),
    };
    let with_light = |x, z, chunk_data, light_data| ClientboundLevelChunkWithLightPacket {
        x,
        z,
        chunk_data,
        light_data,
    };
    g.check(
        "level_chunk_with_light/empty",
        &with_light(0, 0, data(vec![], vec![], vec![]), empty_light.clone()),
    );
    g.check(
        "level_chunk_with_light/client_maps",
        &with_light(
            -3,
            7,
            data(client_maps.clone(), bytes(1500, 5), vec![]),
            light.clone(),
        ),
    );
    g.check(
        "level_chunk_with_light/all_maps_block_entities",
        &with_light(
            i32::MIN,
            i32::MAX,
            data(all_maps, bytes(100000, 6), bes.clone()),
            short_layers.clone(),
        ),
    );
    g.check(
        "level_chunk_with_light/big_light",
        &with_light(
            1875000,
            -1875000,
            data(client_maps, bytes(70, 8), bes[..2].to_vec()),
            many_light.clone(),
        ),
    );
    let upd = |x, z, light_data| ClientboundLightUpdatePacket { x, z, light_data };
    g.check("light_update/empty", &upd(0, 0, empty_light));
    g.check("light_update/light", &upd(-1, 300, light));
    g.check(
        "light_update/short_layers",
        &upd(i32::MAX, i32::MIN, short_layers),
    );
    g.check("light_update/many", &upd(2000000, -2000000, many_light));

    for (x, z) in [(0, 0), (-1, 1), (1875000, -1875000), (i32::MIN, i32::MAX)] {
        g.check(
            &format!("set_chunk_cache_center/{x}_{z}"),
            &ClientboundSetChunkCacheCenterPacket { x, z },
        );
    }
    for v in [0, 2, 32, 127, 128, i32::MAX, -1] {
        g.check(
            &format!("set_chunk_cache_radius/{v}"),
            &ClientboundSetChunkCacheRadiusPacket { radius: v },
        );
        g.check(
            &format!("set_simulation_distance/{v}"),
            &ClientboundSetSimulationDistancePacket {
                simulation_distance: v,
            },
        );
    }
}

fn events_and_border(g: &Golden) {
    use ClientboundGameEventPacket as E;
    let events = [
        E::NO_RESPAWN_BLOCK_AVAILABLE,
        E::START_RAINING,
        E::STOP_RAINING,
        E::CHANGE_GAME_MODE,
        E::WIN_GAME,
        E::DEMO_EVENT,
        E::PLAY_ARROW_HIT_SOUND,
        E::RAIN_LEVEL_CHANGE,
        E::THUNDER_LEVEL_CHANGE,
        E::PUFFER_FISH_STING,
        E::GUARDIAN_ELDER_EFFECT,
        E::IMMEDIATE_RESPAWN,
        E::LIMITED_CRAFTING,
        E::LEVEL_CHUNKS_LOAD_START,
    ];
    let params = [0.0, 1.0, 0.5, -3.25, 101.0, 104.0, f32::MAX, f32::NAN];
    for (i, event) in events.into_iter().enumerate() {
        assert_eq!(event as usize, i);
        let p = E {
            event,
            param: params[i % params.len()],
        };
        if p.param.is_nan() {
            check_bits(g, &format!("game_event/{i}"), &p);
        } else {
            g.check(&format!("game_event/{i}"), &p);
        }
    }
    g.check(
        "game_event/demo_hint_4",
        &E {
            event: E::DEMO_EVENT,
            param: E::DEMO_PARAM_HINT_4,
        },
    );

    let pos = positions();
    let ev = [(1000, 0), (1010, i32::MAX), (-1, i32::MIN), (3000, 5)];
    for (i, (t, d)) in ev.into_iter().enumerate() {
        g.check(
            &format!("level_event/{i}"),
            &ClientboundLevelEventPacket {
                r#type: t,
                pos: pos[i],
                data: d,
                global_event: i % 2 == 0,
            },
        );
    }

    g.check(
        "initialize_border/default",
        &ClientboundInitializeBorderPacket {
            new_center_x: 0.0,
            new_center_z: 0.0,
            old_size: 5.9999968E7,
            new_size: 5.9999968E7,
            lerp_time: 0,
            new_absolute_max_size: 29999984,
            warning_blocks: 5,
            warning_time: 15,
        },
    );
    g.check(
        "initialize_border/lerping",
        &ClientboundInitializeBorderPacket {
            new_center_x: -123.5,
            new_center_z: 1.0e9,
            old_size: 100.0,
            new_size: 3.0,
            lerp_time: 1234567890123,
            new_absolute_max_size: 1,
            warning_blocks: 0,
            warning_time: i32::MAX,
        },
    );
    let extreme = ClientboundInitializeBorderPacket {
        new_center_x: f64::NAN,
        new_center_z: -0.0,
        old_size: f64::INFINITY,
        new_size: f64::from_bits(1),
        lerp_time: i64::MAX,
        new_absolute_max_size: i32::MAX,
        warning_blocks: -1,
        warning_time: i32::MIN,
    };
    check_bits(g, "initialize_border/extreme", &extreme);

    g.check(
        "set_border_center/0",
        &ClientboundSetBorderCenterPacket {
            new_center_x: 0.0,
            new_center_z: 0.0,
        },
    );
    g.check(
        "set_border_center/1",
        &ClientboundSetBorderCenterPacket {
            new_center_x: -29999984.25,
            new_center_z: 29999984.75,
        },
    );
    g.check(
        "set_border_lerp_size/0",
        &ClientboundSetBorderLerpSizePacket {
            old_size: 1.0,
            new_size: 59999968.0,
            lerp_time: 0,
        },
    );
    g.check(
        "set_border_lerp_size/1",
        &ClientboundSetBorderLerpSizePacket {
            old_size: 59999968.0,
            new_size: 0.125,
            lerp_time: 9007199254740993,
        },
    );
    for (label, size) in [
        ("0.0", 0.0),
        ("1.0", 1.0),
        ("5.9999968E7", 5.9999968E7),
        ("-1.5", -1.5),
    ] {
        g.check(
            &format!("set_border_size/{label}"),
            &ClientboundSetBorderSizePacket { size },
        );
    }
    for v in [0, 15, 300, i32::MAX, -1] {
        g.check(
            &format!("set_border_warning_delay/{v}"),
            &ClientboundSetBorderWarningDelayPacket { warning_delay: v },
        );
        g.check(
            &format!("set_border_warning_distance/{v}"),
            &ClientboundSetBorderWarningDistancePacket { warning_blocks: v },
        );
    }
}

fn spawn_infos() -> Vec<CommonPlayerSpawnInfo> {
    let overworld = Some(GlobalPos {
        dimension: id("minecraft:overworld"),
        pos: BlockPos::new(100, 64, -100),
    });
    let nether = Some(GlobalPos {
        dimension: id("minecraft:the_nether"),
        pos: BlockPos::new(-33554432, -2048, 33554431),
    });
    let info = |dimension_type,
                dim: &str,
                seed,
                game_type,
                previous_game_type,
                is_debug,
                is_flat,
                last_death_location,
                portal_cooldown,
                sea_level| {
        CommonPlayerSpawnInfo {
            dimension_type,
            dimension: id(dim),
            seed,
            game_type,
            previous_game_type,
            is_debug,
            is_flat,
            last_death_location,
            portal_cooldown,
            sea_level,
        }
    };
    use GameType::*;
    vec![
        info(
            0,
            "minecraft:overworld",
            0,
            Survival,
            None,
            false,
            false,
            None,
            0,
            63,
        ),
        info(
            1,
            "minecraft:the_nether",
            -1,
            Creative,
            Some(Survival),
            true,
            false,
            overworld,
            10,
            32,
        ),
        info(
            2,
            "minecraft:the_end",
            i64::MAX,
            Adventure,
            Some(Creative),
            false,
            true,
            nether,
            300,
            -64,
        ),
        info(
            3,
            "custom:my_dim",
            i64::MIN,
            Spectator,
            Some(Spectator),
            true,
            true,
            Some(GlobalPos {
                dimension: id("a:b/c"),
                pos: BlockPos::ZERO,
            }),
            i32::MAX,
            i32::MAX,
        ),
        info(
            0,
            "minecraft:overworld",
            12345678901234,
            Survival,
            Some(Adventure),
            false,
            false,
            None,
            1,
            0,
        ),
    ]
}

fn login_respawn_spawn(g: &Golden) {
    let infos = spawn_infos();
    for (i, info) in infos.iter().enumerate() {
        g.check(
            &format!("respawn/{i}"),
            &ClientboundRespawnPacket {
                common_player_spawn_info: info.clone(),
                data_to_keep: (i % 4) as i8,
            },
        );
    }
    g.check(
        "respawn/keep_all",
        &ClientboundRespawnPacket {
            common_player_spawn_info: infos[0].clone(),
            data_to_keep: ClientboundRespawnPacket::KEEP_ALL_DATA,
        },
    );
    g.check(
        "respawn/keep_neg",
        &ClientboundRespawnPacket {
            common_player_spawn_info: infos[1].clone(),
            data_to_keep: -1,
        },
    );
    assert!(
        ClientboundRespawnPacket {
            common_player_spawn_info: infos[0].clone(),
            data_to_keep: 2
        }
        .should_keep(ClientboundRespawnPacket::KEEP_ENTITY_DATA)
    );
    assert!(
        !ClientboundRespawnPacket {
            common_player_spawn_info: infos[0].clone(),
            data_to_keep: 2
        }
        .should_keep(ClientboundRespawnPacket::KEEP_ATTRIBUTE_MODIFIERS)
    );

    let login = |player_id,
                 hardcore,
                 levels: &[&str],
                 max_players,
                 chunk_radius,
                 simulation_distance,
                 flags: [bool; 3],
                 i: usize,
                 online_mode,
                 enforces_secure_chat| {
        ClientboundLoginPacket {
            player_id,
            hardcore,
            levels: levels.iter().map(|l| id(l)).collect(),
            max_players,
            chunk_radius,
            simulation_distance,
            reduced_debug_info: flags[0],
            show_death_screen: flags[1],
            do_limited_crafting: flags[2],
            common_player_spawn_info: infos[i].clone(),
            online_mode,
            enforces_secure_chat,
        }
    };
    g.check(
        "login/0",
        &login(0, false, &[], 0, 0, 0, [false; 3], 0, false, false),
    );
    g.check(
        "login/1",
        &login(
            1,
            true,
            &[
                "minecraft:overworld",
                "minecraft:the_nether",
                "minecraft:the_end",
            ],
            20,
            10,
            8,
            [true; 3],
            1,
            true,
            true,
        ),
    );
    g.check(
        "login/2",
        &login(
            -1,
            false,
            &["custom:my_dim", "a:b/c"],
            i32::MAX,
            32,
            127,
            [false, true, false],
            2,
            true,
            false,
        ),
    );
    g.check(
        "login/3",
        &login(
            i32::MAX,
            true,
            &["minecraft:overworld"],
            100,
            128,
            1000,
            [true, false, true],
            3,
            false,
            true,
        ),
    );
    g.check(
        "login/4",
        &login(
            123456,
            false,
            &["minecraft:the_end"],
            8,
            2,
            2,
            [false, false, true],
            4,
            true,
            true,
        ),
    );

    let pos = positions();
    for (i, &block_pos) in pos.iter().enumerate() {
        let dimension = if i % 2 == 0 {
            id("minecraft:overworld")
        } else {
            id("custom:x")
        };
        g.check(
            &format!("set_default_spawn_position/{i}"),
            &ClientboundSetDefaultSpawnPositionPacket {
                respawn_data: RespawnData {
                    global_pos: GlobalPos {
                        dimension,
                        pos: block_pos,
                    },
                    yaw: -180.0 + i as f32 * 90.0,
                    pitch: 90.0 - i as f32 * 60.0,
                },
            },
        );
    }
    let extreme = ClientboundSetDefaultSpawnPositionPacket {
        respawn_data: RespawnData {
            global_pos: GlobalPos {
                dimension: id("minecraft:the_end"),
                pos: pos[3],
            },
            yaw: f32::NAN,
            pitch: f32::MAX,
        },
    };
    check_bits(g, "set_default_spawn_position/extreme", &extreme);
}

fn time_ticking_sounds(g: &Golden) {
    let clock = |total_ticks, partial_tick, rate| ClockNetworkState {
        total_ticks,
        partial_tick,
        rate,
    };
    g.check(
        "set_time/empty",
        &ClientboundSetTimePacket {
            game_time: 0,
            clock_updates: vec![],
        },
    );
    g.check(
        "set_time/neg_time",
        &ClientboundSetTimePacket {
            game_time: -1,
            clock_updates: vec![],
        },
    );
    for i in 0..2 {
        g.check(
            &format!("set_time/one_{i}"),
            &ClientboundSetTimePacket {
                game_time: 123456789012,
                clock_updates: vec![(i, clock(1000 * (i as i64 + 1), 0.25 * i as f32, 1.0))],
            },
        );
    }
    let all = ClientboundSetTimePacket {
        game_time: i64::MAX,
        clock_updates: vec![
            (0, clock(i64::MAX, f32::NAN, 0.0)),
            (1, clock(-6, -0.0, 1.5)),
        ],
    };
    check_bits(g, "set_time/all", &all);
    g.check(
        "set_time/stopped",
        &ClientboundSetTimePacket {
            game_time: 24000,
            clock_updates: vec![(0, clock(0, 0.0, 0.0))],
        },
    );

    for (label, f) in [
        ("20.0", 20.0),
        ("0.1", 0.1),
        ("10000.0", 10000.0),
        ("0.0", 0.0),
    ] {
        g.check(
            &format!("ticking_state/{label}"),
            &ClientboundTickingStatePacket {
                tick_rate: f,
                is_frozen: f == 20.0 || f == 0.0,
            },
        );
    }
    g.check(
        "ticking_state/frozen",
        &ClientboundTickingStatePacket {
            tick_rate: 20.0,
            is_frozen: true,
        },
    );
    for v in [0, 1, 100, i32::MAX, -1] {
        g.check(
            &format!("ticking_step/{v}"),
            &ClientboundTickingStepPacket { tick_steps: v },
        );
    }

    use SoundSource::*;
    let srcs = [
        Master, Music, Records, Weather, Blocks, Hostile, Neutral, Players, Ambient, Voice, Ui,
    ];
    let names = [
        "MASTER", "MUSIC", "RECORDS", "WEATHER", "BLOCKS", "HOSTILE", "NEUTRAL", "PLAYERS",
        "AMBIENT", "VOICE", "UI",
    ];
    let reference = |i: usize| SoundEventHolder::Reference(SoundEvent::from_id(i as u32).unwrap());
    let direct = [
        SoundEventHolder::Direct(DirectSoundEvent {
            location: id("custom:thing/ping"),
            fixed_range: None,
        }),
        SoundEventHolder::Direct(DirectSoundEvent {
            location: id("minecraft:entity.pig.ambient"),
            fixed_range: Some(16.5),
        }),
        SoundEventHolder::Direct(DirectSoundEvent {
            location: id("x:x"),
            fixed_range: Some(-0.0),
        }),
    ];
    let sounds = SoundEvent::registry().len();
    let s_ids = [0, 1, 200, 1000, sounds - 1];
    for (i, &sid) in s_ids.iter().enumerate() {
        g.check(
            &format!("sound/ref_{sid}"),
            &ClientboundSoundPacket::new(
                reference(sid),
                srcs[i % srcs.len()],
                0.0,
                64.0,
                0.0,
                1.0,
                1.0,
                0,
            ),
        );
    }
    for (src, name) in srcs.into_iter().zip(names) {
        g.check(
            &format!("sound/source_{name}"),
            &ClientboundSoundPacket::new(
                reference(5),
                src,
                -1234.567,
                70.125,
                1.0E7,
                0.5,
                2.0,
                -987654321,
            ),
        );
    }
    for (i, d) in direct.iter().enumerate() {
        g.check(
            &format!("sound/direct_{i}"),
            &ClientboundSoundPacket::new(
                d.clone(),
                srcs[i],
                12.9,
                -64.9,
                -0.01,
                3.4028235E38,
                0.5,
                i64::MAX,
            ),
        );
    }
    // fixed point saturates / NaN -> 0 like Java's (int) cast
    let extreme = ClientboundSoundPacket::new(
        reference(2),
        Ui,
        3.0e9,
        -3.0e9,
        f64::NAN,
        f32::NAN,
        -0.0,
        i64::MIN,
    );
    assert_eq!((extreme.x, extreme.y, extreme.z), (i32::MAX, i32::MIN, 0));
    check_bits(g, "sound/extreme_pos", &extreme);

    for (i, &sid) in s_ids.iter().enumerate() {
        g.check(
            &format!("sound_entity/ref_{sid}"),
            &ClientboundSoundEntityPacket {
                sound: reference(sid),
                source: srcs[(i * 3) % srcs.len()],
                id: i as i32 * 100000 - 1,
                volume: 1.0,
                pitch: 0.5 + i as f32,
                seed: 42 * i as i64,
            },
        );
    }
    for (i, d) in direct.iter().enumerate() {
        g.check(
            &format!("sound_entity/direct_{i}"),
            &ClientboundSoundEntityPacket {
                sound: d.clone(),
                source: srcs[10 - i],
                id: i32::MAX,
                volume: f32::MAX,
                pitch: f32::from_bits(1),
                seed: -1,
            },
        );
    }

    let named = id("minecraft:entity.pig.ambient");
    g.check(
        "stop_sound/none",
        &ClientboundStopSoundPacket {
            name: None,
            source: None,
        },
    );
    g.check(
        "stop_sound/name",
        &ClientboundStopSoundPacket {
            name: Some(named.clone()),
            source: None,
        },
    );
    g.check(
        "stop_sound/name_custom",
        &ClientboundStopSoundPacket {
            name: Some(id("my_pack:a/b.c-d")),
            source: None,
        },
    );
    for (src, name) in srcs.into_iter().zip(names) {
        g.check(
            &format!("stop_sound/source_{name}"),
            &ClientboundStopSoundPacket {
                name: None,
                source: Some(src),
            },
        );
        g.check(
            &format!("stop_sound/both_{name}"),
            &ClientboundStopSoundPacket {
                name: Some(named.clone()),
                source: Some(src),
            },
        );
    }
}

#[test]
fn golden() {
    let g = Golden::load("level");
    blocks(&g);
    section_blocks_update(&g);
    chunks_and_light(&g);
    events_and_border(&g);
    login_respawn_spawn(&g);
    time_ticking_sounds(&g);
    g.assert_all_used();
}

/// Packets with NaN fields (never `==`): our bytes equal vanilla's, and vanilla's bytes re-encode to themselves.
fn check_bits<P: mc_protocol::Packet>(g: &Golden, label: &str, p: &P) {
    let expected = g.bytes(label);
    let mut ours = Vec::new();
    mc_protocol::codec::write_varint(&mut ours, P::ID);
    p.encode(&mut ours).unwrap();
    assert_eq!(hex(&ours), hex(expected), "encode {label}");
    let mut input = expected;
    assert_eq!(mc_protocol::codec::read_varint(&mut input).unwrap(), P::ID);
    let decoded = P::decode(&mut input).unwrap_or_else(|e| panic!("decode {label}: {e}"));
    assert!(input.is_empty());
    assert_eq!(
        hex(&to_bytes(&decoded).unwrap()),
        hex(&to_bytes(p).unwrap()),
        "re-encode {label}"
    );
}

// ---- decode limits and bad ids --------------------------------------------------------------------------

fn msg(s: impl Into<String>) -> Result<(), DecodeError> {
    Err(DecodeError::msg(s))
}

#[test]
fn decode_limits() {
    // chunk data / biome buffer: byteArray(2097152)
    let over = bytes(2097153, 1);
    let mut chunk = vec![0u8]; // no heightmaps
    chunk.extend(to_bytes(&over).unwrap());
    chunk.push(0);
    assert_eq!(
        from_bytes::<ClientboundLevelChunkPacketData>(&chunk).map(|_| ()),
        msg("ByteArray with size 2097153 is bigger than allowed 2097152")
    );
    let at_limit = bytes(2097152, 1);
    let mut ok = vec![0u8];
    ok.extend(to_bytes(&at_limit).unwrap());
    ok.push(0);
    assert_eq!(
        from_bytes::<ClientboundLevelChunkPacketData>(&ok)
            .unwrap()
            .buffer
            .len(),
        2097152
    );
    let mut biomes = to_bytes(&ChunkPos::new(1, 2)).unwrap();
    biomes.extend(to_bytes(&over).unwrap());
    assert_eq!(
        from_bytes::<ChunkBiomeData>(&biomes).map(|_| ()),
        msg("ByteArray with size 2097153 is bigger than allowed 2097152")
    );

    // light layers: byteArray(2048)
    let mut light = vec![0u8, 0, 0, 0, 1];
    light.extend(to_bytes(&bytes(2049, 1)).unwrap());
    light.push(0);
    assert_eq!(
        from_bytes::<ClientboundLightUpdatePacketData>(&light).map(|_| ()),
        msg("ByteArray with size 2049 is bigger than allowed 2048")
    );
    let mut light_ok = vec![0u8, 0, 0, 0, 1];
    light_ok.extend(to_bytes(&bytes(2048, 1)).unwrap());
    light_ok.push(0);
    assert_eq!(
        from_bytes::<ClientboundLightUpdatePacketData>(&light_ok)
            .unwrap()
            .sky_updates
            .len(),
        1
    );

    // heightmap long[] larger than the readable bytes
    assert_eq!(
        from_bytes::<ClientboundLevelChunkPacketData>(&[1, 1, 100]).map(|_| ()),
        msg("LongArray with size 100 is bigger than allowed 0")
    );
    // a negative collection size fails allocation, like vanilla
    assert_eq!(
        from_bytes::<ClientboundChunksBiomesPacket>(&[0xff, 0xff, 0xff, 0xff, 0x0f]).map(|_| ()),
        msg("Illegal Capacity: -1")
    );
    // truncated input never panics
    let full = to_bytes(&ClientboundSectionBlocksUpdatePacket {
        section_pos: SectionPos::of(1, 2, 3),
        changes: vec![(1, state(2)), (3, state(4))],
    })
    .unwrap();
    for n in 0..full.len() {
        assert!(from_bytes::<ClientboundSectionBlocksUpdatePacket>(&full[..n]).is_err());
    }
}

#[test]
fn bad_ids() {
    // unknown registry ids: `No value with id N`
    let mut b = to_bytes(&BlockPos::ZERO).unwrap();
    b.extend_from_slice(&[0, 0]);
    b.extend(to_bytes(&mc_protocol::codec::VarInt(BLOCK_COUNT as i32)).unwrap());
    // `ByteBufCodecs.registry(BLOCK)` on the defaulted block registry: unknown ids read as air
    assert_eq!(
        from_bytes::<ClientboundBlockEventPacket>(&b).unwrap().block,
        mc_data::Block::AIR
    );
    let mut b = to_bytes(&BlockPos::ZERO).unwrap();
    b.extend(to_bytes(&mc_protocol::codec::VarInt(STATE_COUNT as i32)).unwrap());
    assert_eq!(
        from_bytes::<ClientboundBlockUpdatePacket>(&b).map(|_| ()),
        msg(format!("No value with id {STATE_COUNT}"))
    );
    let mut b = to_bytes(&BlockPos::ZERO).unwrap();
    b.extend(to_bytes(&mc_protocol::codec::VarInt(-1)).unwrap());
    assert_eq!(
        from_bytes::<ClientboundAddTransientBlockPacket>(&b).map(|_| ()),
        msg("No value with id -1")
    );
    let n = BlockEntityType::registry().len() as i32;
    let mut b = to_bytes(&BlockPos::ZERO).unwrap();
    b.extend(to_bytes(&mc_protocol::codec::VarInt(n)).unwrap());
    b.push(10);
    b.push(0);
    assert_eq!(
        from_bytes::<ClientboundBlockEntityDataPacket>(&b).map(|_| ()),
        msg(format!("No value with id {n}"))
    );
    // an unknown state id inside a section update
    let mut b = to_bytes(&SectionPos::of(0, 0, 0)).unwrap();
    b.push(1);
    mc_protocol::codec::write_varlong(&mut b, (STATE_COUNT as i64) << 12);
    assert_eq!(
        from_bytes::<ClientboundSectionBlocksUpdatePacket>(&b).map(|_| ()),
        msg(format!("No value with id {STATE_COUNT}"))
    );

    // `readEnum`: unknown SoundSource ordinal
    assert_eq!(
        from_bytes::<SoundSource>(&[11]),
        Err(DecodeError::msg("Index 11 out of bounds for length 11"))
    );
    assert_eq!(from_bytes::<SoundSource>(&[10]), Ok(SoundSource::Ui));
    // unknown sound registry id (`id - 1`)
    let mut b = to_bytes(&mc_protocol::codec::VarInt(
        SoundEvent::registry().len() as i32 + 1,
    ))
    .unwrap();
    b.push(0);
    assert_eq!(
        from_bytes::<ClientboundSoundEntityPacket>(&b).map(|_| ()),
        msg(format!("No value with id {}", SoundEvent::registry().len()))
    );
    // by-id enums wrap/zero like `ByIdMap`
    assert_eq!(
        from_bytes::<HeightmapTypes>(&[9]),
        Ok(HeightmapTypes::WorldSurfaceWg)
    );
    assert_eq!(
        from_bytes::<ClientboundChangeDifficultyPacket>(&[5, 1])
            .unwrap()
            .difficulty,
        Difficulty::Easy
    );
    // game event ids are not validated; a compound tag must be a compound
    assert_eq!(
        from_bytes::<ClientboundGameEventPacket>(&[200, 0, 0, 0, 0])
            .unwrap()
            .event,
        200
    );
    let mut b = to_bytes(&BlockPos::ZERO).unwrap();
    b.push(0);
    b.push(8); // a string tag as the root
    assert!(from_bytes::<ClientboundBlockEntityDataPacket>(&b).is_err());
}

#[test]
fn nbt_quota() {
    // block entity update tags are trusted (unlimited quota), chunk block entity tags use the default 2 MiB quota
    let mut big = CompoundTag::new();
    big.put_byte_array("a", vec![0; 3_000_000]);
    let packet = ClientboundBlockEntityDataPacket {
        pos: BlockPos::ZERO,
        r#type: BlockEntityType::from_id(0).unwrap(),
        tag: big.clone(),
    };
    assert_eq!(
        from_bytes::<ClientboundBlockEntityDataPacket>(&to_bytes(&packet).unwrap())
            .unwrap()
            .tag,
        big
    );
    let info = BlockEntityInfo {
        packed_xz: 0,
        y: 0,
        r#type: BlockEntityType::from_id(0).unwrap(),
        tag: Some(big),
    };
    assert!(from_bytes::<BlockEntityInfo>(&to_bytes(&info).unwrap()).is_err());
}
