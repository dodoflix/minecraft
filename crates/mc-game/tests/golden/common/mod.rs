//! Shared helpers: run the world-load pipeline against a world directory and capture log output.
#![allow(
    dead_code,
    reason = "each test target uses a subset of the shared helpers"
)]

use std::path::Path;
use std::sync::Mutex;

use mc_game::registry::RegistryAccess;
use mc_game::server::packs::ServerPacksSource;
use mc_game::server::world_loader::{self, DataLoadOutput, PackConfig, WorldStem};
use mc_world::level_storage::{DirectoryValidator, WorldDataConfiguration};

/// Tests that look at captured logs hold this for their whole body.
pub static SERIAL: Mutex<()> = Mutex::new(());
/// Log lines with the thread that logged them: tests of one process log concurrently, each one reads only its own.
static LOGS: Mutex<Vec<(std::thread::ThreadId, String)>> = Mutex::new(Vec::new());
static INIT: std::sync::Once = std::sync::Once::new();

struct TestLogger;

impl log::Log for TestLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        LOGS.lock().unwrap().push((
            std::thread::current().id(),
            format!("{}: {}", r.level(), r.args()),
        ));
    }
    fn flush(&self) {}
}

/// Install the capturing logger (once per test binary).
pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&TestLogger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// The lines logged by the calling thread since its last call.
pub fn take_logs() -> Vec<String> {
    let me = std::thread::current().id();
    let mut all = LOGS.lock().unwrap();
    let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut *all)
        .into_iter()
        .partition(|(id, _)| *id == me);
    *all = others;
    mine.into_iter().map(|(_, line)| line).collect()
}

pub fn load_world(
    world: &Path,
    config: WorldDataConfiguration,
    init_mode: bool,
) -> Result<(WorldDataConfiguration, WorldStem<()>), world_loader::WorldLoadError> {
    let mut repo = ServerPacksSource::create_pack_repository(
        world.join("datapacks"),
        DirectoryValidator::no_symlinks_allowed(),
    );
    let mut pc = PackConfig {
        pack_repository: &mut repo,
        initial_data_config: config,
        safe_mode: false,
        init_mode,
        function_compilation_permissions:
            mc_game::server::permissions::LevelBasedPermissionSet::GAMEMASTER.into(),
    };
    world_loader::load(
        &mut pc,
        |_ctx| DataLoadOutput {
            cookie: (),
            final_dimensions: RegistryAccess::empty(),
        },
        WorldStem::new,
    )
}

pub mod block_pool;
pub mod block_scenario;
pub mod entity_describe;
pub mod entity_oracle;
pub mod entity_scenario;
pub mod feature_harness;
pub mod menu_actions;
pub mod menu_oracle;
pub mod mob_verbs;
pub mod player_actions;
pub mod structure_golden;
pub mod test_packs;
