//! Serverbound play packets (group `Serverbound`) against the real vanilla `GameProtocols` codecs.
//! Fixture: `tests/fixtures/play/serverbound.golden` (generator: `GenServerbound.java`).

use crate::common::*;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::vec3i::Vec3i;
use mc_core::rotation::Rotation;
use mc_protocol::codec::read_varint;
use mc_protocol::packets::play::*;
use mc_protocol::{Encode, Packet};
use uuid::Uuid;

const POSITIONS: [(i32, i32, i32); 6] = [
    (0, 0, 0),
    (1, 2, 3),
    (-1, -2, -3),
    (33554431, 2047, 33554431),
    (-33554432, -2048, -33554432),
    (30000000, 319, -30000000),
];
const DOUBLES: [f64; 13] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    123456.789,
    -30000000.5,
    1.0E300,
    f64::from_bits(1),
    f64::MAX,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
];
const FLOATS: [f32; 12] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    90.0,
    -180.5,
    359.99,
    f32::from_bits(1),
    f32::MAX,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NAN,
];
const INTS: [i32; 10] = [0, 1, -1, 127, 128, 255, 16384, 1234567, i32::MAX, i32::MIN];
const HANDS: [(&str, InteractionHand); 2] = [
    ("MAIN_HAND", InteractionHand::MainHand),
    ("OFF_HAND", InteractionHand::OffHand),
];
const DIRECTIONS: [(&str, Direction); 6] = [
    ("DOWN", Direction::Down),
    ("UP", Direction::Up),
    ("NORTH", Direction::North),
    ("SOUTH", Direction::South),
    ("WEST", Direction::West),
    ("EAST", Direction::East),
];

fn pos(i: usize) -> BlockPos {
    let (x, y, z) = POSITIONS[i % 6];
    BlockPos::new(x, y, z)
}

fn strings() -> [String; 5] {
    [
        "".into(),
        "a".into(),
        "hello world".into(),
        "\u{e9}\u{4e2d}\u{1f600} \u{a7}ctext".into(),
        "x".repeat(300),
    ]
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

/// `check` for packets whose float fields may be NaN (`PartialEq` fails): encoding equals vanilla's and decoding vanilla's
/// bytes re-encodes to the same bytes.
fn check_bits<P: Packet + PartialEq + std::fmt::Debug>(g: &Golden, label: &str, p: &P) {
    #[expect(
        clippy::eq_op,
        reason = "NaN != NaN: self-comparison detects NaN-carrying packets"
    )]
    let comparable = p == p;
    if comparable {
        return g.check(label, p);
    }
    let expected = g.bytes(label);
    let mut ours = Vec::new();
    mc_protocol::codec::write_varint(&mut ours, P::ID);
    p.encode(&mut ours).unwrap();
    assert_eq!(hex(&ours), hex(expected), "encode {label}");
    let decoded = decode_body::<P>(expected);
    let mut again = Vec::new();
    mc_protocol::codec::write_varint(&mut again, P::ID);
    decoded.encode(&mut again).unwrap();
    assert_eq!(hex(&again), hex(expected), "re-encode {label}");
}

/// Decodes `id + body` bytes (the whole input must be consumed).
fn decode_body<P: Packet>(bytes: &[u8]) -> P {
    let mut input = bytes;
    assert_eq!(read_varint(&mut input).unwrap(), P::ID);
    let p = P::decode(&mut input).unwrap();
    assert!(input.is_empty(), "trailing bytes");
    p
}

fn text(g: &Golden, label: &str) -> String {
    String::from_utf8(g.bytes(label).to_vec()).unwrap()
}

/// A malformed packet: vanilla's root-cause exception message equals our decode error text.
fn err_case<P: Packet + std::fmt::Debug>(g: &Golden, label: &str) {
    let mut input = g.bytes(&format!("{label}.in"));
    assert_eq!(read_varint(&mut input).unwrap(), P::ID, "id {label}");
    let e = P::decode(&mut input).expect_err(label);
    assert_eq!(e.to_string(), text(g, &format!("{label}.err")), "{label}");
}

/// Truncated input: vanilla throws a Netty `IndexOutOfBoundsException` (message differs), we fail with `Eof`.
fn eof_case<P: Packet + std::fmt::Debug>(g: &Golden, label: &str) {
    let mut input = g.bytes(&format!("{label}.in"));
    read_varint(&mut input).unwrap();
    assert!(
        matches!(
            P::decode(&mut input),
            Err(mc_protocol::codec::DecodeError::Eof { .. })
        ),
        "{label}"
    );
    g.bytes(&format!("{label}.err"));
}

#[test]
fn serverbound_play_packets() {
    let g = Golden::load("serverbound");
    simple(&g);
    interact(&g);
    movement(&g);
    player(&g);
    editors(&g);
    use_item(&g);
    errors(&g);
    g.assert_all_used();
}

fn simple(g: &Golden) {
    for (i, &n) in INTS.iter().enumerate() {
        check_bits(
            g,
            &format!("accept_teleportation_{i}"),
            &ServerboundAcceptTeleportationPacket {
                id: n,
                x: DOUBLES[i],
                y: DOUBLES[(i + 3) % 13],
                z: -DOUBLES[(i + 7) % 10],
                y_rot: FLOATS[i],
                x_rot: FLOATS[(i + 5) % 9],
            },
        );
        g.check(
            &format!("attack_{i}"),
            &ServerboundAttackPacket { entity_id: n },
        );
        g.check(
            &format!("pick_item_from_entity_{i}"),
            &ServerboundPickItemFromEntityPacket {
                id: n,
                include_data: i % 2 == 1,
            },
        );
    }
    g.check("punch", &ServerboundPunchPacket);
    g.check("client_tick_end", &ServerboundClientTickEndPacket);
    g.check("player_loaded", &ServerboundPlayerLoadedPacket);
    g.check(
        "configuration_acknowledged",
        &ServerboundConfigurationAcknowledgedPacket,
    );
    for (name, d) in ["PEACEFUL", "EASY", "NORMAL", "HARD"].into_iter().zip([
        Difficulty::Peaceful,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
    ]) {
        g.check(
            &format!("change_difficulty_{name}"),
            &ServerboundChangeDifficultyPacket { difficulty: d },
        );
    }
    for (name, m) in ["SURVIVAL", "CREATIVE", "ADVENTURE", "SPECTATOR"]
        .into_iter()
        .zip([
            GameType::Survival,
            GameType::Creative,
            GameType::Adventure,
            GameType::Spectator,
        ])
    {
        g.check(
            &format!("change_game_mode_{name}"),
            &ServerboundChangeGameModePacket { mode: m },
        );
    }
    g.check(
        "lock_difficulty_true",
        &ServerboundLockDifficultyPacket { locked: true },
    );
    g.check(
        "lock_difficulty_false",
        &ServerboundLockDifficultyPacket { locked: false },
    );
    for (i, &f) in FLOATS.iter().enumerate() {
        check_bits(
            g,
            &format!("chunk_batch_received_{i}"),
            &ServerboundChunkBatchReceivedPacket {
                desired_chunks_per_tick: f,
            },
        );
    }
    for (name, a) in [
        "PERFORM_RESPAWN",
        "REQUEST_STATS",
        "REQUEST_GAMERULE_VALUES",
    ]
    .into_iter()
    .zip([
        ServerboundClientCommandAction::PerformRespawn,
        ServerboundClientCommandAction::RequestStats,
        ServerboundClientCommandAction::RequestGameruleValues,
    ]) {
        g.check(
            &format!("client_command_{name}"),
            &ServerboundClientCommandPacket { action: a },
        );
    }
    for (i, &levels) in INTS.iter().take(POSITIONS.len()).enumerate() {
        g.check(
            &format!("jigsaw_generate_{i}"),
            &ServerboundJigsawGeneratePacket {
                pos: pos(i),
                levels,
                keep_jigsaws: i % 2 == 0,
            },
        );
        g.check(
            &format!("pick_item_from_block_{i}"),
            &ServerboundPickItemFromBlockPacket {
                pos: pos(i),
                include_data: i % 2 == 1,
            },
        );
    }
    for fl in 0..4 {
        g.check(
            &format!("paddle_boat_{fl}"),
            &ServerboundPaddleBoatPacket {
                left: fl & 1 != 0,
                right: fl & 2 != 0,
            },
        );
    }
    let teleports = [
        (
            "00000000-0000-0000-0000-000000000000",
            Uuid::from_u64_pair(0, 0),
        ),
        (
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            Uuid::from_u64_pair(u64::MAX, u64::MAX),
        ),
        (
            "069a79f4-44e9-4726-a5be-fca90e38aaf5",
            Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap(),
        ),
        (
            "80000000-0000-0000-7fff-ffffffffffff",
            Uuid::from_u64_pair(1 << 63, i64::MAX as u64),
        ),
    ];
    for (name, uuid) in teleports {
        g.check(
            &format!("teleport_to_entity_{name}"),
            &ServerboundTeleportToEntityPacket { uuid },
        );
    }
    for o in [
        None,
        Some(0),
        Some(1),
        Some(-1),
        Some(300),
        Some(i32::MAX),
        Some(i32::MIN),
    ] {
        let name = o.map_or("none".to_string(), |v| v.to_string());
        let p = ServerboundSpectatorActionPacket {
            spectate_entity_id: o,
        };
        if o == Some(-1) {
            // `OptionalInt.of(-1)` encodes as 0 (= empty): lossy in vanilla too.
            let bytes = g.bytes("spectator_action_-1");
            assert_eq!(
                hex(&mc_protocol::codec::to_bytes(&p).unwrap()),
                hex(&bytes[1..]),
                "encode spectator_action_-1"
            );
            assert_eq!(
                decode_body::<ServerboundSpectatorActionPacket>(bytes).spectate_entity_id,
                None
            );
        } else {
            g.check(&format!("spectator_action_{name}"), &p);
        }
    }
}

/// `Vec3.LP_STREAM_CODEC` is lossy: our encoding of the original equals vanilla's and decoding vanilla's bytes gives
/// exactly the doubles vanilla decodes (`<label>.dec`).
fn interact(g: &Golden) {
    let locations: [[f64; 3]; 17] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [-1.0, -1.0, -1.0],
        [0.5, -0.25, 0.125],
        [3.0, -3.0, 2.0],
        [123.456, -7.5, 99.99],
        [1000.5, -2000.25, 3000.125],
        [1.0E-6, 1.0E-6, 1.0E-6],
        [3.0E-5, 0.0, 0.0],
        [4.0, 0.0, 0.0],
        [5.0, 5.0, 5.0],
        [-17179869183.0, 17179869183.0, 0.0],
        [1.0E15, -1.0E15, 1.0E15],
        [f64::NAN, 1.0, 2.0],
        [f64::INFINITY, f64::NEG_INFINITY, 0.0],
    ];
    for (i, l) in locations.iter().enumerate() {
        for (hn, h) in HANDS {
            for sec in [false, true] {
                if hn == "OFF_HAND" && !sec && i % 2 == 1 {
                    continue;
                }
                let label = format!("interact_{i}_{hn}_{sec}");
                let p = ServerboundInteractPacket {
                    entity_id: INTS[i % 10],
                    hand: h,
                    location: Vec3::new(l[0], l[1], l[2]),
                    using_secondary_action: sec,
                };
                let bytes = g.bytes(&label);
                let mut ours = Vec::new();
                mc_protocol::codec::write_varint(&mut ours, ServerboundInteractPacket::ID);
                p.encode(&mut ours).unwrap();
                assert_eq!(hex(&ours), hex(bytes), "encode {label}");
                let d = g.bytes(&format!("{label}.dec"));
                let want: Vec<u64> = d
                    .chunks(8)
                    .map(|c| u64::from_be_bytes(c.try_into().unwrap()))
                    .collect();
                let got = decode_body::<ServerboundInteractPacket>(bytes);
                assert_eq!(got.location.x.to_bits(), want[0], "dec x {label}");
                assert_eq!(got.location.y.to_bits(), want[1], "dec y {label}");
                assert_eq!(got.location.z.to_bits(), want[2], "dec z {label}");
                assert_eq!(
                    (got.entity_id, got.hand, got.using_secondary_action),
                    (p.entity_id, h, sec),
                    "{label}"
                );
            }
        }
    }
}

fn movement(g: &Golden) {
    for fl in 0..4 {
        let (og, hc) = (fl & 1 != 0, fl & 2 != 0);
        let s = ServerboundMovePlayerStatusOnlyPacket {
            on_ground: og,
            horizontal_collision: hc,
        };
        g.check(&format!("move_status_{fl}"), &s);
        assert!(!s.has_position() && !s.has_rotation());
        assert_eq!((s.is_on_ground(), s.horizontal_collision()), (og, hc));
        assert_eq!(
            (
                s.get_x(1.5),
                s.get_y(2.5),
                s.get_z(3.5),
                s.get_y_rot(4.5),
                s.get_x_rot(5.5)
            ),
            (1.5, 2.5, 3.5, 4.5, 5.5)
        );
        for i in 0..13 {
            let p = ServerboundMovePlayerPosPacket {
                x: DOUBLES[i],
                y: DOUBLES[(i + 1) % 13],
                z: DOUBLES[(i + 2) % 13],
                on_ground: og,
                horizontal_collision: hc,
            };
            check_bits(g, &format!("move_pos_{fl}_{i}"), &p);
            assert!(p.has_position() && !p.has_rotation());
            assert_eq!(
                (
                    p.get_x(9.0).to_bits(),
                    p.get_y(9.0).to_bits(),
                    p.get_z(9.0).to_bits()
                ),
                (p.x.to_bits(), p.y.to_bits(), p.z.to_bits())
            );
            assert_eq!((p.get_y_rot(7.0), p.get_x_rot(8.0)), (7.0, 8.0));
            let p = ServerboundMovePlayerPosRotPacket {
                x: DOUBLES[i],
                y: DOUBLES[(i + 4) % 13],
                z: DOUBLES[(i + 9) % 13],
                y_rot: FLOATS[i % 12],
                x_rot: FLOATS[(i + 3) % 12],
                on_ground: og,
                horizontal_collision: hc,
            };
            check_bits(g, &format!("move_pos_rot_{fl}_{i}"), &p);
            assert!(p.has_position() && p.has_rotation());
            assert_eq!(
                (p.get_y_rot(7.0).to_bits(), p.get_x_rot(8.0).to_bits()),
                (p.y_rot.to_bits(), p.x_rot.to_bits())
            );
            assert_eq!(p.get_z(9.0).to_bits(), p.z.to_bits());
        }
        for i in 0..12 {
            let p = ServerboundMovePlayerRotPacket {
                y_rot: FLOATS[i],
                x_rot: FLOATS[(i + 2) % 12],
                on_ground: og,
                horizontal_collision: hc,
            };
            check_bits(g, &format!("move_rot_{fl}_{i}"), &p);
            assert!(!p.has_position() && p.has_rotation());
            assert_eq!((p.get_x(1.0), p.get_y(2.0), p.get_z(3.0)), (1.0, 2.0, 3.0));
            assert_eq!(p.get_y_rot(7.0).to_bits(), p.y_rot.to_bits());
        }
    }
    for i in 0..13 {
        for og in [false, true] {
            check_bits(
                g,
                &format!("move_vehicle_{i}_{og}"),
                &ServerboundMoveVehiclePacket {
                    moving_to: PositionAndRotation {
                        position: Vec3::new(
                            DOUBLES[i],
                            DOUBLES[(i + 5) % 13],
                            DOUBLES[(i + 8) % 13],
                        ),
                        y_rot: FLOATS[i % 12],
                        x_rot: FLOATS[(i + 7) % 12],
                    },
                    on_ground: og,
                },
            );
        }
    }
    // The flags byte is read as an unsigned byte: only bits 0 and 1 matter (crafted bytes, vanilla semantics).
    for flags in [0u8, 1, 2, 3, 4, 0xfc, 0xfd, 0xfe, 0xff] {
        let p = mc_protocol::codec::from_bytes::<ServerboundMovePlayerStatusOnlyPacket>(&[flags])
            .unwrap();
        assert_eq!(
            (p.on_ground, p.horizontal_collision),
            (flags & 1 != 0, flags & 2 != 0)
        );
    }
}

fn player(g: &Golden) {
    g.check(
        "player_abilities_false",
        &ServerboundPlayerAbilitiesPacket { is_flying: false },
    );
    g.check(
        "player_abilities_true",
        &ServerboundPlayerAbilitiesPacket { is_flying: true },
    );
    for flags in [0, 1, 2, 3, 4, 0x7d, 0x7e, 0xff, 0x80] {
        let p = decode_body::<ServerboundPlayerAbilitiesPacket>(
            g.bytes(&format!("player_abilities_byte_{flags}")),
        );
        assert_eq!(
            p.is_flying.to_string(),
            text(g, &format!("player_abilities_byte_{flags}.flying")),
            "flags {flags}"
        );
    }

    let actions = [
        (
            "START_DESTROY_BLOCK",
            ServerboundPlayerActionAction::StartDestroyBlock,
        ),
        (
            "CHANGE_DESTROY_DIRECTION",
            ServerboundPlayerActionAction::ChangeDestroyDirection,
        ),
        (
            "ABORT_DESTROY_BLOCK",
            ServerboundPlayerActionAction::AbortDestroyBlock,
        ),
        (
            "STOP_DESTROY_BLOCK",
            ServerboundPlayerActionAction::StopDestroyBlock,
        ),
        (
            "DROP_ALL_ITEMS",
            ServerboundPlayerActionAction::DropAllItems,
        ),
        ("DROP_ITEM", ServerboundPlayerActionAction::DropItem),
        (
            "RELEASE_USE_ITEM",
            ServerboundPlayerActionAction::ReleaseUseItem,
        ),
        (
            "SWAP_ITEM_WITH_OFFHAND",
            ServerboundPlayerActionAction::SwapItemWithOffhand,
        ),
        ("STAB", ServerboundPlayerActionAction::Stab),
    ];
    let mut n = 0;
    for (an, a) in actions {
        for (dn, d) in DIRECTIONS {
            g.check(
                &format!("player_action_{an}_{dn}"),
                &ServerboundPlayerActionPacket {
                    action: a,
                    pos: pos(n),
                    direction: d,
                    sequence: INTS[n % 10],
                },
            );
            n += 1;
        }
    }
    for db in [0, 5, 6, 7, 11, 12, 100, 127, 128, 200, 255] {
        let p = decode_body::<ServerboundPlayerActionPacket>(
            g.bytes(&format!("player_action_dirbyte_{db}")),
        );
        let want: usize = text(g, &format!("player_action_dirbyte_{db}.dir"))
            .parse()
            .unwrap();
        assert_eq!(p.direction, DIRECTIONS[want].1, "dirbyte {db}");
        assert_eq!(p.sequence, 7);
    }

    let commands = [
        (
            "STOP_SLEEPING",
            ServerboundPlayerCommandAction::StopSleeping,
        ),
        (
            "START_SPRINTING",
            ServerboundPlayerCommandAction::StartSprinting,
        ),
        (
            "STOP_SPRINTING",
            ServerboundPlayerCommandAction::StopSprinting,
        ),
        (
            "START_RIDING_JUMP",
            ServerboundPlayerCommandAction::StartRidingJump,
        ),
        (
            "STOP_RIDING_JUMP",
            ServerboundPlayerCommandAction::StopRidingJump,
        ),
        (
            "OPEN_INVENTORY",
            ServerboundPlayerCommandAction::OpenInventory,
        ),
        (
            "START_FALL_FLYING",
            ServerboundPlayerCommandAction::StartFallFlying,
        ),
    ];
    let mut n = 0;
    for (an, a) in commands {
        for data in [0, 1, 100, 1 << 20, -1, i32::MAX, i32::MIN] {
            g.check(
                &format!("player_command_{an}_{n}"),
                &ServerboundPlayerCommandPacket {
                    id: INTS[n % 10],
                    action: a,
                    data,
                },
            );
            n += 1;
        }
    }

    for m in 0..128u8 {
        let b = |i: u8| m >> i & 1 != 0;
        let input = Input {
            forward: b(0),
            backward: b(1),
            left: b(2),
            right: b(3),
            jump: b(4),
            shift: b(5),
            sprint: b(6),
        };
        g.check(
            &format!("player_input_{m}"),
            &ServerboundPlayerInputPacket { input },
        );
    }
    let all = Input {
        forward: true,
        backward: true,
        left: true,
        right: true,
        jump: true,
        shift: true,
        sprint: true,
    };
    assert_eq!(
        decode_body::<ServerboundPlayerInputPacket>(g.bytes("player_input_byte_255")).input,
        all
    );
    assert_eq!(
        decode_body::<ServerboundPlayerInputPacket>(g.bytes("player_input_byte_128")).input,
        Input::default()
    );

    use mc_data::MobEffect as E;
    let effects = [E::SPEED, E::SLOWNESS, E::LUCK, E::INFESTED];
    for (i, &a) in effects.iter().enumerate() {
        for (j, &b) in effects.iter().enumerate() {
            g.check(
                &format!("set_beacon_{i}_{j}"),
                &ServerboundSetBeaconPacket {
                    primary: Some(a),
                    secondary: Some(b),
                },
            );
        }
        g.check(
            &format!("set_beacon_{i}_none"),
            &ServerboundSetBeaconPacket {
                primary: Some(a),
                secondary: None,
            },
        );
        g.check(
            &format!("set_beacon_none_{i}"),
            &ServerboundSetBeaconPacket {
                primary: None,
                secondary: Some(a),
            },
        );
    }
    g.check(
        "set_beacon_none_none",
        &ServerboundSetBeaconPacket {
            primary: None,
            secondary: None,
        },
    );
}

fn editors(g: &Golden) {
    let strings = strings();
    let modes = [
        ("SEQUENCE", CommandBlockEntityMode::Sequence),
        ("AUTO", CommandBlockEntityMode::Auto),
        ("REDSTONE", CommandBlockEntityMode::Redstone),
    ];
    let mut n = 0;
    for (mn, m) in modes {
        for fl in 0..8 {
            g.check(
                &format!("set_command_block_{mn}_{fl}"),
                &ServerboundSetCommandBlockPacket {
                    pos: pos(n),
                    command: strings[n % 5].clone(),
                    mode: m,
                    track_output: fl & 1 != 0,
                    conditional: fl & 2 != 0,
                    automatic: fl & 4 != 0,
                },
            );
            n += 1;
        }
    }
    g.check(
        "set_command_block_max",
        &ServerboundSetCommandBlockPacket {
            pos: pos(1),
            command: "c".repeat(32767),
            mode: CommandBlockEntityMode::Auto,
            track_output: true,
            conditional: true,
            automatic: true,
        },
    );
    for flags in [0u8, 7, 8, 0x78, 0xff] {
        let p = decode_body::<ServerboundSetCommandBlockPacket>(
            g.bytes(&format!("set_command_block_byte_{flags}")),
        );
        assert_eq!(
            (p.track_output, p.conditional, p.automatic),
            (flags & 1 != 0, flags & 2 != 0, flags & 4 != 0),
            "flags {flags}"
        );
        assert_eq!(
            (p.command.as_str(), p.mode),
            ("cmd", CommandBlockEntityMode::Auto)
        );
    }
    for i in 0..5 {
        g.check(
            &format!("set_command_minecart_{i}"),
            &ServerboundSetCommandMinecartPacket {
                entity: INTS[i],
                command: strings[i].clone(),
                track_output: i % 2 == 0,
            },
        );
    }
    g.check(
        "set_command_minecart_max",
        &ServerboundSetCommandMinecartPacket {
            entity: 5,
            command: "c".repeat(32767),
            track_output: false,
        },
    );

    let ids = [
        id("minecraft:empty"),
        id("my_pack:some/pool_1"),
        id("a.b:c-d/e_f"),
    ];
    let mut n = 0;
    for (jn, j) in [
        ("ROLLABLE", JigsawBlockEntityJointType::Rollable),
        ("ALIGNED", JigsawBlockEntityJointType::Aligned),
    ] {
        for i in 0..3 {
            g.check(
                &format!("set_jigsaw_block_{jn}_{i}"),
                &ServerboundSetJigsawBlockPacket {
                    pos: pos(n),
                    name: ids[i].clone(),
                    target: ids[(i + 1) % 3].clone(),
                    pool: ids[(i + 2) % 3].clone(),
                    final_state: strings[i].clone(),
                    joint: j,
                    selection_priority: INTS[i + 3],
                    placement_priority: INTS[(i * 3) % 10],
                },
            );
            n += 1;
        }
    }
    for joint in ["rollable", "aligned", "ROLLABLE", "bogus", ""] {
        let p = decode_body::<ServerboundSetJigsawBlockPacket>(
            g.bytes(&format!("set_jigsaw_block_joint_{joint}")),
        );
        let want = [
            JigsawBlockEntityJointType::Rollable,
            JigsawBlockEntityJointType::Aligned,
        ][text(g, &format!("set_jigsaw_block_joint_{joint}.joint"))
            .parse::<usize>()
            .unwrap()];
        assert_eq!(p.joint, want, "joint {joint:?}");
        assert_eq!((p.selection_priority, p.placement_priority), (3, -4));
    }

    structure_block(g, &strings);

    let slots = [("BACK", SignTextSlot::Back), ("FRONT", SignTextSlot::Front)];
    let mut n = 0;
    for (sn, s) in slots {
        for i in 0..4 {
            g.check(
                &format!("sign_update_{sn}_{i}"),
                &ServerboundSignUpdatePacket {
                    pos: pos(n),
                    lines: std::array::from_fn(|k| strings[(i + k) % 5].clone()),
                    slot: s,
                },
            );
            n += 1;
        }
    }
    g.check(
        "sign_update_max",
        &ServerboundSignUpdatePacket {
            pos: pos(1),
            lines: [
                "m".repeat(384),
                "\u{1f600}".repeat(192),
                String::new(),
                "z".into(),
            ],
            slot: SignTextSlot::Front,
        },
    );
    for label in ["sign_update_slot_2", "sign_update_slot_neg1"] {
        let p = decode_body::<ServerboundSignUpdatePacket>(g.bytes(label));
        assert_eq!(p.slot, SignTextSlot::Back, "{label}");
        assert_eq!(text(g, &format!("{label}.slot")), "0");
    }
}

fn structure_block(g: &Golden, strings: &[String; 5]) {
    let updates = [
        ("UPDATE_DATA", StructureBlockEntityUpdateType::UpdateData),
        ("SAVE_AREA", StructureBlockEntityUpdateType::SaveArea),
        ("LOAD_AREA", StructureBlockEntityUpdateType::LoadArea),
        ("SCAN_AREA", StructureBlockEntityUpdateType::ScanArea),
    ];
    let modes = [
        ("SAVE", StructureMode::Save),
        ("LOAD", StructureMode::Load),
        ("CORNER", StructureMode::Corner),
        ("DATA", StructureMode::Data),
    ];
    let mirrors = [
        ("NONE", Mirror::None),
        ("LEFT_RIGHT", Mirror::LeftRight),
        ("FRONT_BACK", Mirror::FrontBack),
    ];
    let rotations = [
        ("NONE", Rotation::None),
        ("CLOCKWISE_90", Rotation::Clockwise90),
        ("CLOCKWISE_180", Rotation::Clockwise180),
        ("COUNTERCLOCKWISE_90", Rotation::Counterclockwise90),
    ];
    let mut n = 0usize;
    for (un, u) in updates {
        for (sn, sm) in modes {
            let fl = n % 16;
            g.check(
                &format!("set_structure_block_{un}_{sn}"),
                &ServerboundSetStructureBlockPacket {
                    pos: pos(n),
                    update_type: u,
                    mode: sm,
                    name: strings[n % 5].clone(),
                    offset: BlockPos::new(n as i32 - 8, 48 - n as i32, 0),
                    size: Vec3i::new(n as i32, 48 - n as i32, 24),
                    mirror: mirrors[n % 3].1,
                    rotation: rotations[n % 4].1,
                    data: strings[(n + 1) % 4].clone(),
                    ignore_entities: fl & 1 != 0,
                    strict: fl & 8 != 0,
                    show_air: fl & 2 != 0,
                    show_bounding_box: fl & 4 != 0,
                    integrity: n as f32 / 15.0,
                    seed: n as i64 * 0x1_2345_6789 - 5,
                },
            );
            n += 1;
        }
    }
    let simple = |mirror, rotation| ServerboundSetStructureBlockPacket {
        pos: pos(2),
        update_type: StructureBlockEntityUpdateType::SaveArea,
        mode: StructureMode::Save,
        name: "s".into(),
        offset: BlockPos::new(-48, 48, 0),
        size: Vec3i::new(0, 48, 0),
        mirror,
        rotation,
        data: String::new(),
        ignore_entities: false,
        strict: false,
        show_air: false,
        show_bounding_box: false,
        integrity: 0.0,
        seed: 0,
    };
    for (mn, m) in mirrors {
        for (rn, r) in rotations {
            g.check(&format!("set_structure_block_{mn}_{rn}"), &simple(m, r));
        }
    }
    for fl in 0..16 {
        g.check(
            &format!("set_structure_block_flags_{fl}"),
            &ServerboundSetStructureBlockPacket {
                pos: pos(0),
                update_type: StructureBlockEntityUpdateType::LoadArea,
                mode: StructureMode::Load,
                name: "n".into(),
                offset: BlockPos::new(0, 0, 0),
                size: Vec3i::new(1, 2, 3),
                mirror: Mirror::None,
                rotation: Rotation::None,
                data: "d".into(),
                ignore_entities: fl & 1 != 0,
                strict: fl & 8 != 0,
                show_air: fl & 2 != 0,
                show_bounding_box: fl & 4 != 0,
                integrity: 1.0,
                seed: i64::MIN,
            },
        );
    }
    for seed in [0, 1, -1, 127, 128, i64::MAX, i64::MIN, 1 << 35] {
        g.check(
            &format!("set_structure_block_seed_{seed}"),
            &ServerboundSetStructureBlockPacket {
                pos: pos(1),
                update_type: StructureBlockEntityUpdateType::ScanArea,
                mode: StructureMode::Data,
                name: "seed".into(),
                offset: BlockPos::new(1, -1, 48),
                size: Vec3i::new(48, 0, 1),
                mirror: Mirror::LeftRight,
                rotation: Rotation::Clockwise180,
                data: "x".repeat(128),
                ignore_entities: true,
                strict: true,
                show_air: true,
                show_bounding_box: true,
                integrity: 0.5,
                seed,
            },
        );
    }
    // Decode-side clamps (offset -48..=48, size 0..=48, integrity 0..=1) and flag bytes, against what vanilla decodes.
    for b in [
        [0, 0, 0, 0, 0, 0],
        [48, 48, 48, 48, 48, 48],
        [49, 100, 127, 49, 100, 127],
        [-48, -48, -48, -1, -2, -128],
        [-49, -128, -100, -49, -128, 0],
        [255, 129, 200, 255, 130, 131],
    ] {
        let label = format!(
            "set_structure_block_bytes_{}_{}_{}_{}_{}_{}",
            b[0], b[1], b[2], b[3], b[4], b[5]
        );
        let p = decode_body::<ServerboundSetStructureBlockPacket>(g.bytes(&label));
        let got = format!(
            "{},{},{},{},{},{}",
            p.offset.x(),
            p.offset.y(),
            p.offset.z(),
            p.size.x,
            p.size.y,
            p.size.z
        );
        assert_eq!(got, text(g, &format!("{label}.dec")), "{label}");
    }
    for f in [
        -1.0f32,
        -0.0,
        0.0,
        0.25,
        1.0,
        1.5,
        f32::MAX,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
    ] {
        let label = format!("set_structure_block_integrity_{}", f.to_bits() as i32);
        let p = decode_body::<ServerboundSetStructureBlockPacket>(g.bytes(&label));
        assert_eq!(
            format!("{:x}", p.integrity.to_bits()),
            text(g, &format!("{label}.dec")),
            "{label}"
        );
    }
    for flags in [0u8, 15, 16, 0xf0, 0xff] {
        let p = decode_body::<ServerboundSetStructureBlockPacket>(
            g.bytes(&format!("set_structure_block_flagbyte_{flags}")),
        );
        assert_eq!(
            (p.ignore_entities, p.show_air, p.show_bounding_box, p.strict),
            (
                flags & 1 != 0,
                flags & 2 != 0,
                flags & 4 != 0,
                flags & 8 != 0
            ),
            "flags {flags}"
        );
    }
    for (label, key) in [
        ("set_structure_block_wrap_rotation_5", 1),
        ("set_structure_block_wrap_rotation_neg1", 3),
    ] {
        let p = decode_body::<ServerboundSetStructureBlockPacket>(g.bytes(label));
        assert_eq!(p.rotation, rotations[key].1, "{label}");
        assert_eq!(text(g, &format!("{label}.rot")), key.to_string());
    }
}

fn use_item(g: &Golden) {
    // `(float)((double)(pos + click) - pos)`: what vanilla puts on the wire for `new Vec3(pos + click)` (int + float is float math).
    fn offset(p: i32, c: f32) -> f32 {
        ((p as f32 + c) as f64 - p as f64) as f32
    }
    let clicks = [0.0f32, 0.5, 1.0, -0.25, 0.999, 1.5, f32::NAN, 1.0E-30];
    let hit = |p: BlockPos, d: Direction, cx: f32, cy: f32, cz: f32, inside, world_border_hit| {
        BlockHitResult {
            block_pos: p,
            direction: d,
            click_x: offset(p.x(), cx),
            click_y: offset(p.y(), cy),
            click_z: offset(p.z(), cz),
            inside,
            world_border_hit,
        }
    };
    let mut n = 0;
    for (dn, d) in DIRECTIONS {
        for (hn, h) in HANDS {
            for fl in 0..4 {
                let p = pos(n);
                check_bits(
                    g,
                    &format!("use_item_on_{dn}_{hn}_{fl}"),
                    &ServerboundUseItemOnPacket {
                        hand: h,
                        hit_result: hit(
                            p,
                            d,
                            clicks[n % 6],
                            clicks[(n + 1) % 6],
                            clicks[(n + 2) % 6],
                            fl & 1 != 0,
                            fl & 2 != 0,
                        ),
                        sequence: INTS[n % 10],
                    },
                );
                n += 1;
            }
        }
    }
    for i in 0..8 {
        check_bits(
            g,
            &format!("use_item_on_click_{i}"),
            &ServerboundUseItemOnPacket {
                hand: InteractionHand::MainHand,
                hit_result: hit(
                    pos(i),
                    Direction::Up,
                    clicks[i],
                    clicks[(i + 3) % 8],
                    clicks[(i + 5) % 8],
                    false,
                    false,
                ),
                sequence: 0,
            },
        );
    }
    let hr = BlockHitResult {
        block_pos: BlockPos::new(10, -5, 7),
        direction: Direction::Up,
        click_x: 0.5,
        click_y: 0.25,
        click_z: 1.0,
        inside: false,
        world_border_hit: false,
    };
    assert_eq!(hr.location(), Vec3::new(10.5, -4.75, 8.0));
    for db in [6, 7, 11, -1, -7, 600] {
        let p =
            decode_body::<ServerboundUseItemOnPacket>(g.bytes(&format!("use_item_on_dir_{db}")));
        let want: usize = text(g, &format!("use_item_on_dir_{db}.dir"))
            .parse()
            .unwrap();
        assert_eq!(p.hit_result.direction, DIRECTIONS[want].1, "dir {db}");
    }
    for hv in [2, 3, -1, 100] {
        let p =
            decode_body::<ServerboundUseItemOnPacket>(g.bytes(&format!("use_item_on_hand_{hv}")));
        assert_eq!(p.hand, InteractionHand::MainHand);
        assert_eq!(text(g, &format!("use_item_on_hand_{hv}.hand")), "0");
    }
    for (hn, h) in HANDS {
        for i in 0..12 {
            check_bits(
                g,
                &format!("use_item_{hn}_{i}"),
                &ServerboundUseItemPacket {
                    hand: h,
                    sequence: INTS[i % 10],
                    y_rot: FLOATS[i],
                    x_rot: FLOATS[(i + 4) % 12],
                },
            );
        }
    }
}

/// Bad ordinals, over-long strings, unknown registry ids, truncated input: vanilla's messages.
fn errors(g: &Golden) {
    for l in [
        "err_client_command_3",
        "err_client_command_neg",
        "err_client_command_big",
    ] {
        err_case::<ServerboundClientCommandPacket>(g, l);
    }
    for l in ["err_player_action_9", "err_player_action_neg"] {
        err_case::<ServerboundPlayerActionPacket>(g, l);
    }
    for l in ["err_player_command_7", "err_player_command_neg"] {
        err_case::<ServerboundPlayerCommandPacket>(g, l);
    }
    for l in [
        "err_set_command_block_mode_3",
        "err_set_command_block_long",
        "err_set_command_block_bytes_long",
    ] {
        err_case::<ServerboundSetCommandBlockPacket>(g, l);
    }
    err_case::<ServerboundSetCommandMinecartPacket>(g, "err_set_command_minecart_long");
    for l in [
        "err_set_jigsaw_block_bad_id",
        "err_set_jigsaw_block_state_long",
    ] {
        err_case::<ServerboundSetJigsawBlockPacket>(g, l);
    }
    for l in [
        "err_set_structure_block_update_4",
        "err_set_structure_block_mode_4",
        "err_set_structure_block_mirror_3",
        "err_set_structure_block_mirror_neg",
        "err_set_structure_block_data_129",
        "err_set_structure_block_data_bytes",
    ] {
        err_case::<ServerboundSetStructureBlockPacket>(g, l);
    }
    for l in [
        "err_sign_update_385",
        "err_sign_update_emoji_385",
        "err_sign_update_bytes",
    ] {
        err_case::<ServerboundSignUpdatePacket>(g, l);
    }
    for l in [
        "err_set_beacon_effect_unknown",
        "err_set_beacon_effect_neg",
        "err_set_beacon_second_unknown",
    ] {
        err_case::<ServerboundSetBeaconPacket>(g, l);
    }
    eof_case::<ServerboundMovePlayerPosPacket>(g, "err_truncated_move_pos");
    eof_case::<ServerboundTeleportToEntityPacket>(g, "err_teleport_to_entity_short");
}
