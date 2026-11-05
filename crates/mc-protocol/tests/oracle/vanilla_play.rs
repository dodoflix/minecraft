//! Real-network play test: an offline bot joins the vanilla server (`ref/server.jar`), stays in play for ~20 s while
//! the server console moves it to new chunks and triggers a wide range of packets, and every clientbound packet received
//! must decode (and re-encode to the same bytes); only packets carrying a not-yet-implemented data component may fail.
//! Needs java, `ref/server.jar` and `MC_ACCEPT_EULA=1` (the test fails with the fix named when one is missing).
//! Run with `-- --nocapture` for the per-packet-type counts.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::common::*;
use mc_protocol::packets::{configuration as cfg, handshake::ClientIntent, login, play};
use uuid::Uuid;

/// Console commands run (with a pause between them) once the bot is in the world: new chunks, entities, particles,
/// sounds, scoreboard/bossbar/title/chat traffic, effects, weather/time/border changes, inventory changes.
const COMMANDS: &[&str] = &[
    "tp PlayBot 3000 120 3000",
    "execute at PlayBot run summon pig ~ ~ ~",
    "execute at PlayBot run summon villager ~ ~ ~",
    "execute at PlayBot run summon zombie ~ ~ ~",
    "execute at PlayBot run summon armor_stand ~ ~ ~ {ShowArms:1b}",
    "execute at PlayBot run summon item ~ ~ ~ {Item:{id:\"minecraft:stone\",count:3}}",
    "execute at PlayBot run summon text_display ~ ~2 ~ {text:\"hi\"}",
    "execute at PlayBot run summon item_display ~ ~2 ~ {item:{id:\"minecraft:stone\",count:1}}",
    "execute at PlayBot run summon tnt ~ ~ ~",
    "execute at PlayBot run particle flame ~ ~1 ~ 0.5 0.5 0.5 0 20",
    "execute at PlayBot run particle dust{color:[1.0,0.0,0.0],scale:1.0} ~ ~1 ~ 0 0 0 0 5",
    "execute at PlayBot run particle block{block_state:\"minecraft:stone\"} ~ ~1 ~ 0 0 0 0 5",
    "execute at PlayBot run particle entity_effect{color:[0.2,0.4,0.6,1.0]} ~ ~1 ~ 0 0 0 0 3",
    "playsound minecraft:entity.pig.ambient master PlayBot",
    "stopsound PlayBot",
    "title PlayBot title {\"text\":\"hello\",\"color\":\"red\"}",
    "title PlayBot subtitle {\"text\":\"sub\"}",
    "title PlayBot actionbar {\"text\":\"bar\"}",
    "title PlayBot times 5 20 5",
    "title PlayBot clear",
    "bossbar add test:bar {\"text\":\"Boss\"}",
    "bossbar set test:bar players PlayBot",
    "bossbar set test:bar value 50",
    "bossbar set test:bar color red",
    "scoreboard objectives add obj dummy {\"text\":\"Obj\"}",
    "scoreboard objectives setdisplay sidebar obj",
    "scoreboard players set PlayBot obj 7",
    "team add red",
    "team modify red color red",
    "team join red PlayBot",
    "effect give PlayBot minecraft:speed 30 1",
    "effect give PlayBot minecraft:glowing 30",
    "experience add PlayBot 10 levels",
    "gamemode creative PlayBot",
    "give PlayBot minecraft:stone 5",
    "weather rain",
    "time set 6000",
    "worldborder set 500",
    "worldborder warning distance 3",
    "difficulty hard",
    "say hello",
    "tellraw PlayBot {\"text\":\"raw\"}",
    "advancement grant PlayBot everything",
    "recipe give PlayBot *",
    "execute at PlayBot run summon minecart ~ ~ ~",
    "execute at PlayBot run setblock ~ ~ ~2 minecraft:oak_sign{front_text:{messages:['\"a\"','\"b\"','\"c\"','\"d\"']}}",
    "execute at PlayBot run setblock ~ ~ ~3 minecraft:chest",
    "item replace entity PlayBot armor.head with minecraft:iron_helmet",
    "damage PlayBot 2",
    "scoreboard players reset PlayBot obj",
    "team leave PlayBot",
    "bossbar remove test:bar",
    "setworldspawn 10 70 10",
    "gamerule minecraft:advance_time false",
    "tp PlayBot 0 90 0",
    "gamemode survival PlayBot",
    "kill PlayBot",
];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn play_state_decodes_everything_vanilla_sends() {
    let server = start_vanilla(false);
    let port = server.port;
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    send!(c, login::ServerboundLoginPacket::Hello(login::ServerboundHelloPacket { name: "PlayBot".into(), profile_id: Uuid::nil() }));
    loop {
        let payload = read(&mut c).await;
        match login::ClientboundLoginPacket::decode_payload(&payload).unwrap() {
            login::ClientboundLoginPacket::LoginCompression(p) => c.set_compression(p.compression_threshold, false),
            login::ClientboundLoginPacket::LoginFinished(_) => break,
            other => panic!("unexpected login packet {other:?}"),
        }
    }
    send!(c, login::ServerboundLoginPacket::LoginAcknowledged(login::ServerboundLoginAcknowledgedPacket));
    send!(c, cfg::ServerboundConfigurationPacket::ClientInformation(cfg::ServerboundClientInformationPacket { information: Default::default() }));
    loop {
        let payload = read(&mut c).await;
        match cfg::ClientboundConfigurationPacket::decode_payload(&payload).unwrap() {
            cfg::ClientboundConfigurationPacket::SelectKnownPacks(_) => {
                send!(c, cfg::ServerboundConfigurationPacket::SelectKnownPacks(cfg::ServerboundSelectKnownPacks { known_packs: vec![] }))
            }
            cfg::ClientboundConfigurationPacket::KeepAlive(p) => {
                send!(c, cfg::ServerboundConfigurationPacket::KeepAlive(cfg::ServerboundKeepAlivePacket { id: p.id }))
            }
            cfg::ClientboundConfigurationPacket::Ping(p) => send!(c, cfg::ServerboundConfigurationPacket::Pong(cfg::ServerboundPongPacket { id: p.id })),
            cfg::ClientboundConfigurationPacket::FinishConfiguration(_) => {
                send!(c, cfg::ServerboundConfigurationPacket::FinishConfiguration(cfg::ServerboundFinishConfigurationPacket));
                break;
            }
            _ => {}
        }
    }

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let started = Instant::now();
    let mut next_command = 0;
    let mut next_at = Duration::from_secs(3);
    while started.elapsed() < Duration::from_secs(24) {
        if next_command < COMMANDS.len() && started.elapsed() >= next_at {
            server.cmd(COMMANDS[next_command]);
            next_command += 1;
            next_at += Duration::from_millis(300);
        }
        let Ok(Ok(payload)) = tokio::time::timeout(Duration::from_millis(100), c.read_payload()).await else { continue };
        let packet = match play::ClientboundPlayPacket::decode_payload(&payload) {
            Ok(p) => p,
            Err(e) => {
                let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap_or(-1);
                let msg = format!("decode id {id} ({} bytes): {e}", payload.len());
                failures.push(msg);
                continue;
            }
        };
        *counts.entry(packet.name()).or_default() += 1;
        let mut again = Vec::new();
        packet.encode_payload(&mut again).unwrap();
        if again != payload {
            failures.push(format!("re-encode mismatch {} ({} vs {} bytes)", packet.name(), again.len(), payload.len()));
        }
        match packet {
            play::ClientboundPlayPacket::KeepAlive(p) => send!(c, play::ServerboundPlayPacket::KeepAlive(play::ServerboundKeepAlivePacket { id: p.id })),
            play::ClientboundPlayPacket::Ping(p) => send!(c, play::ServerboundPlayPacket::Pong(play::ServerboundPongPacket { id: p.id })),
            play::ClientboundPlayPacket::ChunkBatchFinished(_) => {
                send!(c, play::ServerboundPlayPacket::ChunkBatchReceived(play::ServerboundChunkBatchReceivedPacket { desired_chunks_per_tick: 20.0 }))
            }
            play::ClientboundPlayPacket::PlayerPosition(p) => send!(
                c,
                play::ServerboundPlayPacket::AcceptTeleportation(play::ServerboundAcceptTeleportationPacket {
                    id: p.id,
                    x: p.change.position.x,
                    y: p.change.position.y,
                    z: p.change.position.z,
                    y_rot: p.change.y_rot,
                    x_rot: p.change.x_rot,
                })
            ),
            play::ClientboundPlayPacket::Login(_) => send!(c, play::ServerboundPlayPacket::PlayerLoaded(play::ServerboundPlayerLoadedPacket)),
            play::ClientboundPlayPacket::Disconnect(d) => panic!("kicked: {}", d.reason.get_string()),
            _ => {}
        }
    }
    println!("{} packets, {} distinct types", counts.values().sum::<usize>(), counts.len());
    for (name, n) in &counts {
        println!("{n:6} {name}");
    }
    assert!(failures.is_empty(), "{} packets failed:\n{}", failures.len(), failures.join("\n"));
    for expected in ["minecraft:login", "minecraft:level_chunk_with_light", "minecraft:commands", "minecraft:player_position", "minecraft:set_time"] {
        assert!(counts.contains_key(expected), "never received {expected}");
    }
}
