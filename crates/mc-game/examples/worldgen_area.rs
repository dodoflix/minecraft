//! Headless area generation for `scripts/worldgen-diff.sh` (our side of the vanilla-vs-ours world differ).
//!
//!   worldgen_area --dims overworld,the_nether,the_end --area X1,Z1,X2,Z2      (chunk coordinates, inclusive)
//!
//! Run in a server directory holding `server.properties` (seed, generate-structures, ...) and `eula.txt`. It starts the
//! REAL server (`create_server` + `run_server`: `initServer`, the tick loop, `stopServer` with its final save) and lets a
//! tickable queue one step per tick as a server task. The steps do what the script's vanilla half does through
//! commands: `tick freeze` + `gamerule random_tick_speed 0`, then per dimension and per chunk in raster order (z outer,
//! x inner) `forceload add` (a FORCED ticket = `ServerChunkCache.updateChunkForced`), wait until the chunk is
//! ENTITY_TICKING and the chunk map has been idle for [`SETTLE_TICKS`] ticks (vanilla: `if loaded` + `sleep 1`), so each
//! chunk's neighbourhood is generated on the serial worldgen thread before the next one is requested. When the area is
//! done the server is halted, which saves every level (like `save-all flush` + `stop`).
#![allow(clippy::print_stderr, reason = "command-line example")]

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_game::server::MinecraftServer;
use mc_game::server::dedicated::launch::create_server;
use mc_game::server::dedicated::logging;
use mc_game::server::dedicated::options::ServerOptions;
use mc_game::server::dedicated::properties::DedicatedServerSettings;
use mc_game::server::level::FullChunkStatus;
use mc_game::world::level::gamerules::GameRules;
use mc_world::region::RegionFileVersion;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Consecutive idle ticks before the next chunk is requested (vanilla's script sleeps 1 s = 20 ticks).
const SETTLE_TICKS: u32 = 20;
/// The loop runs at this tick rate (pacing only; the vanilla half runs 20 TPS).
const TICK_RATE: f32 = 1000.0;

struct Gen {
    dims: Vec<Identifier>,
    area: (i32, i32, i32, i32),
    started: bool,
    dim: usize,
    cx: i32,
    cz: i32,
    /// The chunk currently being waited for, with its settled-tick counter.
    waiting: Option<u32>,
    done: u32,
}

impl Gen {
    fn step(&mut self, server: &mut MinecraftServer) {
        let (x1, z1, x2, z2) = self.area;
        if !self.started {
            self.started = true;
            server.with_tick_rate_manager(|manager, host| {
                manager.set_frozen(true, host);
                manager.set_tick_rate(TICK_RATE, host);
            });
            server.set_game_rule(GameRules::RANDOM_TICK_SPEED, 0);
            (self.cx, self.cz) = (x1, z1);
        }
        let Some(dim) = self.dims.get(self.dim).cloned() else {
            return;
        };
        let Some(level) = server.level_mut(&dim) else {
            panic!("dimension {dim} is not loaded")
        };
        let Some(settled) = self.waiting else {
            level
                .chunk_source
                .update_chunk_forced(ChunkPos::new(self.cx, self.cz), true);
            self.waiting = Some(0);
            return;
        };
        let map = &level.chunk_source.chunk_map;
        let idle = map.pending_task_count() == 0
            && !map.worldgen_busy()
            && map.loading_count() == 0
            && map.main_tasks.is_empty();
        if level.full_chunk_status(self.cx, self.cz) != FullChunkStatus::EntityTicking || !idle {
            self.waiting = Some(0);
            return;
        }
        if settled < SETTLE_TICKS {
            self.waiting = Some(settled + 1);
            return;
        }
        self.waiting = None;
        self.done += 1;
        if self.cx < x2 {
            self.cx += 1;
        } else if self.cz < z2 {
            (self.cx, self.cz) = (x1, self.cz + 1);
        } else {
            log::info!("{dim}: {} chunks generated", (x2 - x1 + 1) * (z2 - z1 + 1));
            self.dim += 1;
            (self.cx, self.cz) = (x1, z1);
            if self.dim == self.dims.len() {
                log::info!("worldgen_area: area done ({} chunks), stopping", self.done);
                server.shared.halt();
            }
        }
    }
}

fn parse_args() -> Result<Gen, String> {
    let mut dims =
        vec![Identifier::with_default_namespace("overworld").map_err(|e| e.to_string())?];
    let mut area = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or(format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--dims" => {
                dims = value
                    .split(',')
                    .map(|d| {
                        Identifier::read(d.strip_prefix("minecraft:").unwrap_or(d))
                            .map_err(|e| format!("bad dimension {d}: {e}"))
                    })
                    .collect::<Result<_, _>>()?
            }
            "--area" => {
                let n: Vec<i32> = value
                    .split(',')
                    .map(|v| {
                        v.parse()
                            .map_err(|_| format!("bad number in --area {value}"))
                    })
                    .collect::<Result<_, _>>()?;
                let [x1, z1, x2, z2] = n[..] else {
                    return Err("--area wants X1,Z1,X2,Z2".to_owned());
                };
                area = Some((x1, z1, x2, z2));
            }
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    let area = area.ok_or("--area X1,Z1,X2,Z2 is required")?;
    Ok(Gen {
        dims,
        area,
        started: false,
        dim: 0,
        cx: 0,
        cz: 0,
        waiting: None,
        done: 0,
    })
}

fn main() -> std::process::ExitCode {
    let job = match parse_args() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("worldgen_area: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let thread = std::thread::Builder::new()
        .name("Server thread".to_owned())
        .stack_size(64 << 20)
        .spawn(move || {
            if let Err(e) = logging::init("logs", log::LevelFilter::Info) {
                eprintln!("Failed to initialize logging: {e}");
            }
            let settings = DedicatedServerSettings::new(Path::new("server.properties"))
                .expect("server.properties");
            RegionFileVersion::configure(&settings.get_properties().region_file_compression);
            let mut server = create_server(ServerOptions::default(), settings)
                .expect("server")
                .expect("server stopped early (eula?)");
            let job = Arc::new(Mutex::new(job));
            let shared = server.shared.clone();
            server.add_tickable(move || {
                let job = job.clone();
                shared.execute(move |server| job.lock().expect("job lock").step(server));
            });
            server.run_server();
        });
    match thread.map(|t| t.join()) {
        Ok(Ok(())) => std::process::ExitCode::SUCCESS,
        _ => std::process::ExitCode::FAILURE,
    }
}
