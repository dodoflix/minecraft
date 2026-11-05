//! Differential test: the block-change packets (`block_update`, `section_blocks_update`, `level_event`) of a scripted
//! `/setblock` + `/fill` sequence, recorded from the REAL vanilla server (fixture written by
//! `mc-server/tests/oracle/record_setblock.rs`), against our level mutation + chunk change broadcast driven with the same
//! operations (the commands' own logic is replayed here: `SetBlockCommand` / `FillCommand` + `Level.setBlock` flags).

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_game::server::level::chunk_send_sink::ChunkSendSink;
use mc_game::server::level::{FullChunkStatus, PlayerTrackingInfo, ServerLevel};
use mc_game::server::network::{ConnectionSender, Outbound};
use mc_game::world::entity::Entity;
use mc_game::world::entity::player::ServerPlayer;
use mc_game::world::level::{BlockGetter, LevelAccessor};
use mc_protocol::codec::{Decode, GameProfile, read_varint};
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::play::{ClientboundBlockEntityDataPacket, ClientboundBlockUpdatePacket, ClientboundLevelEventPacket, ClientboundSectionBlocksUpdatePacket};
use std::time::{Duration, Instant};
use uuid::Uuid;

struct Sink;
impl ChunkSendSink for Sink {
    fn mark_chunk_pending_to_send(&mut self, _pos: ChunkPos) {}
    fn drop_chunk(&mut self, _pos: ChunkPos, _alive: bool) {}
    /// The recording bot never acknowledges chunk batches, so the vanilla `PlayerChunkSender` keeps the chunks beyond
    /// the first batch pending (and block changes of pending chunks are not sent): chunk (6, -1) is one of them.
    fn is_pending(&self, pos: i64) -> bool {
        pos == ChunkPos::pack_xz(6, -1)
    }
}

fn parse_state(text: &str) -> BlockState {
    let (name, props) = match text.split_once('[') {
        Some((n, p)) => (n, Some(p.trim_end_matches(']'))),
        None => (text, None),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("block {name}"));
    let props: Vec<(&str, &str)> = props.map(|p| p.split(',').filter_map(|kv| kv.split_once('=')).collect()).unwrap_or_default();
    block.state_from_properties(props).expect("valid properties")
}

fn parse_pos(parts: &[&str]) -> BlockPos {
    BlockPos::new(parts[0].parse().unwrap(), parts[1].parse().unwrap(), parts[2].parse().unwrap())
}

/// `BlockInput.place(level, pos, flags)`.
fn place(level: &mut ServerLevel, pos: BlockPos, state: BlockState, flags: u32) -> bool {
    level.set_block(pos, state, flags, 512)
}

/// `SetBlockCommand.setBlock`.
fn run_setblock(level: &mut ServerLevel, args: &[&str]) {
    let pos = parse_pos(&args[0..3]);
    let state = parse_state(args[3]);
    let mode = args.get(4).copied().unwrap_or("replace");
    if mode == "keep" && !level.get_block_state(pos).is_air() {
        return;
    }
    let place_needed = if mode == "destroy" {
        level.destroy_block(pos, true, None, 512);
        !state.is_air() || !level.get_block_state(pos).is_air()
    } else {
        true
    };
    let old_state = level.get_block_state(pos);
    if place_needed && !place(level, pos, state, 2 | 256) {
        return;
    }
    level.update_neighbours_on_block_set(pos, old_state);
}

/// `FillCommand.fillBlocks` (replace / keep-with-filter / destroy / hollow).
fn run_fill(level: &mut ServerLevel, args: &[&str]) {
    let (a, b) = (parse_pos(&args[0..3]), parse_pos(&args[3..6]));
    let state = parse_state(args[6]);
    let mode = args.get(7).copied().unwrap_or("replace");
    let filter = (mode == "replace").then(|| args.get(8).map(|f| Block::by_name(f).expect("filter block")));
    let (min, max) = (BlockPos::new(a.x().min(b.x()), a.y().min(b.y()), a.z().min(b.z())), BlockPos::new(a.x().max(b.x()), a.y().max(b.y()), a.z().max(b.z())));
    let mut updated: Vec<(BlockPos, BlockState)> = Vec::new();
    for pos in BlockPos::between_closed(&min, &max) {
        if let Some(Some(block)) = filter
            && level.get_block_state(pos).block() != block
        {
            continue;
        }
        let old_state = level.get_block_state(pos);
        if mode == "destroy" {
            level.destroy_block(pos, true, None, 512);
        }
        let target = if mode == "hollow" {
            let interior = pos.x() != min.x() && pos.x() != max.x() && pos.y() != min.y() && pos.y() != max.y() && pos.z() != min.z() && pos.z() != max.z();
            if interior { Some(Block::AIR.default_state()) } else { Some(state) }
        } else if mode == "destroy" {
            None
        } else {
            Some(state)
        };
        if let Some(target) = target
            && place(level, pos, target, 2 | 256)
        {
            updated.push((pos, old_state));
        }
    }
    for (pos, old_state) in updated {
        level.update_neighbours_on_block_set(pos, old_state);
    }
}

fn decode_line(payload: &[u8]) -> Option<String> {
    let mut body = payload;
    let id = read_varint(&mut body).ok()?;
    let name = packet_name(ConnectionState::Play, PacketFlow::Clientbound, id)?;
    let line = match name {
        "minecraft:block_update" => ClientboundBlockUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
        "minecraft:section_blocks_update" => ClientboundSectionBlocksUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
        "minecraft:level_event" => ClientboundLevelEventPacket::decode(&mut body).map(|p| format!("{p:?}")),
        "minecraft:block_entity_data" => ClientboundBlockEntityDataPacket::decode(&mut body).map(|p| format!("{p:?}")),
        _ => return None,
    };
    Some(line.unwrap_or_else(|e| format!("undecodable {name}: {e}")))
}

#[test]
fn setblock_and_fill_packets_match_vanilla() {
    let text = include_str!("../fixtures/setblock_vanilla.txt");
    // fixture: `# i command` header lines followed by the packet lines of that command
    let mut expected: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            let command = rest.split_once(' ').expect("index").1.to_string();
            expected.push((command, Vec::new()));
        } else if !line.is_empty() {
            expected.last_mut().expect("header first").1.push(line.to_string());
        }
    }
    assert!(!expected.is_empty());

    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.chunk_source.set_view_distance(4);
    level.chunk_source.set_simulation_distance(4);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = ConnectionSender::new(tx);
    let mut player = ServerPlayer::new(1, Identifier::parse("minecraft:overworld").unwrap(), GameProfile { id: Uuid::from_u128(1), name: "Bot".into(), properties: vec![] }, Default::default());
    player.connection = Some(sender.clone());
    player.set_pos(91.5, 102.0, -21.5);
    level.add_player(Box::new(player));
    level.chunk_source.chunk_map.add_player(PlayerTrackingInfo {
        entity_id: 1,
        uuid: Uuid::from_u128(1),
        connection: sender,
        sink: Box::new(Sink),
        position: Vec3::new(91.5, 102.0, -21.5),
        requested_view_distance: 4,
        spectator: false,
        alive: true,
    });
    let start = Instant::now();
    let ticking = |l: &ServerLevel, x: i32, z: i32| l.chunk_source.chunk_map.holders.visible_holder(ChunkPos::pack_xz(x, z)).is_some_and(|h| h.get_ticking_chunk().is_some());
    while level.full_chunk_status(5, -2) != FullChunkStatus::EntityTicking || !ticking(&level, 5, -2) || !ticking(&level, 6, -1) {
        level.tick_chunk_source(&|| true, false, true);
        if !level.poll_chunk_task() {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(start.elapsed() < Duration::from_secs(30), "chunks did not become ticking: (5,-2) {:?}, (6,-1) loaded {}", level.full_chunk_status(5, -2), level.chunk_now(6, -1).is_some());
    }
    while rx.try_recv().is_ok() {}

    for (command, expected_lines) in &expected {
        let words: Vec<&str> = command.split(' ').collect();
        match words[0] {
            "setblock" => run_setblock(&mut level, &words[1..]),
            "fill" => run_fill(&mut level, &words[1..]),
            other => panic!("unknown command {other}"),
        }
        level.chunk_source.broadcast_changed_chunks();
        let mut actual = Vec::new();
        while let Ok(out) = rx.try_recv() {
            if let Outbound::Payload(p) = out
                && let Some(line) = decode_line(&p)
            {
                actual.push(line);
            }
        }
        assert_eq!(&actual, expected_lines, "packets of `{command}`");
    }
    level.close();
}
