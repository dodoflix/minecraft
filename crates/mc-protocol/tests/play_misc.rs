//! Golden tests for the `misc` play packets against the real vanilla codecs (fixtures/play/GenMisc.java).
mod common;
use common::*;

use mc_core::ident::Identifier;
use mc_core::pos::{block_pos::BlockPos, chunk_pos::ChunkPos, vec3i::Vec3i};
use mc_data::DataComponentType;
use mc_data::{Block, CustomStat, EntityType, Item};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::codec::{DecodeError, Either, Instant, RgbColor, VarInt, from_bytes, to_bytes};
use mc_protocol::components::{
    DataComponentPatch, ItemStackTemplate,
    values::{DAMAGE, MAX_STACK_SIZE},
};
use mc_protocol::packets::play::*;
use mc_protocol::{Decode, Packet};
use mc_text::{ChatFormatting, Component, FontDescription, Style, TranslatableArg};
use uuid::Uuid;

const NON_BMP: &str = "\u{1F600} smile \u{e9}";

fn u1() -> Uuid {
    Uuid::from_u64_pair(0x0123456789abcdef, 0xfedcba9876543210)
}
fn u2() -> Uuid {
    Uuid::from_u64_pair(u64::MAX, 1 << 63)
}
fn u3() -> Uuid {
    Uuid::nil()
}
fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}
fn blk(n: &str) -> Block {
    Block::by_name(n).unwrap()
}
fn item(n: &str) -> Item {
    Item::by_name(n).unwrap()
}
fn ent(n: &str) -> EntityType {
    EntityType::by_name(n).unwrap()
}
fn plain() -> Component {
    Component::literal("Ender Dragon")
}
fn styled() -> Component {
    Component::translatable("entity.minecraft.wither", vec![])
        .with_style_fn(|s| s.with_color_rgb(0xff5555).with_bold(Some(true)))
        .append(Component::literal(NON_BMP))
}

/// For NaN payloads (`NaN != NaN`): our encoding equals vanilla's and vanilla's bytes re-encode to themselves.
fn check_nan<P: Packet + std::fmt::Debug>(g: &Golden, label: &str, p: &P) {
    let expected = g.bytes(label);
    let mut ours = Vec::new();
    mc_protocol::codec::write_varint(&mut ours, P::ID);
    p.encode(&mut ours);
    assert_eq!(hex(&ours), hex(expected), "encode {label}");
    let mut input = expected;
    assert_eq!(mc_protocol::codec::read_varint(&mut input).unwrap(), P::ID);
    let decoded = P::decode(&mut input).unwrap();
    assert!(input.is_empty());
    let mut again = Vec::new();
    mc_protocol::codec::write_varint(&mut again, P::ID);
    decoded.encode(&mut again);
    assert_eq!(hex(&again), hex(expected), "re-encode {label}");
}

fn award_stats(g: &Golden) {
    let v = VarInt;
    g.check(
        "award_stats/empty",
        &ClientboundAwardStatsPacket { stats: vec![] },
    );
    g.check(
        "award_stats/all",
        &ClientboundAwardStatsPacket {
            stats: vec![
                (Stat::Mined(blk("stone")), v(5)),
                (Stat::Crafted(item("diamond_pickaxe")), v(0)),
                (Stat::Used(item("ender_pearl")), v(300)),
                (Stat::Broken(item("iron_sword")), v(1)),
                (Stat::PickedUp(item("dirt")), v(70000)),
                (Stat::Dropped(item("cobblestone")), v(16384)),
                (Stat::Killed(ent("zombie")), v(12)),
                (Stat::KilledBy(ent("creeper")), v(2)),
                (
                    Stat::Custom(CustomStat::by_name("jump").unwrap()),
                    v(1234567),
                ),
                (
                    Stat::Custom(CustomStat::by_name("play_time").unwrap()),
                    v(-1),
                ),
                (Stat::Mined(blk("air")), v(i32::MAX)),
                (Stat::Mined(blk("reinforced_deepslate")), v(i32::MIN)),
            ],
        },
    );
    for (label, stat) in [
        ("block_mined", Stat::Mined(blk("diamond_ore"))),
        ("entity_killed", Stat::Killed(ent("player"))),
        (
            "custom_first",
            Stat::Custom(CustomStat::by_name("leave_game").unwrap()),
        ),
    ] {
        g.check(
            &format!("award_stats/one_{label}"),
            &ClientboundAwardStatsPacket {
                stats: vec![(stat, v(42))],
            },
        );
    }
}

const COLORS: [(&str, BossBarColor); 7] = [
    ("PINK", BossBarColor::Pink),
    ("BLUE", BossBarColor::Blue),
    ("RED", BossBarColor::Red),
    ("GREEN", BossBarColor::Green),
    ("YELLOW", BossBarColor::Yellow),
    ("PURPLE", BossBarColor::Purple),
    ("WHITE", BossBarColor::White),
];
const OVERLAYS: [(&str, BossBarOverlay); 5] = [
    ("PROGRESS", BossBarOverlay::Progress),
    ("NOTCHED_6", BossBarOverlay::Notched6),
    ("NOTCHED_10", BossBarOverlay::Notched10),
    ("NOTCHED_12", BossBarOverlay::Notched12),
    ("NOTCHED_20", BossBarOverlay::Notched20),
];

fn props(bits: u8) -> BossEventProperties {
    BossEventProperties {
        darken_screen: bits & 1 != 0,
        play_music: bits & 2 != 0,
        create_world_fog: bits & 4 != 0,
    }
}

fn boss(id: Uuid, operation: BossEventOperation) -> ClientboundBossEventPacket {
    ClientboundBossEventPacket { id, operation }
}

fn boss_event(g: &Golden) {
    let mut n = 0;
    for (cn, color) in COLORS {
        for (on, overlay) in OVERLAYS {
            let op = BossEventOperation::Add {
                name: if n % 2 == 0 { plain() } else { styled() },
                progress: 0.25 * (n % 5) as f32,
                color,
                overlay,
                properties: BossEventProperties {
                    darken_screen: n % 2 == 0,
                    play_music: n % 3 == 0,
                    create_world_fog: n % 5 == 0,
                },
            };
            g.check(&format!("boss_event/add_{cn}_{on}"), &boss(u1(), op));
            n += 1;
        }
    }
    g.check("boss_event/remove", &boss(u2(), BossEventOperation::Remove));
    g.check(
        "boss_event/remove_zero_uuid",
        &boss(u3(), BossEventOperation::Remove),
    );
    for p in [0f32, 1.0, 0.333, -1.5, f32::NAN, f32::INFINITY, 1e-45] {
        let packet = boss(u2(), BossEventOperation::UpdateProgress { progress: p });
        let label = format!("boss_event/progress_{}", p.to_bits() as i32);
        if p.is_nan() {
            check_nan(g, &label, &packet)
        } else {
            g.check(&label, &packet)
        }
    }
    g.check(
        "boss_event/name_styled",
        &boss(u2(), BossEventOperation::UpdateName { name: styled() }),
    );
    g.check(
        "boss_event/name_empty",
        &boss(
            u2(),
            BossEventOperation::UpdateName {
                name: Component::empty(),
            },
        ),
    );
    for (i, (name, color)) in COLORS.into_iter().enumerate() {
        let overlay = OVERLAYS[i % 5].1;
        g.check(
            &format!("boss_event/style_{name}"),
            &boss(u2(), BossEventOperation::UpdateStyle { color, overlay }),
        );
    }
    for bits in 0..8 {
        g.check(
            &format!("boss_event/properties_{bits}"),
            &boss(
                u2(),
                BossEventOperation::UpdateProperties {
                    properties: props(bits),
                },
            ),
        );
    }
}

fn combat_and_simple(g: &Golden) {
    for d in [0, 1, 127, 128, 20000, i32::MAX, -1] {
        g.check(
            &format!("player_combat_end/{d}"),
            &ClientboundPlayerCombatEndPacket { duration: d },
        );
    }
    g.check("player_combat_enter", &ClientboundPlayerCombatEnterPacket);
    g.check(
        "player_combat_kill/plain",
        &ClientboundPlayerCombatKillPacket {
            player_id: 7,
            message: Component::translatable(
                "death.attack.generic",
                vec![TranslatableArg::String("Steve".into())],
            ),
        },
    );
    g.check(
        "player_combat_kill/big_id_styled",
        &ClientboundPlayerCombatKillPacket {
            player_id: i32::MAX,
            message: styled(),
        },
    );
    g.check(
        "player_combat_kill/negative_empty",
        &ClientboundPlayerCombatKillPacket {
            player_id: -1,
            message: Component::empty(),
        },
    );

    g.check(
        "reset_score/owner_only",
        &ClientboundResetScorePacket {
            owner: "Steve".into(),
            objective_name: None,
        },
    );
    g.check(
        "reset_score/with_objective",
        &ClientboundResetScorePacket {
            owner: "Steve".into(),
            objective_name: Some("kills".into()),
        },
    );
    g.check(
        "reset_score/unicode",
        &ClientboundResetScorePacket {
            owner: NON_BMP.into(),
            objective_name: Some(NON_BMP.into()),
        },
    );
    g.check(
        "reset_score/empty",
        &ClientboundResetScorePacket {
            owner: "".into(),
            objective_name: Some("".into()),
        },
    );

    g.check(
        "select_advancements_tab/none",
        &ClientboundSelectAdvancementsTabPacket { tab: None },
    );
    g.check(
        "select_advancements_tab/tab",
        &ClientboundSelectAdvancementsTabPacket {
            tab: Some(id("minecraft:story/root")),
        },
    );
    g.check(
        "select_advancements_tab/custom_ns",
        &ClientboundSelectAdvancementsTabPacket {
            tab: Some(id("my_pack:a/b/c.d-e_f")),
        },
    );

    g.check(
        "server_data/no_icon",
        &ClientboundServerDataPacket {
            motd: plain(),
            icon_bytes: None,
        },
    );
    g.check(
        "server_data/empty_icon",
        &ClientboundServerDataPacket {
            motd: styled(),
            icon_bytes: Some(vec![]),
        },
    );
    let icon: Vec<u8> = (0..1000usize).map(|i| (i * 31) as u8).collect();
    g.check(
        "server_data/icon",
        &ClientboundServerDataPacket {
            motd: Component::literal(NON_BMP),
            icon_bytes: Some(icon),
        },
    );

    let slots = [
        ("LIST", DisplaySlot::List),
        ("SIDEBAR", DisplaySlot::Sidebar),
        ("BELOW_NAME", DisplaySlot::BelowName),
        ("TEAM_BLACK", DisplaySlot::TeamBlack),
        ("TEAM_DARK_BLUE", DisplaySlot::TeamDarkBlue),
        ("TEAM_DARK_GREEN", DisplaySlot::TeamDarkGreen),
        ("TEAM_DARK_AQUA", DisplaySlot::TeamDarkAqua),
        ("TEAM_DARK_RED", DisplaySlot::TeamDarkRed),
        ("TEAM_DARK_PURPLE", DisplaySlot::TeamDarkPurple),
        ("TEAM_GOLD", DisplaySlot::TeamGold),
        ("TEAM_GRAY", DisplaySlot::TeamGray),
        ("TEAM_DARK_GRAY", DisplaySlot::TeamDarkGray),
        ("TEAM_BLUE", DisplaySlot::TeamBlue),
        ("TEAM_GREEN", DisplaySlot::TeamGreen),
        ("TEAM_AQUA", DisplaySlot::TeamAqua),
        ("TEAM_RED", DisplaySlot::TeamRed),
        ("TEAM_LIGHT_PURPLE", DisplaySlot::TeamLightPurple),
        ("TEAM_YELLOW", DisplaySlot::TeamYellow),
        ("TEAM_WHITE", DisplaySlot::TeamWhite),
    ];
    for (name, slot) in slots {
        g.check(
            &format!("set_display_objective/{name}"),
            &ClientboundSetDisplayObjectivePacket {
                slot,
                objective_name: "kills".into(),
            },
        );
    }
    g.check(
        "set_display_objective/cleared",
        &ClientboundSetDisplayObjectivePacket {
            slot: DisplaySlot::Sidebar,
            objective_name: "".into(),
        },
    );

    // Java passes (progress, total, level)
    for (p, total, level) in [
        (0f32, 0, 0),
        (0.5, 100, 5),
        (1.0, i32::MAX, i32::MAX),
        (f32::NAN, -1, -1),
        (0.123, 21863, 30),
    ] {
        let packet = ClientboundSetExperiencePacket {
            experience_progress: p,
            experience_level: level,
            total_experience: total,
        };
        let label = format!("set_experience/{}_{total}", p.to_bits() as i32);
        if p.is_nan() {
            check_nan(g, &label, &packet)
        } else {
            g.check(&label, &packet)
        }
    }
    for (health, food, saturation) in [
        (20f32, 20, 5f32),
        (0.0, 0, 0.0),
        (0.5, 1, 20.25),
        (f32::MAX, -1, -3.0),
        (f32::NAN, i32::MAX, f32::NEG_INFINITY),
    ] {
        let packet = ClientboundSetHealthPacket {
            health,
            food,
            saturation,
        };
        let label = format!("set_health/{}_{food}", health.to_bits() as i32);
        if health.is_nan() {
            check_nan(g, &label, &packet)
        } else {
            g.check(&label, &packet)
        }
    }
}

fn number_formats() -> Vec<Option<NumberFormat>> {
    let rich = Style::EMPTY
        .with_bold(Some(true))
        .with_italic(Some(false))
        .with_underlined(Some(true))
        .with_strikethrough(Some(true))
        .with_obfuscated(Some(true))
        .with_color_rgb(0x123456)
        .with_shadow_color(0x80ff00ffu32 as i32)
        .with_insertion(Some("ins".into()))
        .with_font(Some(FontDescription::Resource(id("minecraft:uniform"))));
    vec![
        None,
        Some(NumberFormat::Blank),
        Some(NumberFormat::Styled(Style::EMPTY)),
        Some(NumberFormat::Styled(
            Style::EMPTY.with_color_formatting(Some(ChatFormatting::Red)),
        )),
        Some(NumberFormat::Styled(
            Style::EMPTY.with_color_formatting(Some(ChatFormatting::Yellow)),
        )),
        Some(NumberFormat::Styled(rich)),
        Some(NumberFormat::Fixed(Component::literal("fixed"))),
        Some(NumberFormat::Fixed(styled())),
        Some(NumberFormat::Fixed(Component::empty())),
    ]
}

fn scoreboard(g: &Golden) {
    for (k, nf) in number_formats().into_iter().enumerate() {
        for (rn, render_type) in [
            ("INTEGER", ObjectiveRenderType::Integer),
            ("HEARTS", ObjectiveRenderType::Hearts),
        ] {
            let settings = ObjectiveSettings {
                display_name: if k % 2 == 0 { plain() } else { styled() },
                render_type,
                number_format: nf.clone(),
            };
            let objective_name = format!("o{k}");
            g.check(
                &format!("set_objective/add_{k}_{rn}"),
                &ClientboundSetObjectivePacket {
                    objective_name: objective_name.clone(),
                    method: ObjectiveMethod::Add(settings.clone()),
                },
            );
            g.check(
                &format!("set_objective/change_{k}_{rn}"),
                &ClientboundSetObjectivePacket {
                    objective_name,
                    method: ObjectiveMethod::Change(settings),
                },
            );
        }
    }
    let removed = |method| ClientboundSetObjectivePacket {
        objective_name: "removed".into(),
        method,
    };
    g.check("set_objective/remove", &removed(ObjectiveMethod::Remove));
    g.check(
        "set_objective/method_3",
        &removed(ObjectiveMethod::Other(3)),
    );
    g.check(
        "set_objective/method_127",
        &removed(ObjectiveMethod::Other(127)),
    );
    g.check(
        "set_objective/method_neg",
        &removed(ObjectiveMethod::Other(-1)),
    );

    let score =
        |owner: &str, objective: &str, score, display, number_format| ClientboundSetScorePacket {
            owner: owner.into(),
            objective_name: objective.into(),
            score,
            display,
            number_format,
        };
    g.check("set_score/minimal", &score("Steve", "kills", 0, None, None));
    g.check(
        "set_score/display",
        &score("Steve", "kills", 5, Some(styled()), None),
    );
    g.check(
        "set_score/format_blank",
        &score("Alex", "deaths", -1, None, Some(NumberFormat::Blank)),
    );
    let red = NumberFormat::Styled(Style::EMPTY.with_color_formatting(Some(ChatFormatting::Red)));
    g.check(
        "set_score/format_styled",
        &score("Alex", "deaths", 200, None, Some(red)),
    );
    g.check(
        "set_score/all",
        &score(
            NON_BMP,
            NON_BMP,
            i32::MIN,
            Some(plain()),
            Some(NumberFormat::Fixed(styled())),
        ),
    );
    g.check(
        "set_score/max",
        &score(
            "#fake",
            "x",
            i32::MAX,
            Some(Component::empty()),
            Some(NumberFormat::Fixed(Component::empty())),
        ),
    );
}

fn teams(g: &Golden) {
    let strs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let team = |name: &str, method| ClientboundSetPlayerTeamPacket {
        name: name.into(),
        method,
    };
    let default_params = TeamParameters {
        display_name: styled(),
        player_prefix: Component::literal("[R] "),
        player_suffix: Component::literal(NON_BMP),
        name_tag_visibility: TeamVisibility::Always,
        collision_rule: TeamCollisionRule::Always,
        color: None,
        options: 3,
    };
    g.check(
        "set_player_team/add_default",
        &team(
            "red_team",
            TeamMethod::Add {
                parameters: default_params.clone(),
                players: strs(&["a", "b", "c"]),
            },
        ),
    );
    g.check(
        "set_player_team/change_default",
        &team(
            "red_team",
            TeamMethod::Change {
                parameters: default_params,
            },
        ),
    );
    g.check(
        "set_player_team/remove",
        &team("red_team", TeamMethod::Remove),
    );
    g.check(
        "set_player_team/join",
        &team(
            "red_team",
            TeamMethod::Join {
                players: strs(&[NON_BMP]),
            },
        ),
    );
    g.check(
        "set_player_team/leave",
        &team(
            "red_team",
            TeamMethod::Leave {
                players: strs(&["b"]),
            },
        ),
    );

    let all_colors = [
        ("BLACK", TeamColor::Black),
        ("DARK_BLUE", TeamColor::DarkBlue),
        ("DARK_GREEN", TeamColor::DarkGreen),
        ("DARK_AQUA", TeamColor::DarkAqua),
        ("DARK_RED", TeamColor::DarkRed),
        ("DARK_PURPLE", TeamColor::DarkPurple),
        ("GOLD", TeamColor::Gold),
        ("GRAY", TeamColor::Gray),
        ("DARK_GRAY", TeamColor::DarkGray),
        ("BLUE", TeamColor::Blue),
        ("GREEN", TeamColor::Green),
        ("AQUA", TeamColor::Aqua),
        ("RED", TeamColor::Red),
        ("LIGHT_PURPLE", TeamColor::LightPurple),
        ("YELLOW", TeamColor::Yellow),
        ("WHITE", TeamColor::White),
    ];
    let vis = [
        TeamVisibility::Always,
        TeamVisibility::Never,
        TeamVisibility::HideForOtherTeams,
        TeamVisibility::HideForOwnTeam,
    ];
    let coll = [
        TeamCollisionRule::Always,
        TeamCollisionRule::Never,
        TeamCollisionRule::PushOtherTeams,
        TeamCollisionRule::PushOwnTeam,
    ];
    for (i, (name, color)) in all_colors.into_iter().enumerate() {
        let parameters = TeamParameters {
            display_name: if i % 2 == 0 { plain() } else { styled() },
            player_prefix: Component::literal(format!("p{i}")),
            player_suffix: Component::empty(),
            name_tag_visibility: vis[i % 4],
            collision_rule: coll[(i / 2) % 4],
            color: Some(color),
            options: i as i8,
        };
        let t = format!("t{i}");
        g.check(
            &format!("set_player_team/change_color_{name}"),
            &team(
                &t,
                TeamMethod::Change {
                    parameters: parameters.clone(),
                },
            ),
        );
        let players = if i == 0 { vec![] } else { strs(&["p1", "p2"]) };
        g.check(
            &format!("set_player_team/add_color_{name}"),
            &team(
                &t,
                TeamMethod::Add {
                    parameters,
                    players,
                },
            ),
        );
    }
    let names = [
        "ALWAYS",
        "NEVER",
        "HIDE_FOR_OTHER_TEAMS",
        "HIDE_FOR_OWN_TEAM",
    ];
    let coll_names = ["ALWAYS", "NEVER", "PUSH_OTHER_TEAMS", "PUSH_OWN_TEAM"];
    for (vi, v) in vis.into_iter().enumerate() {
        for (ci, c) in coll.into_iter().enumerate() {
            let parameters = TeamParameters {
                display_name: plain(),
                player_prefix: plain(),
                player_suffix: plain(),
                name_tag_visibility: v,
                collision_rule: c,
                color: None,
                options: -1,
            };
            g.check(
                &format!("set_player_team/change_{}_{}", names[vi], coll_names[ci]),
                &team("v", TeamMethod::Change { parameters }),
            );
        }
    }
    let big = TeamParameters {
        display_name: plain(),
        player_prefix: plain(),
        player_suffix: plain(),
        name_tag_visibility: TeamVisibility::Always,
        collision_rule: TeamCollisionRule::Always,
        color: None,
        options: 3,
    };
    g.check(
        "set_player_team/add_many_players",
        &team(
            "big",
            TeamMethod::Add {
                parameters: big,
                players: strs(&["one", NON_BMP, "", "four", "five"]),
            },
        ),
    );
    g.check(
        "set_player_team/join_many",
        &team(
            "big",
            TeamMethod::Join {
                players: strs(&["x", "y"]),
            },
        ),
    );
    g.check(
        "set_player_team/leave_many",
        &team(
            "big",
            TeamMethod::Leave {
                players: strs(&["x", "y", "z"]),
            },
        ),
    );
    g.check(
        "set_player_team/join_none",
        &team("big", TeamMethod::Join { players: vec![] }),
    );
    g.check(
        "set_player_team/method_5",
        &team("big", TeamMethod::Other(5)),
    );
    g.check(
        "set_player_team/method_neg",
        &team("big", TeamMethod::Other(-128)),
    );
    g.check(
        "set_player_team/remove_named",
        &team(NON_BMP, TeamMethod::Remove),
    );
}

fn tag_queries(g: &Golden) {
    let mut tag = CompoundTag::new();
    tag.put_int("a", 1);
    tag.put("s", Tag::String(NON_BMP.into()));
    let mut inner = CompoundTag::new();
    inner.put_long_array("l", vec![1, -2, i64::MAX]);
    inner.put_byte_array("b", vec![1, 2, 3]);
    inner.put_int_array("i", vec![]);
    tag.put("inner", Tag::Compound(inner));
    let q = |transaction_id, tag| ClientboundTagQueryPacket {
        transaction_id,
        tag,
    };
    g.check("tag_query/empty_compound", &q(0, Some(CompoundTag::new())));
    g.check("tag_query/compound", &q(300, Some(tag.clone())));
    g.check("tag_query/null", &q(i32::MAX, None));
    g.check("tag_query/negative_id", &q(-1, Some(tag)));

    g.check(
        "seen_advancements/opened_tab",
        &ServerboundSeenAdvancementsPacket::OpenedTab(id("minecraft:story/root")),
    );
    g.check(
        "seen_advancements/opened_tab_custom",
        &ServerboundSeenAdvancementsPacket::OpenedTab(id("my_pack:a/b")),
    );
    g.check(
        "seen_advancements/closed_screen",
        &ServerboundSeenAdvancementsPacket::ClosedScreen,
    );
    for (t, e) in [(0, 0), (1, -1), (300, 123456), (i32::MAX, 0), (-1, 0)] {
        g.check(
            &format!("entity_tag_query/{t}"),
            &ServerboundEntityTagQueryPacket {
                transaction_id: t,
                entity_id: e,
            },
        );
    }
    for pos in [
        BlockPos::new(0, 0, 0),
        BlockPos::new(1, 2, 3),
        BlockPos::new(-30000000, -2048, 29999999),
        BlockPos::new(33554431, 2047, -33554432),
    ] {
        g.check(
            &format!("block_entity_tag_query/{}_{}", pos.x(), pos.y()),
            &ServerboundBlockEntityTagQueryPacket {
                transaction_id: pos.x() & 1023,
                pos,
            },
        );
    }
}

fn waypoints(g: &Golden) {
    let default = WaypointIcon::null();
    let style = WaypointIcon {
        style: id("minecraft:bowtie"),
        color: None,
    };
    let color = WaypointIcon {
        style: id("minecraft:default"),
        color: Some(RgbColor(0xFF123456u32 as i32)),
    };
    let both = WaypointIcon {
        style: id("my_pack:fancy"),
        color: Some(RgbColor(-1)),
    };
    let wp = |operation, uuid, icon: &WaypointIcon, data| ClientboundTrackedWaypointPacket {
        operation,
        waypoint: TrackedWaypoint {
            identifier: Either::Left(uuid),
            icon: icon.clone(),
            data,
        },
    };
    for (s, icon) in [
        ("default", &default),
        ("style", &style),
        ("color", &color),
        ("both", &both),
    ] {
        use WaypointOperation::{Track, Update};
        let vec3i = |x, y, z| TrackedWaypointData::Vec3i(Vec3i::new(x, y, z));
        g.check(
            &format!("waypoint/track_vec3i_{s}"),
            &wp(Track, u1(), icon, vec3i(10, -64, -30000000)),
        );
        g.check(
            &format!("waypoint/update_vec3i_{s}"),
            &wp(Update, u2(), icon, vec3i(i32::MAX, 320, i32::MIN)),
        );
        g.check(
            &format!("waypoint/track_chunk_{s}"),
            &wp(
                Track,
                u1(),
                icon,
                TrackedWaypointData::Chunk(ChunkPos::new(-5, 1000000)),
            ),
        );
        g.check(
            &format!("waypoint/update_chunk_{s}"),
            &wp(
                Update,
                u3(),
                icon,
                TrackedWaypointData::Chunk(ChunkPos::new(0, 0)),
            ),
        );
        g.check(
            &format!("waypoint/track_azimuth_{s}"),
            &wp(Track, u1(), icon, TrackedWaypointData::Azimuth(1.5707964)),
        );
        g.check(
            &format!("waypoint/update_azimuth_{s}"),
            &wp(Update, u2(), icon, TrackedWaypointData::Azimuth(-3.1415927)),
        );
    }
    let untrack = |uuid| ClientboundTrackedWaypointPacket {
        operation: WaypointOperation::Untrack,
        waypoint: TrackedWaypoint::empty(uuid),
    };
    g.check("waypoint/untrack", &untrack(u1()));
    g.check("waypoint/untrack_zero", &untrack(u3()));
    check_nan(
        g,
        "waypoint/azimuth_nan",
        &wp(
            WaypointOperation::Track,
            u1(),
            &default,
            TrackedWaypointData::Azimuth(f32::NAN),
        ),
    );
    let named =
        |operation, name: &str, icon: &WaypointIcon, data| ClientboundTrackedWaypointPacket {
            operation,
            waypoint: TrackedWaypoint {
                identifier: Either::Right(name.into()),
                icon: icon.clone(),
                data,
            },
        };
    g.check(
        "waypoint/named_empty_icon",
        &named(
            WaypointOperation::Update,
            "Steve",
            &both,
            TrackedWaypointData::Empty,
        ),
    );
    g.check(
        "waypoint/named_vec3i",
        &named(
            WaypointOperation::Track,
            NON_BMP,
            &style,
            TrackedWaypointData::Vec3i(Vec3i::new(1, 2, 3)),
        ),
    );
    g.check(
        "waypoint/named_chunk",
        &named(
            WaypointOperation::Track,
            "",
            &color,
            TrackedWaypointData::Chunk(ChunkPos::new(-1, -2)),
        ),
    );
    g.check(
        "waypoint/named_azimuth",
        &named(
            WaypointOperation::Untrack,
            "n",
            &default,
            TrackedWaypointData::Azimuth(0.5),
        ),
    );
}

fn advancements(g: &Golden) {
    let template = |n: &str, count| {
        ItemStackTemplate::new(item(n), count, DataComponentPatch::empty()).unwrap()
    };
    let patch = DataComponentPatch::builder()
        .set(MAX_STACK_SIZE, 16)
        .set(DAMAGE, 7)
        .remove(DataComponentType::by_name("max_damage").unwrap())
        .build();
    let tr = |key: &str| Component::translatable(key, vec![]);
    let reqs = |r: &[&[&str]]| AdvancementRequirements {
        requirements: r
            .iter()
            .map(|c| c.iter().map(|s| s.to_string()).collect())
            .collect(),
    };
    let holder = |id_: &str, parent: Option<&str>, display, requirements, sends_telemetry_event| {
        AdvancementHolder {
            id: id(id_),
            value: Advancement {
                parent: parent.map(id),
                display,
                requirements,
                sends_telemetry_event,
            },
        }
    };
    let root = holder(
        "minecraft:story/root",
        None,
        Some(DisplayInfo {
            icon: template("grass_block", 1),
            title: tr("advancements.story.root.title"),
            description: tr("advancements.story.root.description"),
            background: Some(id("minecraft:gui/advancements/backgrounds/stone")),
            advancement_type: AdvancementType::Task,
            show_toast: true,
            hidden: false,
        }),
        reqs(&[&["crafting_table"]]),
        false,
    );
    let child_display = DisplayInfo {
        icon: template("wooden_pickaxe", 1),
        title: plain(),
        description: styled(),
        background: None,
        advancement_type: AdvancementType::Challenge,
        show_toast: false,
        hidden: true,
    };
    let child = holder(
        "minecraft:story/mine_stone",
        Some("minecraft:story/root"),
        Some(child_display.clone()),
        reqs(&[&["a", "b"], &["c"]]),
        true,
    );
    let hidden = holder(
        "my_pack:x",
        Some("minecraft:story/root"),
        None,
        reqs(&[]),
        false,
    );
    let goal_icon = ItemStackTemplate::new(item("diamond"), 64, patch.clone()).unwrap();
    let goal = holder(
        "minecraft:story/goal",
        Some("minecraft:story/root"),
        Some(DisplayInfo {
            icon: goal_icon.clone(),
            title: styled(),
            description: Component::empty(),
            background: None,
            advancement_type: AdvancementType::Goal,
            show_toast: true,
            hidden: false,
        }),
        reqs(&[&[]]),
        true,
    );
    let all_flags = holder(
        "minecraft:story/all_flags",
        Some("minecraft:story/root"),
        Some(DisplayInfo {
            icon: template("stick", 2),
            title: plain(),
            description: plain(),
            background: None,
            advancement_type: AdvancementType::Task,
            show_toast: true,
            hidden: true,
        }),
        reqs(&[]),
        false,
    );
    let pos = |advancement, x, y| PositionedAdvancement { advancement, x, y };
    let packet = |should_reset, added, removed, progress, show_advancements| {
        ClientboundUpdateAdvancementsPacket {
            should_reset,
            added,
            removed,
            progress,
            show_advancements,
        }
    };
    g.check(
        "update_advancements/empty",
        &packet(false, vec![], vec![], vec![], false),
    );
    g.check(
        "update_advancements/reset_show",
        &packet(true, vec![], vec![], vec![], true),
    );
    g.check(
        "update_advancements/root",
        &packet(
            false,
            vec![pos(root.clone(), 0.0, 0.0)],
            vec![],
            vec![],
            true,
        ),
    );
    g.check(
        "update_advancements/child",
        &packet(
            false,
            vec![pos(child.clone(), 1.5, -2.25)],
            vec![],
            vec![],
            false,
        ),
    );
    check_nan(
        g,
        "update_advancements/hidden",
        &packet(
            false,
            vec![pos(hidden, f32::NAN, f32::MAX)],
            vec![],
            vec![],
            false,
        ),
    );
    g.check(
        "update_advancements/goal_all",
        &packet(
            true,
            vec![pos(goal.clone(), 3.0, 4.0), pos(all_flags, -0.5, 1e-10)],
            vec![],
            vec![],
            true,
        ),
    );
    let removed = vec![
        id("minecraft:story/root"),
        id("my_pack:b/c"),
        id("minecraft:nether/x"),
    ];
    let ts = Instant(1700000000123);
    let crit = |name: &str, obtained| (name.to_string(), CriterionProgress { obtained });
    let progress = vec![
        (
            id("minecraft:story/root"),
            AdvancementProgress {
                criteria: vec![crit("crafting_table", Some(ts))],
            },
        ),
        (
            id("minecraft:story/mine_stone"),
            AdvancementProgress {
                criteria: vec![
                    crit("a", Some(ts)),
                    crit("b", None),
                    crit("c", Some(Instant(-1))),
                ],
            },
        ),
        (id("my_pack:empty"), AdvancementProgress::default()),
        (
            id("my_pack:big"),
            AdvancementProgress {
                criteria: vec![
                    crit(NON_BMP, Some(Instant(i64::MAX))),
                    crit("z", Some(Instant(i64::MIN))),
                ],
            },
        ),
    ];
    g.check(
        "update_advancements/removed",
        &packet(false, vec![], removed.clone(), vec![], false),
    );
    g.check(
        "update_advancements/progress",
        &packet(false, vec![], vec![], progress.clone(), false),
    );
    g.check(
        "update_advancements/everything",
        &packet(
            true,
            vec![
                pos(root, 0.0, 0.0),
                pos(child, 1.0, 0.0),
                pos(goal, 2.0, 0.0),
            ],
            removed,
            progress,
            true,
        ),
    );
    let many = (0..200)
        .map(|j| {
            pos(
                holder(&format!("big:adv{j}"), None, None, reqs(&[]), j % 2 == 0),
                j as f32,
                -j as f32,
            )
        })
        .collect();
    g.check(
        "update_advancements/many",
        &packet(false, many, vec![], vec![], false),
    );
    g.check_value("display_info/standalone", &child_display);
    g.check_value("item_stack_template/diamond_patch", &goal_icon);
}

#[test]
fn golden() {
    let g = Golden::load("misc");
    award_stats(&g);
    boss_event(&g);
    combat_and_simple(&g);
    scoreboard(&g);
    teams(&g);
    tag_queries(&g);
    waypoints(&g);
    advancements(&g);
    g.assert_all_used();
}

fn err<T: Decode + std::fmt::Debug>(bytes: &[u8]) -> String {
    match from_bytes::<T>(bytes) {
        Err(DecodeError::Message(m)) => m,
        other => panic!("expected a decode error message, got {other:?}"),
    }
}

fn utf(s: &str) -> Vec<u8> {
    to_bytes(s)
}

#[test]
fn string_limits() {
    // every string here is `readUtf()` = 32767 UTF-16 units; a non-BMP char counts as two
    let ok = "a".repeat(32767);
    let mut bytes = utf(&ok);
    bytes.push(0);
    assert_eq!(
        from_bytes::<ClientboundResetScorePacket>(&bytes)
            .unwrap()
            .owner,
        ok
    );
    let over = "a".repeat(32768);
    let mut bytes = utf(&over);
    bytes.push(0);
    assert_eq!(
        err::<ClientboundResetScorePacket>(&bytes),
        "The received string length is longer than maximum allowed (32768 > 32767)"
    );
    let emoji = "\u{1F600}".repeat(16384); // 32768 UTF-16 units
    let mut bytes = utf(&emoji);
    bytes.push(0);
    assert_eq!(
        err::<ClientboundResetScorePacket>(&bytes),
        "The received string length is longer than maximum allowed (32768 > 32767)"
    );
    // encoded byte length above 3 * max is rejected before reading
    let mut bytes = mc_protocol::codec::to_bytes(&VarInt(98302));
    bytes.extend(std::iter::repeat(b'a').take(98302));
    assert_eq!(
        err::<ClientboundSetDisplayObjectivePacket>(&[vec![0], bytes].concat()),
        "The received encoded string buffer length is longer than maximum allowed (98302 > 98301)"
    );
    // tab / objective / team strings share the limit
    let mut bytes = utf(&over);
    bytes.push(0);
    assert!(from_bytes::<ClientboundSetPlayerTeamPacket>(&bytes).is_err());
    let mut bytes = utf(&over);
    bytes.push(0);
    assert!(from_bytes::<ClientboundSetScorePacket>(&bytes).is_err());
}

#[test]
fn enum_and_registry_ids() {
    let mut b = vec![];
    b.extend(u1().as_bytes());
    b.push(6);
    assert_eq!(
        err::<ClientboundBossEventPacket>(&b),
        "Index 6 out of bounds for length 6"
    );
    b.pop();
    b.extend([4u8, 0x0a]); // UPDATE_STYLE, color 10
    assert_eq!(
        err::<ClientboundBossEventPacket>(&b),
        "Index 10 out of bounds for length 7"
    );
    let mut b = u1().as_bytes().to_vec();
    b.extend([4, 0, 5]); // UPDATE_STYLE, pink, overlay 5
    assert_eq!(
        err::<ClientboundBossEventPacket>(&b),
        "Index 5 out of bounds for length 5"
    );
    let mut b = u1().as_bytes().to_vec();
    b.extend([0xff, 0xff, 0xff, 0xff, 0x0f]); // operation type -1
    assert_eq!(
        err::<ClientboundBossEventPacket>(&b),
        "Index -1 out of bounds for length 6"
    );

    assert_eq!(
        err::<ServerboundSeenAdvancementsPacket>(&[2]),
        "Index 2 out of bounds for length 2"
    );
    // ObjectiveCriteria.RenderType through set_objective(add): name, method 0, display name "", render type 2
    let mut b = utf("o");
    b.push(0); // method add
    b.extend(to_bytes(&Component::empty()));
    b.push(2);
    assert_eq!(
        err::<ClientboundSetObjectivePacket>(&b),
        "Index 2 out of bounds for length 2"
    );
    // NumberFormat registry id
    let mut b = utf("n");
    b.push(0); // method add
    b.extend(to_bytes(&Component::empty()));
    b.extend([0, 1, 3]); // render type integer, number format present, format type 3
    assert_eq!(
        err::<ClientboundSetObjectivePacket>(&b),
        "No value with id 3"
    );
    // stat type / value registry ids
    assert_eq!(
        err::<ClientboundAwardStatsPacket>(&[1, 9]),
        "No value with id 9"
    );
    assert_eq!(
        err::<ClientboundAwardStatsPacket>(&[1, 8, 0xff, 0x7f]),
        "No value with id 16383"
    );
    // defaulted registries (block, item, entity type) fall back to their default entry like `DefaultedRegistry.byId`
    assert_eq!(
        from_bytes::<ClientboundAwardStatsPacket>(&[1, 0, 0xff, 0x7f, 5])
            .unwrap()
            .stats,
        vec![(Stat::Mined(blk("air")), VarInt(5))]
    );
    // waypoint type
    let mut b = to_bytes(&Either::<Uuid, String>::Left(u1()));
    b.extend(to_bytes(&WaypointIcon::null()));
    b.push(4);
    assert_eq!(
        err::<TrackedWaypoint>(&b),
        "Index 4 out of bounds for length 4"
    );
}

#[test]
fn by_id_map_strategies() {
    // WRAP = floorMod, ZERO = out of range -> first constant
    assert_eq!(WaypointOperation::by_id(3), WaypointOperation::Track);
    assert_eq!(WaypointOperation::by_id(-1), WaypointOperation::Update);
    assert_eq!(
        from_bytes::<ClientboundTrackedWaypointPacket>(&{
            let mut b = vec![4]; // 4 mod 3 = 1 = UNTRACK
            b.extend(to_bytes(&TrackedWaypoint::empty(u1())));
            b
        })
        .unwrap()
        .operation,
        WaypointOperation::Untrack
    );
    assert_eq!(DisplaySlot::by_id(19), DisplaySlot::List);
    assert_eq!(DisplaySlot::by_id(-1), DisplaySlot::List);
    assert_eq!(TeamColor::by_id(16), TeamColor::Black);
    assert_eq!(TeamVisibility::by_id(4), TeamVisibility::Always);
    assert_eq!(TeamCollisionRule::by_id(-3), TeamCollisionRule::Always);
}

#[test]
fn nbt_and_collections() {
    // a non-compound tag in tag_query
    let mut b = vec![1];
    b.extend(to_bytes(&Tag::Int(5)));
    assert_eq!(
        err::<ClientboundTagQueryPacket>(&b),
        "Not a compound tag: 5"
    );
    // huge claimed counts must fail by running out of bytes, not by allocating
    assert!(from_bytes::<ClientboundAwardStatsPacket>(&[0xff, 0xff, 0xff, 0xff, 0x07]).is_err());
    assert!(
        from_bytes::<ClientboundSetPlayerTeamPacket>(&{
            let mut b = utf("t");
            b.push(3);
            b.extend([0xff, 0xff, 0xff, 0xff, 0x07]);
            b
        })
        .is_err()
    );
    // negative count: Illegal Capacity like `new ArrayList<>(-1)`
    assert_eq!(
        err::<ClientboundAwardStatsPacket>(&[0xff, 0xff, 0xff, 0xff, 0x0f]),
        "Illegal Capacity: -1"
    );
    // server icon larger than the remaining frame
    let mut b = to_bytes(&Component::empty());
    b.extend([1, 100, 1, 2, 3]);
    assert_eq!(
        err::<ClientboundServerDataPacket>(&b),
        "ByteArray with size 100 is bigger than allowed 4"
    );
    // truncated input never panics
    for p in 0..8 {
        let _ = from_bytes::<ClientboundBossEventPacket>(&vec![0u8; p]);
    }
}
