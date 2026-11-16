//! `ServerboundSetGameRulePacket`, `ServerboundEntityTagQueryPacket`, `ServerboundBlockEntityTagQueryPacket` and the
//! game rule request, sent by a player without and with operator rights: the replies the client sees (tag query NBT,
//! `commands.gamerule.set` system chat, the rule values) and the warnings in the server log are equal on the real
//! vanilla server and on `mc-server`.
use crate::common::*;
use mc_protocol::packets::play::*;
use mc_testbot::join::BotOptions;
use mc_testbot::log::Dir;
use mc_testbot::{Bot, harness};
use std::sync::Arc;
use std::time::Duration;

async fn join_recording(port: u16) -> Arc<Bot> {
    let mut o = BotOptions::new("127.0.0.1", port, "Bot");
    o.timeout = Duration::from_secs(120);
    o.record_raw = true;
    let bot = Bot::connect(o).await.expect("connect");
    bot.wait_joined(Duration::from_secs(60))
        .await
        .expect("login packet");
    bot.wait_ready(Duration::from_secs(90))
        .await
        .expect("player loaded");
    Arc::new(bot)
}

fn from_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The answers the packets under test provoke among the clientbound packets logged from entry `from` on.
fn replies(bot: &Bot, from: usize) -> (usize, Vec<String>) {
    let log = bot.shared.log.lock().unwrap();
    let mut out = Vec::new();
    for e in &log.entries()[from..] {
        if e.dir != Dir::In || e.state != "play" {
            continue;
        }
        let p = match ClientboundPlayPacket::decode_payload(&from_hex(e.raw.as_ref().unwrap())) {
            Ok(p) => p,
            Err(err) => {
                if matches!(
                    e.name.as_str(),
                    "tag_query" | "system_chat" | "game_rule_values"
                ) {
                    out.push(format!("undecodable {}: {err}", e.name));
                }
                continue;
            }
        };
        match p {
            ClientboundPlayPacket::TagQuery(mut q) => {
                // the entity's random UUID and its position (the player spawn offset is random) differ between two servers
                if let Some(tag) = &mut q.tag {
                    tag.remove("UUID");
                    tag.remove("Pos");
                }
                out.push(format!("tag_query {} {:?}", q.transaction_id, q.tag));
            }
            ClientboundPlayPacket::SystemChat(c) => {
                out.push(format!("system_chat {:?} {}", c.content, c.overlay));
            }
            ClientboundPlayPacket::GameRuleValues(v) => {
                let pick = |key: &str| {
                    v.values
                        .iter()
                        .find(|(k, _)| k.to_string() == key)
                        .map(|(_, v)| v.clone())
                };
                out.push(format!(
                    "game_rule_values count={} random_tick_speed={:?} advance_time={:?}",
                    v.values.len(),
                    pick("minecraft:random_tick_speed"),
                    pick("minecraft:advance_time")
                ));
            }
            _ => {}
        }
    }
    (log.entries().len(), out)
}

fn set_rules(entries: &[(&str, &str)]) -> ServerboundPlayPacket {
    ServerboundPlayPacket::SetGameRule(ServerboundSetGameRulePacket {
        entries: entries
            .iter()
            .map(|(k, v)| ServerboundSetGameRulePacketEntry {
                game_rule_key: mc_core::ident::Identifier::parse(k).unwrap(),
                value: (*v).to_owned(),
            })
            .collect(),
    })
}

fn request_rules() -> ServerboundPlayPacket {
    ServerboundPlayPacket::ClientCommand(ServerboundClientCommandPacket {
        action: ServerboundClientCommandAction::RequestGameruleValues,
    })
}

fn entity_query(transaction_id: i32, entity_id: i32) -> ServerboundPlayPacket {
    ServerboundPlayPacket::EntityTagQuery(ServerboundEntityTagQueryPacket {
        transaction_id,
        entity_id,
    })
}

fn block_query(transaction_id: i32, pos: mc_core::pos::BlockPos) -> ServerboundPlayPacket {
    ServerboundPlayPacket::BlockEntityTagQuery(ServerboundBlockEntityTagQueryPacket {
        transaction_id,
        pos,
    })
}

async fn scenario(server: &(dyn Console + Sync), port: u16) -> Vec<String> {
    let bot = join_recording(port).await;
    let mut t = Vec::new();
    let p = bot.position();
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    server.cmd(&format!("setblock {} {by} {bz} minecraft:chest", bx + 2));
    server.cmd(&format!(
        "summon minecraft:armor_stand {}.5 {by} {}.5 {{NoGravity:1b,Invisible:1b}}",
        bx - 2,
        bz
    ));
    let stand = bot
        .wait_entity(mc_data::EntityType::ARMOR_STAND, &[], T)
        .await
        .expect("armor stand");
    let chest = mc_core::pos::BlockPos::new(bx + 2, by, bz);
    let empty = mc_core::pos::BlockPos::new(bx + 4, by + 20, bz);
    let me = bot.with_state(|s| s.entity_id);
    let mut from = bot.shared.log.lock().unwrap().entries().len();

    // without operator rights: nothing is answered, the rule requests are warned about
    bot.send(set_rules(&[("minecraft:random_tick_speed", "7")]));
    bot.send(request_rules());
    bot.send(entity_query(1, me));
    bot.send(block_query(2, chest));
    server
        .wait_log(
            "Player Bot tried to set game rule values without required permissions",
            20,
        )
        .expect("set game rule warning");
    server
        .wait_log(
            "Player Bot tried to request game rule values without required permissions",
            20,
        )
        .expect("request game rules warning");
    bot.wait_ticks(20).await.expect("ticking");
    let (next, got) = replies(&bot, from);
    from = next;
    t.push(format!("non-op {got:?}"));

    // with operator rights
    server.cmd("op Bot");
    bot.wait_ticks(20).await.expect("ticking");
    from = replies(&bot, from).0;
    bot.send(set_rules(&[("minecraft:random_tick_speed", "7")]));
    bot.wait_ticks(10).await.expect("ticking");
    let (next, got) = replies(&bot, from);
    from = next;
    t.push(format!("set one rule {got:?}"));
    // unknown rule, unparsable value, trailing characters, out of range, then a valid boolean
    bot.send(set_rules(&[
        ("minecraft:nope", "1"),
        ("minecraft:random_tick_speed", "abc"),
        ("minecraft:random_tick_speed", "9x"),
        ("minecraft:random_tick_speed", "-1"),
        ("minecraft:advance_time", "false"),
    ]));
    server
        .wait_log(
            r"Received request to set unknown game rule: ResourceKey\[minecraft:game_rule / minecraft:nope\]",
            20,
        )
        .expect("unknown rule warning");
    bot.wait_ticks(10).await.expect("ticking");
    let (next, got) = replies(&bot, from);
    from = next;
    t.push(format!("set several rules {got:?}"));
    bot.send(request_rules());
    bot.wait_ticks(10).await.expect("ticking");
    let (next, got) = replies(&bot, from);
    from = next;
    t.push(format!("request rules {got:?}"));
    bot.send(entity_query(3, stand));
    bot.send(entity_query(4, 999_999));
    bot.send(block_query(6, chest));
    bot.send(block_query(7, empty));
    bot.wait_ticks(20).await.expect("ticking");
    let (_, got) = replies(&bot, from);
    t.push(format!("tag queries {got:?}"));
    bot.disconnect().await;
    t
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn admin_packets_equal_vanilla() {
    let props = ["level-type=minecraft:flat", "generate-structures=false"];
    let vanilla = harness::vanilla(1, "creative", &props)
        .start()
        .expect("start vanilla");
    let theirs = scenario(&vanilla, vanilla.port).await;
    drop(vanilla);
    let server = ours(1, "creative", &props).start().expect("start ours");
    let ours = scenario(&server, server.port).await;
    for (a, b) in theirs.iter().zip(&ours) {
        assert_eq!(a, b, "vanilla (left) vs ours (right)");
    }
    assert_eq!(theirs.len(), ours.len());
    println!("{}", theirs.join("\n"));
}
