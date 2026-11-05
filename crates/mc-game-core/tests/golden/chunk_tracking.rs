//! A player joining an empty (void) level: player tickets, throttled loading, generation through the pyramid on the
//! worker thread, promotion to ticking, chunk tracking callbacks and unloading after the player leaves.

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::ChunkPos;
use mc_game::server::level::chunk_send_sink::ChunkSendSink;
use mc_game::server::level::{ChunkTrackingView, FullChunkStatus, PlayerTrackingInfo, ServerLevel};
use mc_game::server::network::ConnectionSender;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Default)]
struct SinkState {
    pending: HashSet<i64>,
    dropped: Vec<ChunkPos>,
}

struct Sink(Arc<Mutex<SinkState>>);

impl ChunkSendSink for Sink {
    fn mark_chunk_pending_to_send(&mut self, pos: ChunkPos) {
        self.0.lock().unwrap().pending.insert(pos.pack());
    }
    fn drop_chunk(&mut self, pos: ChunkPos, _alive: bool) {
        let mut s = self.0.lock().unwrap();
        s.pending.remove(&pos.pack());
        s.dropped.push(pos);
    }
    fn is_pending(&self, pos: i64) -> bool {
        self.0.lock().unwrap().pending.contains(&pos)
    }
}

fn run_until(level: &mut ServerLevel, what: &str, mut done: impl FnMut(&mut ServerLevel) -> bool) {
    let start = Instant::now();
    while !done(level) {
        if !level.poll_chunk_task() {
            std::thread::sleep(Duration::from_millis(1));
        }
        if start.elapsed() > Duration::from_secs(10) {
            let cs = &level.chunk_source;
            let mut by_status: std::collections::BTreeMap<String, usize> = Default::default();
            for k in cs.visible_keys() {
                let h = cs.chunk_map.holders.holder(k).unwrap();
                *by_status.entry(format!("lvl{} latest={:?} full={:?}", h.get_ticket_level(), h.get_latest_status(), h.get_full_status())).or_default() += 1;
            }
            for k in cs.visible_keys() {
                let h = cs.chunk_map.holders.holder(k).unwrap();
                if h.get_ticket_level() <= 33 && h.get_latest_status() != Some(mc_world::status::ChunkStatus::Full) {
                    let slot = match &h.slot {
                        mc_game::server::level::chunk_holder::ChunkSlot::None => "none",
                        mc_game::server::level::chunk_holder::ChunkSlot::Proto(_) => "proto",
                        mc_game::server::level::chunk_holder::ChunkSlot::Level(_) => "level",
                        mc_game::server::level::chunk_holder::ChunkSlot::Loaned => "loaned",
                    };
                    let futs: Vec<_> = mc_world::status::ChunkStatus::ALL.iter().map(|st| format!("{:?}", h.future_state(*st))).collect();
                    eprintln!("STUCK {} slot={slot} started={:?} allowed={:?} refs={} futs={futs:?}", h.get_pos(), h.started_work(), h.highest_allowed_status(), h.generation_ref_count());
                }
            }
            panic!("timed out waiting for {what}: has_work={} main_tasks={} {:#?}", cs.chunk_map.has_work(), cs.chunk_map.main_tasks.len(), by_status);
        }
    }
}

#[test]
fn player_loads_ticks_and_unloads_chunks() {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.chunk_source.set_view_distance(4);
    level.chunk_source.set_simulation_distance(3);
    let state = Arc::new(Mutex::new(SinkState::default()));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    level.chunk_source.chunk_map.add_player(mc_game::server::level::PlayerTrackingInfo {
        entity_id: 1,
        uuid: Uuid::nil(),
        connection: ConnectionSender::new(tx),
        sink: Box::new(Sink(state.clone())),
        position: Vec3::new(8.0, 64.0, 8.0),
        requested_view_distance: 4,
        spectator: false,
        alive: true,
    });
    let _ = std::any::type_name::<PlayerTrackingInfo>();
    let view = level.chunk_source.chunk_map.player_chunk_tracking_view(1);
    assert_eq!(view, ChunkTrackingView::of(ChunkPos::new(0, 0), 4));
    let mut expected = HashSet::new();
    view.for_each(|p| {
        expected.insert(p.pack());
    });
    run_until(&mut level, "all tracked chunks to be ready to send", |_l| {
        state.lock().unwrap().pending.len() >= expected.len()
    });
    assert_eq!(state.lock().unwrap().pending, expected);
    assert_eq!(level.full_chunk_status(0, 0), FullChunkStatus::EntityTicking);
    // Simulation distance 3: chunks at distance 3 tick blocks, farther ones are only FULL.
    // Simulation distance 3 (player simulation ticket level 28): inside it chunks tick entities, outside they do not
    // (the loading level of tracked chunks is ENTITY_TICKING anyway).
    run_until(&mut level, "simulation ticket levels", |l| l.chunk_source.chunk_map.distance_manager.in_entity_ticking_range(ChunkPos::pack_xz(2, 0)));
    assert!(!level.chunk_source.chunk_map.distance_manager.in_entity_ticking_range(ChunkPos::pack_xz(4, 0)));
    assert!(level.chunk_source.chunk_map.distance_manager.in_block_ticking_range(ChunkPos::pack_xz(3, 0)));
    assert!(level.chunk_now(4, 4).is_some());

    // The player leaves: tickets are removed and every chunk unloads (saved to the region files).
    level.chunk_source.chunk_map.remove_player(1);
    run_until(&mut level, "chunks to unload", |l| {
        l.tick_chunk_source(&|| true, false, true);
        !l.chunk_source.chunk_map.has_work() || l.chunk_source.get_loaded_chunks_count() == 0
    });
    assert_eq!(level.chunk_source.get_loaded_chunks_count(), 0);
    assert!(state.lock().unwrap().pending.is_empty(), "every chunk was dropped for the player");
    level.close();
}

/// `ChunkMap.waitForLightBeforeSending`: every visible chunk around the centre gets its own `waitForPendingTasks`
/// barrier (queued through the light dispatcher at the chunk's queue level); a chunk is not sendable until the light
/// thread (`poll_chunk_task`) ran its barrier, and is sendable afterwards.
#[test]
fn wait_for_light_before_sending_holds_chunks_until_their_barrier_ran() {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    level.chunk_source.set_view_distance(3);
    let state = Arc::new(Mutex::new(SinkState::default()));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    level.chunk_source.chunk_map.add_player(PlayerTrackingInfo {
        entity_id: 1,
        uuid: Uuid::nil(),
        connection: ConnectionSender::new(tx),
        sink: Box::new(Sink(state.clone())),
        position: Vec3::new(8.0, 64.0, 8.0),
        requested_view_distance: 3,
        spectator: false,
        alive: true,
    });
    run_until(&mut level, "centre chunk sendable", |l| l.chunk_source.chunk_map.get_chunk_to_send(ChunkPos::pack_xz(0, 0)).is_some());
    run_until(&mut level, "light settled", |l| !l.chunk_source.chunk_map.light.has_pending_work());
    let around: Vec<i64> = ChunkPos::range_closed(&ChunkPos::new(0, 0), 1).map(|p| p.pack()).collect();
    level.chunk_source.chunk_map.wait_for_light_before_sending(ChunkPos::new(0, 0), 0);
    for key in &around {
        assert!(level.chunk_source.chunk_map.get_chunk_to_send(*key).is_none(), "{key} is held back by its barrier");
    }
    assert!(level.chunk_source.chunk_map.light.dispatcher_has_work());
    run_until(&mut level, "barriers to run", |l| around.iter().all(|k| l.chunk_source.chunk_map.get_chunk_to_send(*k).is_some()));
    assert!(!level.chunk_source.chunk_map.light.has_pending_work());
    level.chunk_source.chunk_map.remove_player(1);
    level.close();
}
