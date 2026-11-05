//! Play packets of the `debug` group against the real vanilla codecs (`fixtures/play/GenDebug.java` -> `debug.golden`).
use crate::common::*;
use mc_core::bounding_box::BoundingBox;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::vec3::Vec3;
use mc_core::pos::{BlockPos, ChunkPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_data::{Block, DebugSubscription, GameEvent, GameRule, PointOfInterestType};
use mc_protocol::codec::{from_bytes, to_bytes, write_varint};
use mc_protocol::packets::play::*;
use mc_protocol::{Decode, Encode};
use mc_text::{ChatFormatting, Component};

fn bp(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

fn sub(name: &str) -> DebugSubscription {
    DebugSubscription::by_name(name).unwrap()
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

fn min_f32() -> f32 {
    f32::from_bits(1)
}

fn styled() -> Component {
    Component::translatable("test.status", vec!["a\u{1F600}".into(), 3.into()])
        .with_formats(&[ChatFormatting::Red, ChatFormatting::Bold])
}

/// The four clientbound carriers of one subscription value.
fn all(g: &Golden, name: &str, event: DebugSubscriptionEvent, salt: i32) {
    let update = || DebugSubscriptionUpdate::Value(event.clone());
    g.check(
        &format!("block_{name}"),
        &ClientboundDebugBlockValuePacket {
            block_pos: bp(salt, salt * 2 - 3, -salt),
            update: update(),
        },
    );
    g.check(
        &format!("chunk_{name}"),
        &ClientboundDebugChunkValuePacket {
            chunk_pos: ChunkPos::new(salt, -salt),
            update: update(),
        },
    );
    g.check(
        &format!("entity_{name}"),
        &ClientboundDebugEntityValuePacket {
            entity_id: salt * 1000,
            update: update(),
        },
    );
    g.check(
        &format!("event_{name}"),
        &ClientboundDebugEventPacket { event },
    );
}

#[allow(clippy::too_many_arguments)]
fn node(
    x: i32,
    y: i32,
    z: i32,
    walked_distance: f32,
    cost_malus: f32,
    closed: bool,
    r#type: PathType,
    f: f32,
) -> DebugPathNode {
    DebugPathNode {
        x,
        y,
        z,
        walked_distance,
        cost_malus,
        closed,
        r#type,
        f,
    }
}

fn bbox(a: [i32; 6]) -> BoundingBox {
    BoundingBox::new(a[0], a[1], a[2], a[3], a[4], a[5])
}

fn piece(a: [i32; 6], is_start: bool) -> DebugStructureInfoPiece {
    DebugStructureInfoPiece {
        bounding_box: bbox(a),
        is_start,
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn subscription_values(g: &Golden) {
    use DebugSubscriptionEvent as E;

    all(
        g,
        "bees_empty",
        E::Bees(DebugBeeInfo {
            hive_pos: None,
            flower_pos: None,
            travel_ticks: 0,
            blacklisted_hives: vec![],
        }),
        1,
    );
    all(
        &g,
        "bees_full",
        E::Bees(DebugBeeInfo {
            hive_pos: Some(bp(10, 64, -10)),
            flower_pos: Some(bp(-3, 70, 99)),
            travel_ticks: 12345,
            blacklisted_hives: vec![bp(1, 2, 3), bp(-1, -2, -3), bp(33554431, 2047, -33554432)],
        }),
        2,
    );
    all(
        g,
        "bees_hive",
        E::Bees(DebugBeeInfo {
            hive_pos: Some(bp(0, 0, 0)),
            flower_pos: None,
            travel_ticks: -1,
            blacklisted_hives: vec![bp(5, 5, 5)],
        }),
        3,
    );
    all(
        g,
        "bees_flower",
        E::Bees(DebugBeeInfo {
            hive_pos: None,
            flower_pos: Some(bp(7, -64, 7)),
            travel_ticks: i32::MAX,
            blacklisted_hives: vec![],
        }),
        4,
    );

    let brain = |name: &str,
                 profession: &str,
                 xp,
                 health,
                 max_health,
                 inventory: &str,
                 wants_golem,
                 anger_level| DebugBrainDump {
        name: name.into(),
        profession: profession.into(),
        xp,
        health,
        max_health,
        inventory: inventory.into(),
        wants_golem,
        anger_level,
        activities: vec![],
        behaviors: vec![],
        memories: vec![],
        gossips: vec![],
        pois: vec![],
        potential_pois: vec![],
    };
    all(
        g,
        "brains_min",
        E::Brains(brain("", "", 0, 0.0, 0.0, "", false, 0)),
        1,
    );
    all(
        &g,
        "brains_full",
        E::Brains(DebugBrainDump {
            activities: strs(&["core", "idle", "work"]),
            behaviors: strs(&["run_one", "\u{1F600}"]),
            memories: strs(&["home: [1, 2, 3]", "job_site: -"]),
            gossips: strs(&["Alice: major_positive: 20", "Carl: trading: 1"]),
            pois: vec![bp(1, 2, 3), bp(-4, 5, -6), bp(7, 8, 9)],
            potential_pois: vec![bp(100, 64, -100)],
            ..brain(
                "Bob \u{1F600}",
                "minecraft:farmer",
                -5,
                20.0,
                0.5,
                "1 bread, 2 wheat",
                true,
                -1,
            )
        }),
        2,
    );
    all(
        &g,
        "brains_warden",
        E::Brains(DebugBrainDump {
            activities: strs(&["fight"]),
            potential_pois: vec![bp(0, 0, 0), bp(1, 1, 1)],
            ..brain("Warden", "", 1000000, 500.0, 500.0, "", false, 4)
        }),
        3,
    );
    all(
        &g,
        "brains_extremes",
        E::Brains(DebugBrainDump {
            activities: strs(&[""]),
            behaviors: strs(&[""]),
            memories: strs(&[""]),
            gossips: strs(&[""]),
            ..brain(
                &"x".repeat(32767),
                "p",
                i32::MIN,
                min_f32(),
                f32::MAX,
                &"i".repeat(300),
                true,
                i32::MAX,
            )
        }),
        4,
    );

    let breeze = |attack_target, jump_target| {
        E::Breezes(DebugBreezeInfo {
            attack_target,
            jump_target,
        })
    };
    all(g, "breezes_empty", breeze(None, None), 1);
    all(
        g,
        "breezes_full",
        breeze(Some(300), Some(bp(-1, -1, -1))),
        2,
    );
    all(g, "breezes_target", breeze(Some(0), None), 3);
    all(g, "breezes_neg", breeze(Some(-1), Some(bp(9, 9, 9))), 4);
    all(g, "breezes_max", breeze(Some(i32::MAX), None), 5);

    let goal = |priority, is_running, name: &str| DebugGoalInfoDebugGoal {
        priority,
        is_running,
        name: name.into(),
    };
    all(
        g,
        "goals_empty",
        E::GoalSelectors(DebugGoalInfo { goals: vec![] }),
        1,
    );
    all(
        &g,
        "goals_some",
        E::GoalSelectors(DebugGoalInfo {
            goals: vec![
                goal(0, true, "FloatGoal"),
                goal(-1, false, ""),
                goal(1000, true, "\u{1F600}goal"),
                goal(i32::MAX, false, &"x".repeat(255)),
            ],
        }),
        2,
    );

    let all_types: Vec<DebugPathNode> = (0..27)
        .map(|i| {
            let malus = [
                -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -1.0, -1.0, 8.0, 8.0, 0.0, -1.0, 8.0, 16.0,
                8.0, -1.0, 0.0, -1.0, -1.0, 4.0, -1.0, 8.0, 0.0, 0.0, 0.0, 4.0,
            ];
            node(
                i,
                -i,
                i * 100,
                i as f32 * 0.5,
                malus[i as usize],
                i % 2 == 0,
                PathType::by_id(i),
                i as f32 * 1.25,
            )
        })
        .collect();
    let path = |reached, next_node_index, target, nodes, open_set, closed_set, target_nodes| {
        DebugPathInfo {
            path: DebugPath {
                reached,
                next_node_index,
                target,
                nodes,
                debug_data: DebugPathData {
                    open_set,
                    closed_set,
                    target_nodes,
                },
            },
            max_node_distance: 0.0,
        }
    };
    all(
        &g,
        "paths_empty",
        E::EntityPaths(path(
            false,
            0,
            bp(0, 0, 0),
            vec![],
            vec![],
            vec![],
            vec![node(0, 0, 0, 0.0, 0.0, false, PathType::Blocked, 0.0)],
        )),
        1,
    );
    all(
        &g,
        "paths_types",
        E::EntityPaths(DebugPathInfo {
            max_node_distance: 16.0,
            ..path(
                true,
                5,
                bp(1, 2, 3),
                all_types.clone(),
                all_types[0..3].to_vec(),
                all_types[3..9].to_vec(),
                vec![
                    node(1, 2, 3, 1.5, -1.0, true, PathType::Walkable, 7.0),
                    node(-1, -2, -3, 0.0, 8.0, false, PathType::Water, f32::MAX),
                ],
            )
        }),
        2,
    );
    all(
        &g,
        "paths_neg",
        E::EntityPaths(DebugPathInfo {
            max_node_distance: f32::MAX,
            ..path(
                false,
                -1,
                bp(-30000000, -64, 30000000),
                vec![node(
                    i32::MIN,
                    i32::MAX,
                    -1,
                    min_f32(),
                    -0.0,
                    false,
                    PathType::BigMobsCloseToDanger,
                    f32::MAX,
                )],
                vec![],
                vec![node(1, 1, 1, 0.0, 0.0, true, PathType::Open, 0.0)],
                vec![node(5, 5, 5, 2.0, 2.0, false, PathType::Fence, 2.0)],
            )
        }),
        3,
    );

    for (i, (name, x)) in [
        ("in_block", DebugEntityBlockIntersection::InBlock),
        ("in_fluid", DebugEntityBlockIntersection::InFluid),
        ("in_air", DebugEntityBlockIntersection::InAir),
    ]
    .into_iter()
    .enumerate()
    {
        all(
            g,
            &format!("intersection_{name}"),
            E::EntityBlockIntersections(x),
            1 + i as i32,
        );
    }

    let block = |name: &str| Block::by_name(name).unwrap();
    let hive = |r#type, occupant_count, honey_level, sedated| {
        E::BeeHives(DebugHiveInfo {
            r#type,
            occupant_count,
            honey_level,
            sedated,
        })
    };
    all(g, "hive_beehive", hive(block("beehive"), 3, 5, true), 1);
    all(g, "hive_nest", hive(block("bee_nest"), 0, 0, false), 2);
    all(g, "hive_other", hive(block("stone"), i32::MAX, -1, true), 3);
    all(
        g,
        "hive_lastblock",
        hive(
            Block::from_id(Block::registry().len() as u32 - 1).unwrap(),
            1,
            1,
            false,
        ),
        4,
    );

    let poi_count = PointOfInterestType::registry().len() as u32;
    let poi = |pos, id, free_ticket_count| {
        E::Pois(DebugPoiInfo {
            pos,
            poi_type: PointOfInterestType::from_id(id).unwrap(),
            free_ticket_count,
        })
    };
    all(g, "poi_first", poi(bp(1, 2, 3), 0, 0), 1);
    all(g, "poi_last", poi(bp(-1, -2, -3), poi_count - 1, 1), 2);
    all(
        g,
        "poi_tickets",
        poi(bp(33554431, 2047, -33554432), 5, i32::MAX),
        3,
    );

    let mut n = 0;
    for up in Direction::ALL {
        for front in Direction::ALL {
            if up.get_axis() == front.get_axis() {
                continue;
            }
            for side_bias in [
                RedstoneOrientationSideBias::Left,
                RedstoneOrientationSideBias::Right,
            ] {
                let o = RedstoneOrientation {
                    up,
                    front,
                    side_bias,
                };
                assert_eq!(RedstoneOrientation::from_index(o.index()), Some(o));
                all(
                    g,
                    &format!("orientation_{}", o.index()),
                    E::RedstoneWireOrientations(o),
                    1 + n % 7,
                );
                n += 1;
            }
        }
    }
    assert_eq!(n, 48);

    all(g, "village", E::VillageSections(()), 1);
    all(g, "raids_empty", E::Raids(vec![]), 1);
    all(g, "raids_one", E::Raids(vec![bp(1, 2, 3)]), 2);
    all(
        &g,
        "raids_many",
        E::Raids(vec![
            bp(1, 2, 3),
            bp(-100, 64, 100),
            bp(0, 0, 0),
            bp(33554431, 2047, -33554432),
            bp(-33554432, -2048, 33554431),
        ]),
        3,
    );

    all(g, "structures_empty", E::Structures(vec![]), 1);
    all(
        g,
        "structures_one",
        E::Structures(vec![DebugStructureInfo {
            bounding_box: bbox([0; 6]),
            pieces: vec![],
        }]),
        2,
    );
    all(
        &g,
        "structures_pieces",
        E::Structures(vec![
            DebugStructureInfo {
                bounding_box: bbox([-10, -64, -10, 10, 100, 10]),
                pieces: vec![
                    piece([-10, -64, -10, 0, 0, 0], true),
                    piece([1, 1, 1, 10, 100, 10], false),
                ],
            },
            DebugStructureInfo {
                bounding_box: bbox([100, 0, 100, 200, 50, 300]),
                pieces: vec![piece([100, 0, 100, 200, 50, 300], true)],
            },
        ]),
        3,
    );

    for (name, radius, salt) in [
        ("0", 0, 1),
        ("15", 15, 2),
        ("max", i32::MAX, 3),
        ("neg", -1, 4),
    ] {
        all(
            g,
            &format!("listener_{name}"),
            E::GameEventListeners(DebugGameEventListenerInfo {
                listener_radius: radius,
            }),
            salt,
        );
    }

    all(g, "neighbor_origin", E::NeighborUpdates(bp(0, 0, 0)), 1);
    all(g, "neighbor_neg", E::NeighborUpdates(bp(-1, -1, -1)), 2);
    all(
        g,
        "neighbor_extreme",
        E::NeighborUpdates(bp(33554431, 2047, -33554432)),
        3,
    );
    all(
        g,
        "neighbor_extreme2",
        E::NeighborUpdates(bp(-33554432, -2048, 33554431)),
        4,
    );

    let events = GameEvent::registry().len() as u32;
    for gid in [0, 1, events / 2, events - 1] {
        let pos = Vec3::new(1.5 + f64::from(gid), -64.25, 1e10);
        all(
            g,
            &format!("gameevent_{gid}"),
            E::GameEvents(DebugGameEventInfo {
                event: GameEvent::from_id(gid).unwrap(),
                pos,
            }),
            1 + gid as i32 % 5,
        );
    }
    all(
        &g,
        "gameevent_extreme",
        E::GameEvents(DebugGameEventInfo {
            event: GameEvent::from_id(2).unwrap(),
            pos: Vec3::new(-0.0, f64::from_bits(1), f64::MAX),
        }),
        6,
    );

    // empty updates for every subscription that has a value codec
    for s in DebugSubscription::iter().filter(|s| s.id() != 0) {
        let name = s.name().replace("minecraft:", "");
        let empty = || DebugSubscriptionUpdate::Empty(s);
        g.check(
            &format!("block_empty_{name}"),
            &ClientboundDebugBlockValuePacket {
                block_pos: bp(-5, 5, -5),
                update: empty(),
            },
        );
        g.check(
            &format!("chunk_empty_{name}"),
            &ClientboundDebugChunkValuePacket {
                chunk_pos: ChunkPos::new(-7, 7),
                update: empty(),
            },
        );
        g.check(
            &format!("entity_empty_{name}"),
            &ClientboundDebugEntityValuePacket {
                entity_id: -1,
                update: empty(),
            },
        );
    }
    let neighbor =
        || DebugSubscriptionUpdate::Value(DebugSubscriptionEvent::NeighborUpdates(bp(1, 1, 1)));
    g.check(
        "entity_max",
        &ClientboundDebugEntityValuePacket {
            entity_id: i32::MAX,
            update: neighbor(),
        },
    );
    g.check(
        "chunk_extreme",
        &ClientboundDebugChunkValuePacket {
            chunk_pos: ChunkPos::new(i32::MIN, i32::MAX),
            update: neighbor(),
        },
    );

    // every variant maps to its registry entry
    assert_eq!(
        DebugSubscriptionEvent::GameEvents(DebugGameEventInfo {
            event: GameEvent::from_id(0).unwrap(),
            pos: Vec3::new(0.0, 0.0, 0.0)
        })
        .subscription(),
        sub("game_events")
    );
    assert_eq!(
        DebugSubscriptionEvent::VillageSections(()).subscription(),
        sub("village_sections")
    );
}

fn samples_rules_and_game_tests(g: &Golden) {
    let sample = |sample: Vec<i64>| ClientboundDebugSamplePacket {
        sample,
        debug_sample_type: RemoteDebugSampleType::TickTime,
    };
    g.check("sample_empty", &sample(vec![]));
    g.check(
        "sample_some",
        &sample(vec![0, -1, i64::MAX, i64::MIN, 50_000_000]),
    );
    g.check(
        "sample_big",
        &sample((0..200).map(|k| k * 123456789 - 7).collect()),
    );

    let rule = |name: &str| id(&format!("minecraft:{name}"));
    g.check(
        "rules_empty",
        &ClientboundGameRuleValuesPacket { values: vec![] },
    );
    g.check(
        "rules_one",
        &ClientboundGameRuleValuesPacket {
            values: vec![(rule("advance_time"), "true".into())],
        },
    );
    g.check(
        "rules_many",
        &ClientboundGameRuleValuesPacket {
            values: vec![
                (rule("advance_time"), "false".into()),
                (rule("random_tick_speed"), "3".into()),
                (rule("max_command_sequence_length"), "-2147483648".into()),
                (rule("fire_damage"), "\u{1F600}".into()),
                (id("mod:custom/rule"), "".into()),
                (rule("block_drops"), "x".repeat(32767)),
            ],
        },
    );
    g.check(
        "rules_all",
        &ClientboundGameRuleValuesPacket {
            values: GameRule::iter()
                .map(|r| (r.identifier(), format!("v{}", r.id())))
                .collect(),
        },
    );

    g.check(
        "highlight_origin",
        &ClientboundGameTestHighlightPosPacket {
            absolute_pos: bp(0, 0, 0),
            relative_pos: bp(0, 0, 0),
        },
    );
    g.check(
        "highlight_some",
        &ClientboundGameTestHighlightPosPacket {
            absolute_pos: bp(100, 64, -100),
            relative_pos: bp(-1, -2, -3),
        },
    );
    g.check(
        "highlight_extreme",
        &ClientboundGameTestHighlightPosPacket {
            absolute_pos: bp(33554431, 2047, -33554432),
            relative_pos: bp(-33554432, -2048, 33554431),
        },
    );
    g.check(
        "status_plain",
        &ClientboundTestInstanceBlockStatus {
            status: Component::literal("Running"),
            size: None,
        },
    );
    g.check(
        "status_size",
        &ClientboundTestInstanceBlockStatus {
            status: Component::literal("\u{1F600} done"),
            size: Some(Vec3i::new(3, 4, 5)),
        },
    );
    g.check(
        "status_styled",
        &ClientboundTestInstanceBlockStatus {
            status: styled(),
            size: Some(Vec3i::new(0, 0, 0)),
        },
    );
    g.check(
        "status_neg",
        &ClientboundTestInstanceBlockStatus {
            status: Component::empty(),
            size: Some(Vec3i::new(-1, 300, i32::MAX)),
        },
    );
}

fn serverbound(g: &Golden) {
    let request = |names: &[&str]| ServerboundDebugSubscriptionRequestPacket {
        subscriptions: names.iter().map(|n| sub(n)).collect(),
    };
    g.check("subscribe_none", &request(&[]));
    g.check("subscribe_one", &request(&["bees"]));
    g.check(
        "subscribe_three",
        &request(&["bees", "dedicated_server_tick_time", "game_events"]),
    );
    g.check(
        "subscribe_all",
        &ServerboundDebugSubscriptionRequestPacket {
            subscriptions: DebugSubscription::iter().collect(),
        },
    );

    let entry = |key: Identifier, value: &str| ServerboundSetGameRulePacketEntry {
        game_rule_key: key,
        value: value.into(),
    };
    let rule = |name: &str| id(&format!("minecraft:{name}"));
    g.check(
        "setrule_empty",
        &ServerboundSetGameRulePacket { entries: vec![] },
    );
    g.check(
        "setrule_one",
        &ServerboundSetGameRulePacket {
            entries: vec![entry(rule("advance_time"), "false")],
        },
    );
    g.check(
        "setrule_many",
        &ServerboundSetGameRulePacket {
            entries: vec![
                entry(rule("random_tick_speed"), "-7"),
                entry(rule("fire_damage"), ""),
                entry(id("mod:x/y"), "\u{1F600}"),
                entry(rule("block_drops"), &"y".repeat(32767)),
            ],
        },
    );

    for (name, mode) in [
        ("start", TestBlockMode::Start),
        ("log", TestBlockMode::Log),
        ("fail", TestBlockMode::Fail),
        ("accept", TestBlockMode::Accept),
    ] {
        let p = |position, message: &str| ServerboundSetTestBlockPacket {
            position,
            mode,
            message: message.into(),
        };
        g.check(&format!("testblock_{name}_empty"), &p(bp(1, 2, 3), ""));
        g.check(
            &format!("testblock_{name}_msg"),
            &p(bp(-1, -2, -3), "hello \u{1F600} \u{e9}"),
        );
    }
    g.check(
        "testblock_long",
        &ServerboundSetTestBlockPacket {
            position: bp(33554431, 2047, -33554432),
            mode: TestBlockMode::Accept,
            message: "z".repeat(32767),
        },
    );

    use ServerboundTestInstanceBlockActionPacketAction as Action;
    let data = |test, size, rotation, ignore_entities, status, error_message| {
        TestInstanceBlockEntityData {
            test,
            size,
            rotation,
            ignore_entities,
            status,
            error_message,
        }
    };
    for (name, action) in [
        ("init", Action::Init),
        ("query", Action::Query),
        ("set", Action::Set),
        ("reset", Action::Reset),
        ("save", Action::Save),
        ("export", Action::Export),
        ("run", Action::Run),
    ] {
        let d = data(
            None,
            Vec3i::new(1, 1, 1),
            Rotation::None,
            false,
            TestInstanceBlockEntityStatus::Cleared,
            None,
        );
        g.check(
            &format!("testaction_{name}"),
            &ServerboundTestInstanceBlockActionPacket {
                pos: bp(4, 5, 6),
                action,
                data: d,
            },
        );
    }
    let mut r = 0;
    for (rname, rot) in [
        ("none", Rotation::None),
        ("clockwise_90", Rotation::Clockwise90),
        ("clockwise_180", Rotation::Clockwise180),
        ("counterclockwise_90", Rotation::Counterclockwise90),
    ] {
        for (sname, status) in [
            ("cleared", TestInstanceBlockEntityStatus::Cleared),
            ("running", TestInstanceBlockEntityStatus::Running),
            ("finished", TestInstanceBlockEntityStatus::Finished),
        ] {
            let error = match r % 3 {
                0 => Some(Component::literal("failed \u{1F600}")),
                1 => Some(styled()),
                _ => None,
            };
            let d = data(
                Some(id(&format!("mod:tests/t{r}"))),
                Vec3i::new(r, -r, 300 * r),
                rot,
                r & 1 == 0,
                status,
                error,
            );
            g.check(
                &format!("testdata_{rname}_{sname}"),
                &ServerboundTestInstanceBlockActionPacket {
                    pos: bp(-r, r, r),
                    action: Action::Set,
                    data: d,
                },
            );
            r += 1;
        }
    }
    let d = data(
        Some(id("minecraft:empty")),
        Vec3i::new(i32::MIN, i32::MAX, 0),
        Rotation::Counterclockwise90,
        true,
        TestInstanceBlockEntityStatus::Finished,
        Some(Component::empty()),
    );
    g.check(
        "testdata_extreme",
        &ServerboundTestInstanceBlockActionPacket {
            pos: bp(33554431, 2047, -33554432),
            action: Action::Run,
            data: d,
        },
    );
}

#[test]
fn golden() {
    let g = Golden::load("debug");
    subscription_values(&g);
    samples_rules_and_game_tests(&g);
    serverbound(&g);
    g.assert_all_used();
}

// ---- decode limits and unknown ids ---------------------------------------------------------------------------

fn body<T: Encode>(v: &T) -> Vec<u8> {
    to_bytes(v).unwrap()
}

/// A string as vanilla would read it, whatever its length (our encoder refuses strings over the limit).
fn utf(s: &str) -> Vec<u8> {
    let mut b = Vec::new();
    mc_protocol::codec::write_utf(&mut b, s, 1 << 30).unwrap();
    b
}

fn varint(v: i32) -> Vec<u8> {
    let mut b = Vec::new();
    write_varint(&mut b, v);
    b
}

fn err<T: Decode + std::fmt::Debug>(bytes: &[u8]) -> String {
    from_bytes::<T>(bytes).unwrap_err().to_string()
}

#[test]
fn subscription_request_limit() {
    let sub = sub("bees");
    let ok = ServerboundDebugSubscriptionRequestPacket {
        subscriptions: vec![sub; 32],
    };
    // a `ReferenceOpenHashSet`: the 32 equal elements collapse to one on decode
    assert_eq!(
        from_bytes::<ServerboundDebugSubscriptionRequestPacket>(&body(&ok)).unwrap().subscriptions,
        vec![sub]
    );
    let too_many = ServerboundDebugSubscriptionRequestPacket {
        subscriptions: vec![sub; 33],
    };
    // the encoder refuses it first (`ByteBufCodecs.collection(.., 32)`)...
    assert_eq!(
        to_bytes(&too_many).unwrap_err().to_string(),
        "33 elements exceeded max size of: 32"
    );
    // ...and the decoder rejects what arrives anyway
    let mut raw = varint(33);
    for _ in 0..33 {
        raw.extend(body(&sub));
    }
    assert_eq!(
        err::<ServerboundDebugSubscriptionRequestPacket>(&raw),
        "33 elements exceeded max size of: 32"
    );
    // unknown registry id
    let mut b = varint(1);
    b.extend(varint(DebugSubscription::registry().len() as i32));
    assert_eq!(
        err::<ServerboundDebugSubscriptionRequestPacket>(&b),
        "No value with id 16"
    );
}

#[test]
fn string_limits() {
    let long = "a".repeat(32768);
    // set_test_block message: STRING_UTF8
    let mut b = body(&bp(0, 0, 0));
    b.extend(varint(0));
    b.extend(utf(&long));
    assert_eq!(
        err::<ServerboundSetTestBlockPacket>(&b),
        "The received string length is longer than maximum allowed (32768 > 32767)"
    );
    // set_game_rule value
    let mut b = varint(1);
    b.extend(body(&id("minecraft:advance_time")));
    b.extend(utf(&long));
    assert_eq!(
        err::<ServerboundSetGameRulePacket>(&b),
        "The received string length is longer than maximum allowed (32768 > 32767)"
    );
    // game rule values
    let mut b = varint(1);
    b.extend(body(&id("minecraft:advance_time")));
    b.extend(utf(&long));
    assert_eq!(
        err::<ClientboundGameRuleValuesPacket>(&b),
        "The received string length is longer than maximum allowed (32768 > 32767)"
    );
    // DebugGoal name: stringUtf8(255), counted in UTF-16 units
    let goal = |name: &str| DebugGoalInfo {
        goals: vec![DebugGoalInfoDebugGoal {
            priority: 1,
            is_running: true,
            name: name.into(),
        }],
    };
    assert!(from_bytes::<DebugGoalInfo>(&body(&goal(&"x".repeat(255)))).is_ok());
    assert!(from_bytes::<DebugGoalInfo>(&body(&goal(&"\u{1F600}".repeat(127)))).is_ok());
    assert_eq!(
        err::<DebugGoalInfo>(&body(&goal(&"x".repeat(256)))),
        "The received string length is longer than maximum allowed (256 > 255)"
    );
    assert_eq!(
        err::<DebugGoalInfo>(&body(&goal(&"\u{1F600}".repeat(128)))),
        "The received string length is longer than maximum allowed (256 > 255)"
    );
}

#[test]
fn unknown_ids() {
    // event/update with an unknown subscription, and with `dedicated_server_tick_time` (null value codec)
    assert_eq!(
        err::<ClientboundDebugEventPacket>(&varint(16)),
        "No value with id 16"
    );
    assert_eq!(
        err::<ClientboundDebugEventPacket>(&varint(-1)),
        "No value with id -1"
    );
    assert!(err::<ClientboundDebugEventPacket>(&varint(0)).contains("dedicated_server_tick_time"));
    let mut b = body(&bp(0, 0, 0));
    b.extend(varint(0));
    b.push(0);
    assert!(err::<ClientboundDebugBlockValuePacket>(&b).contains("dedicated_server_tick_time"));
    let mut b = body(&bp(0, 0, 0));
    b.extend(varint(99));
    b.push(0);
    assert_eq!(
        err::<ClientboundDebugBlockValuePacket>(&b),
        "No value with id 99"
    );
    // redstone orientation index out of range (`ArrayIndexOutOfBoundsException`)
    for index in [48, 100, -1] {
        let mut b = varint(9);
        b.extend(varint(index));
        assert_eq!(
            err::<ClientboundDebugEventPacket>(&b),
            format!("Index {index} out of bounds for length 48")
        );
    }
    // debug sample type: `readEnum` has no out-of-bounds strategy
    let mut b = varint(0);
    b.extend(varint(1));
    assert_eq!(
        err::<ClientboundDebugSamplePacket>(&b),
        "Index 1 out of bounds for length 1"
    );
    // long array bigger than the readable bytes
    let mut b = varint(2);
    b.extend([0; 8]);
    b.extend(varint(0));
    assert_eq!(
        err::<ClientboundDebugSamplePacket>(&b),
        "LongArray with size 2 is bigger than allowed 1"
    );
    // ByIdMap.ZERO enums
    let mut b = body(&bp(0, 0, 0));
    b.extend(varint(77));
    b.extend(body(&String::new()));
    assert_eq!(
        from_bytes::<ServerboundSetTestBlockPacket>(&b)
            .unwrap()
            .mode,
        TestBlockMode::Start
    );
    assert_eq!(
        from_bytes::<DebugEntityBlockIntersection>(&varint(9)).unwrap(),
        DebugEntityBlockIntersection::InBlock
    );
    assert_eq!(
        from_bytes::<PathType>(&varint(-3)).unwrap(),
        PathType::Blocked
    );
    assert_eq!(
        from_bytes::<TestInstanceBlockEntityStatus>(&varint(3)).unwrap(),
        TestInstanceBlockEntityStatus::Cleared
    );
    assert_eq!(
        from_bytes::<ServerboundTestInstanceBlockActionPacketAction>(&varint(7)).unwrap(),
        ServerboundTestInstanceBlockActionPacketAction::Init
    );
    // Rotation.BY_ID wraps; unknown registry ids fail
    let d = TestInstanceBlockEntityData {
        test: None,
        size: Vec3i::new(0, 0, 0),
        rotation: Rotation::None,
        ignore_entities: false,
        status: TestInstanceBlockEntityStatus::Cleared,
        error_message: None,
    };
    let mut b = body(&d);
    b[4] = 6; // None(1) + size(3) -> rotation byte; 6 mod 4 = 2
    assert_eq!(
        from_bytes::<TestInstanceBlockEntityData>(&b)
            .unwrap()
            .rotation,
        Rotation::Clockwise180
    );
    let mut b = varint(Block::registry().len() as i32);
    b.extend([0, 0, 0]);
    // `ByteBufCodecs.registry(BLOCK)`: the defaulted block registry answers unknown ids with air.
    assert_eq!(from_bytes::<DebugHiveInfo>(&b).unwrap().r#type, Block::AIR);
}

#[test]
fn bounding_box_normalizes_inverted_corners() {
    // `new BoundingBox(...)` swaps inverted bounds on decode
    let mut b = body(&bp(5, 5, 5));
    b.extend(body(&bp(1, 9, 1)));
    b.push(0);
    let piece = from_bytes::<DebugStructureInfoPiece>(&b).unwrap();
    assert_eq!(piece.bounding_box, BoundingBox::new(1, 5, 1, 5, 9, 5));
}

#[test]
fn truncated_input_does_not_panic() {
    let ev = DebugSubscriptionEvent::Brains(DebugBrainDump {
        name: "n".into(),
        profession: "p".into(),
        xp: 1,
        health: 1.0,
        max_health: 2.0,
        inventory: "i".into(),
        wants_golem: true,
        anger_level: 3,
        activities: strs(&["a"]),
        behaviors: strs(&["b"]),
        memories: strs(&["c"]),
        gossips: strs(&["d"]),
        pois: vec![bp(1, 1, 1)],
        potential_pois: vec![bp(2, 2, 2)],
    });
    let b = body(&ClientboundDebugEventPacket { event: ev });
    for n in 0..b.len() {
        assert!(
            from_bytes::<ClientboundDebugEventPacket>(&b[..n]).is_err(),
            "prefix {n}"
        );
    }
}
