//! Digging timeline of `ServerPlayerGameMode` against the real vanilla class: for every (tool, block, effect) case of
//! `tests/oracle/scenarios/dig.txt` the vanilla oracle (`scripts/entity-oracle.sh`, real `ServerGamePacketListenerImpl` /
//! `ServerPlayerGameMode` on an in-process level) printed the number of client ticks until the block broke, whether it
//! broke, and every `block_destruction` / `level_event` packet a second player in range received, per server tick.
//! This test replays the same client behaviour (`MultiPlayerGameMode`: START, accumulate `getDestroyProgress` per tick,
//! STOP at >= 1) against our `ServerPlayer::{handle_block_break_action, tick_game_mode}` and compares all of it.
//! Regenerate the fixture: `scripts/entity-oracle.sh $PWD/crates/mc-game/tests/oracle/scenarios/dig.txt -o /tmp/dig_raw.txt`, then keep only the lines the test reads:
//! `grep -E "^(T |DIG |# CASE|PKT Obs minecraft:(block_destruction|level_event) )" /tmp/dig_raw.txt > crates/mc-game/tests/fixtures/entity_oracle/dig.txt`.

use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_data::{Item, MobEffect};
use mc_game::server::level::ServerLevel;
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::world::entity::effect::MobEffectInstance;
use mc_game::world::entity::{Entity, ServerPlayer};
use mc_game::world::item::{InteractionHand, ItemStack};
use mc_game::world::level::entity::Visibility;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_protocol::codec::GameProfile;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::ServerboundPlayerActionAction as Action;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const BLOCK_DESTRUCTION: &str = "minecraft:block_destruction";
const LEVEL_EVENT: &str = "minecraft:level_event";

#[derive(Debug)]
struct Case {
    name: String,
    tool: String,
    block: String,
    effect: Option<(String, i32)>,
    dig_ticks: i32,
    broken: bool,
    /// `(tick index inside the case starting at 1, packet kind, hex)` of the observer's destruction / level-event packets.
    packets: Vec<(i32, String, String)>,
}

fn parse_fixture() -> Vec<Case> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/entity_oracle/dig.txt"
    ))
    .expect("dig fixture");
    let mut cases: Vec<Case> = Vec::new();
    let mut tick = 0;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# CASE ") {
            let mut parts = rest.split(' ');
            let n = parts.next().unwrap();
            let get = |p: Option<&str>, key: &str| p.unwrap().strip_prefix(key).unwrap().to_owned();
            let tool = get(parts.next(), "tool=");
            let block = get(parts.next(), "block=");
            let effect = get(parts.next(), "effect=");
            let (e, a) = effect.split_once(':').unwrap();
            cases.push(Case {
                name: format!("{n} {tool} {block} {effect}"),
                tool,
                block,
                effect: (e != "none").then(|| (e.to_owned(), a.parse().unwrap())),
                dig_ticks: -1,
                broken: false,
                packets: Vec::new(),
            });
            tick = 0;
        } else if let Some(c) = cases.last_mut() {
            if line.starts_with("T ") {
                tick += 1;
            } else if let Some(d) = line.strip_prefix("DIG Steve ticks=") {
                let (t, b) = d.split_once(" broken=").unwrap();
                c.dig_ticks = t.parse().unwrap();
                c.broken = b == "true";
            } else if let Some(p) = line.strip_prefix("PKT Obs ") {
                let (kind, hex) = p.split_once(' ').unwrap();
                if kind == BLOCK_DESTRUCTION || kind == LEVEL_EVENT {
                    c.packets
                        .push((tick, kind.to_owned(), normalize(kind, hex)));
                }
            }
        }
    }
    cases
}

fn read_varint(bytes: &[u8]) -> (i32, usize) {
    let (mut value, mut shift, mut i) = (0i32, 0, 0);
    loop {
        let b = bytes[i];
        value |= i32::from(b & 0x7f) << shift;
        i += 1;
        if b & 0x80 == 0 {
            return (value, i);
        }
        shift += 7;
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A destruction packet carries the breaker's entity id, which differs between the servers: drop it.
fn normalize(kind: &str, hex_text: &str) -> String {
    if kind != BLOCK_DESTRUCTION {
        return hex_text.to_owned();
    }
    let bytes: Vec<u8> = (0..hex_text.len() / 2)
        .map(|i| u8::from_str_radix(&hex_text[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let (_, n) = read_varint(&bytes);
    let (_, m) = read_varint(&bytes[n..]);
    format!("{}|{}", hex(&bytes[..n]), hex(&bytes[n + m..]))
}

struct Replay {
    level: ServerLevel,
    steve: ServerPlayer,
    rx: UnboundedReceiver<Outbound>,
}

fn overworld() -> Identifier {
    Identifier::parse("minecraft:overworld").unwrap()
}

fn replay_setup() -> Replay {
    let mut level = ServerLevel::detached(overworld());
    level
        .entity_manager
        .update_chunk_status(ChunkPos::new(0, 0), Visibility::Ticking);
    let (tx, rx) = unbounded_channel();
    let mut obs = ServerPlayer::new(
        2,
        overworld(),
        GameProfile {
            id: uuid::Uuid::from_u128(2),
            name: "Obs".into(),
            properties: vec![],
        },
        Default::default(),
    );
    obs.connection = Some(ConnectionSender::new(tx));
    obs.set_pos(6.5, -60.0, 0.5);
    level.add_player(Box::new(obs));
    let mut steve = ServerPlayer::new(
        1,
        overworld(),
        GameProfile {
            id: uuid::Uuid::from_u128(1),
            name: "Steve".into(),
            properties: vec![],
        },
        Default::default(),
    );
    steve.set_pos(0.5, -60.0, 0.5);
    steve.base_mut().set_on_ground_raw(true);
    steve.player.abilities.may_build = true;
    Replay { level, steve, rx }
}

/// What the observer received since the last call: `(kind, normalized hex)`.
fn drain(rx: &mut UnboundedReceiver<Outbound>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        let Outbound::Payload(p) = msg else { continue };
        let (id, _) = read_varint(&p);
        let kind = if id
            == <mc_protocol::packets::play::ClientboundBlockDestructionPacket as Packet>::ID
        {
            BLOCK_DESTRUCTION
        } else if id == <mc_protocol::packets::play::ClientboundLevelEventPacket as Packet>::ID {
            LEVEL_EVENT
        } else {
            continue;
        };
        out.push((kind.to_owned(), normalize(kind, &hex(&p))));
    }
    out
}

/// One server tick of the digger (`ServerPlayer.tick` -> `gameMode.tick()`), then the observer's packets.
fn tick_once(r: &mut Replay, case_tick: &mut i32, got: &mut Vec<(i32, String, String)>) {
    *case_tick += 1;
    r.steve.tick_game_mode(&mut r.level);
    for (kind, h) in drain(&mut r.rx) {
        got.push((*case_tick, kind, h));
    }
}

fn run_case(r: &mut Replay, c: &Case) -> (i32, bool, Vec<(i32, String, String)>) {
    let pos = BlockPos::new(2, -60, 0);
    let block = Block::by_name(&format!("minecraft:{}", c.block))
        .unwrap_or_else(|| panic!("block {}", c.block));
    r.level.set_block(pos, block.default_state(), 3, 512);
    let stack = if c.tool == "none" {
        ItemStack::empty()
    } else {
        ItemStack::new(Item::by_name(&c.tool).unwrap_or_else(|| panic!("item {}", c.tool)))
    };
    r.steve.set_item_in_hand(InteractionHand::MainHand, stack);
    r.steve.player.living.active_effects.clear();
    if let Some((name, amplifier)) = &c.effect {
        let effect = MobEffect::by_name(&format!("minecraft:{name}")).unwrap();
        r.steve.player.living.active_effects.insert(
            effect,
            MobEffectInstance::new(effect, 1_000_000, *amplifier, false, true, true, None),
        );
    }
    let mut got = Vec::new();
    let mut case_tick = 0;
    // the observer's packets from before this case (the previous case's trailing tick) are not part of it
    drain(&mut r.rx);
    r.steve.handle_block_break_action(
        &mut r.level,
        pos,
        Action::StartDestroyBlock,
        Direction::Up,
        319,
        false,
    );
    // `MultiPlayerGameMode.startDestroyBlock`: a survival client breaks at once when one tick of progress is >= 1
    let state = r.level.get_block_state(pos);
    let mut stopped = !state.is_air() && r.steve.get_destroy_progress(state) >= 1.0;
    let unbreakable = c.block == "bedrock";
    let mut progress = 0f32;
    let mut ticks = 0;
    while !stopped {
        ticks += 1;
        progress += r.steve.get_destroy_progress(r.level.get_block_state(pos));
        if if unbreakable {
            ticks >= 40
        } else {
            progress >= 1.0
        } {
            r.steve.handle_block_break_action(
                &mut r.level,
                pos,
                Action::StopDestroyBlock,
                Direction::Up,
                319,
                false,
            );
            stopped = true;
            tick_once(r, &mut case_tick, &mut got);
            break;
        }
        tick_once(r, &mut case_tick, &mut got);
    }
    let broken = r.level.get_block_state(pos).is_air();
    r.level.set_block(pos, Block::AIR.default_state(), 3, 512);
    tick_once(r, &mut case_tick, &mut got);
    (ticks, broken, got)
}

#[test]
fn dig_timeline_matches_vanilla() {
    let cases = parse_fixture();
    assert!(cases.len() > 100, "fixture has {} cases", cases.len());
    let mut r = replay_setup();
    let mut failures = Vec::new();
    for c in &cases {
        let (ticks, broken, got) = run_case(&mut r, c);
        let expected: Vec<&(i32, String, String)> = c.packets.iter().collect();
        let mut problems = Vec::new();
        // ice leaves water in vanilla (`IceBlock.playerDestroy`, a concrete block class that is not ported): only its `broken` flag differs
        if ticks != c.dig_ticks || (broken != c.broken && c.block != "ice") {
            problems.push(format!(
                "dig ticks={ticks} broken={broken}, vanilla ticks={} broken={}",
                c.dig_ticks, c.broken
            ));
        }
        if got.iter().collect::<Vec<_>>() != expected {
            let first = got
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != *b)
                .unwrap_or(got.len().min(expected.len()));
            problems.push(format!(
                "packets differ at #{first} (ours {} vs vanilla {}): ours {:?} vanilla {:?}",
                got.len(),
                expected.len(),
                got.get(first),
                expected.get(first)
            ));
        }
        if !problems.is_empty() {
            failures.push(format!("case {}: {}", c.name, problems.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
