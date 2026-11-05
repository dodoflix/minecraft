//! Shared helpers: run the world-load pipeline against a world directory and capture log output.
#![allow(dead_code, reason = "each test target uses a subset of the shared helpers")]

use std::path::Path;
use std::sync::Mutex;

use mc_game::registry::RegistryAccess;
use mc_game::server::packs::ServerPacksSource;
use mc_game::server::world_loader::{self, DataLoadOutput, PackConfig, WorldStem};
use mc_world::level_storage::{DirectoryValidator, WorldDataConfiguration};

/// Tests that look at captured logs hold this for their whole body.
pub static SERIAL: Mutex<()> = Mutex::new(());
static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INIT: std::sync::Once = std::sync::Once::new();

struct TestLogger;

impl log::Log for TestLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        LOGS.lock().unwrap().push(format!("{}: {}", r.level(), r.args()));
    }
    fn flush(&self) {}
}

/// Install the capturing logger (once per test binary). Tests using it must run serially (`--test-threads=1` or one test).
pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&TestLogger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
}

pub fn take_logs() -> Vec<String> {
    std::mem::take(&mut *LOGS.lock().unwrap())
}

pub fn load_world(world: &Path, config: WorldDataConfiguration, init_mode: bool) -> Result<(WorldDataConfiguration, WorldStem<()>), world_loader::WorldLoadError> {
    let mut repo = ServerPacksSource::create_pack_repository(world.join("datapacks"), DirectoryValidator::no_symlinks_allowed());
    let mut pc = PackConfig { pack_repository: &mut repo, initial_data_config: config, safe_mode: false, init_mode, function_compilation_permissions: mc_game::server::permissions::LevelBasedPermissionSet::GAMEMASTER.into() };
    world_loader::load(
        &mut pc,
        |_ctx| DataLoadOutput { cookie: (), final_dimensions: RegistryAccess::empty() },
        |rm, managers, layers, cookie| WorldStem::new(rm, managers, layers, cookie),
    )
}

pub mod entity_oracle;
pub mod entity_scenario;
pub mod feature_harness;
pub mod structure_golden;
pub mod block_scenario;
pub mod test_packs;
