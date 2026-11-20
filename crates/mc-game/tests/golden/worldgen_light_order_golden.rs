//! Worldgen steps read the sky light of the chunks around them: a brown mushroom needs a raw brightness below 13, and the
//! one it takes the snow's place with survives only where the light is not known yet. Vanilla runs the light update on its
//! `light` executor, which the game thread schedules from outside the worker pool, so a chunk's decoration sees its
//! neighbours' light data only once the worker has nothing else queued; running the light engine between the worldgen
//! tasks (earlier than that) leaves bare grass where vanilla has snow.
//!
//! Seed 12345, chunks [146,146]..[154,148] forced one at a time in raster order (`scripts/worldgen-diff.sh` does the same
//! on both servers). Chunk [149,148] is snowy taiga: its snow layers and `snowy` grass columns are identical in every
//! vanilla run recorded of this area.
use crate::command_common::CWD_LOCK;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_game::server::MinecraftServer;
use mc_game::server::dedicated::launch::create_server;
use mc_game::server::dedicated::options::ServerOptions;
use mc_game::server::dedicated::properties::DedicatedServerSettings;
use mc_game::server::level::FullChunkStatus;
use mc_game::world::level::BlockGetter;
use mc_game::world::level::gamerules::GameRules;
use std::sync::{Arc, Mutex};

/// Ticks the chunk map stays idle before the next chunk is requested.
const SETTLE_TICKS: u32 = 20;

struct Raster {
    next: (i32, i32),
    area: (i32, i32, i32, i32),
    started: bool,
    waiting: Option<u32>,
    snow: Arc<Mutex<Option<i32>>>,
}

impl Raster {
    fn step(&mut self, server: &mut MinecraftServer) {
        let (x1, z1, x2, z2) = self.area;
        if !self.started {
            self.started = true;
            server.with_tick_rate_manager(|manager, host| {
                manager.set_frozen(true, host);
                manager.set_tick_rate(1000.0, host);
            });
            server.set_game_rule(GameRules::RANDOM_TICK_SPEED, 0);
            self.next = (x1, z1);
        }
        let (cx, cz) = self.next;
        let level = server
            .level_mut(&Identifier::with_default_namespace("overworld").expect("overworld"))
            .expect("overworld is loaded");
        let Some(settled) = self.waiting else {
            level
                .chunk_source
                .update_chunk_forced(ChunkPos::new(cx, cz), true);
            self.waiting = Some(0);
            return;
        };
        let map = &level.chunk_source.chunk_map;
        let idle = map.pending_task_count() == 0
            && !map.worldgen_busy()
            && map.loading_count() == 0
            && map.main_tasks.is_empty();
        if level.full_chunk_status(cx, cz) != FullChunkStatus::EntityTicking || !idle {
            self.waiting = Some(0);
            return;
        }
        if settled < SETTLE_TICKS {
            self.waiting = Some(settled + 1);
            return;
        }
        self.waiting = None;
        if cx < x2 {
            self.next = (cx + 1, cz);
        } else if cz < z2 {
            self.next = (x1, cz + 1);
        } else {
            let mut snow = 0;
            for x in 149 * 16..150 * 16 {
                for z in 148 * 16..149 * 16 {
                    for y in level.get_min_y()..level.get_max_y() {
                        if level.get_block_state(BlockPos::new(x, y, z)).block() == Block::SNOW {
                            snow += 1;
                        }
                    }
                }
            }
            *self.snow.lock().expect("result") = Some(snow);
            server.shared.halt();
        }
    }
}

#[test]
fn snowy_taiga_snow_matches_vanilla() {
    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("temp dir");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("free port")
        .local_addr()
        .expect("port")
        .port();
    std::fs::write(
        dir.path().join("server.properties"),
        format!(
            "server-port={port}\nonline-mode=false\nview-distance=3\nsimulation-distance=3\n\
             level-seed=12345\ngenerate-structures=true\nmax-players=0\npause-when-empty-seconds=-1\n"
        ),
    )
    .expect("server.properties");
    std::fs::write(dir.path().join("eula.txt"), "eula=true\n").expect("eula");
    std::env::set_current_dir(dir.path()).expect("cd into the server directory");
    let snow = Arc::new(Mutex::new(None));
    let result = snow.clone();
    // the server lives on its own thread, like `Server thread` in the real process
    std::thread::Builder::new()
        .name("Server thread".to_owned())
        .stack_size(64 << 20)
        .spawn(move || {
            let settings = DedicatedServerSettings::new(std::path::Path::new("server.properties"))
                .expect("server.properties");
            let mut server = create_server(ServerOptions::default(), settings)
                .expect("server")
                .expect("not stopped early");
            let raster = Arc::new(Mutex::new(Raster {
                next: (0, 0),
                area: (146, 146, 154, 148),
                started: false,
                waiting: None,
                snow: result,
            }));
            let shared = server.shared.clone();
            server.add_tickable(move || {
                let raster = raster.clone();
                shared.execute(move |server| raster.lock().expect("raster").step(server));
            });
            server.run_server();
        })
        .expect("server thread")
        .join()
        .expect("the server ran");
    let snow = snow
        .lock()
        .expect("result")
        .expect("the area was generated");
    assert_eq!(snow, 255, "snow layers in chunk [149,148]");
}
