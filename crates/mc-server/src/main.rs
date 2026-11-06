//! The dedicated server binary: `net.minecraft.server.Main`'s flow up to the point where the server proper starts.

use std::path::Path;
use std::process::ExitCode;

use log::LevelFilter;
use mc_game::server::dedicated::eula::Eula;
use mc_game::server::dedicated::logging;
use mc_game::server::dedicated::options::{ServerOptions, USAGE};
use mc_game::server::dedicated::properties::DedicatedServerSettings;
use mc_world::region::RegionFileVersion;

fn main() -> ExitCode {
    // vanilla runs `Main.main` on a thread called `ServerMain`; the log lines carry that name
    let main_thread = std::thread::Builder::new()
        .name("ServerMain".to_owned())
        .spawn(run);
    match main_thread.map(|t| t.join()) {
        Ok(Ok(code)) => code,
        _ => ExitCode::FAILURE,
    }
}

/// `LOGGER.error(LogUtils.FATAL_MARKER, "Failed to start the minecraft server", e)`.
fn fatal(error: impl std::fmt::Display) {
    log::error!("Failed to start the minecraft server\n{error}");
}

#[expect(
    clippy::print_stderr,
    reason = "the usage text and a logging-init failure go to stderr before the logger exists"
)]
fn run() -> ExitCode {
    if let Err(e) = logging::init("logs", LevelFilter::Info) {
        eprintln!("Failed to initialize logging: {e}");
    }
    // `Main.main` returns normally (JVM exit code 0) after every early exit below, failures included
    let options = match ServerOptions::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => return report(e),
    };
    if options.help {
        eprint!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if let Some(e) = &options.early_error {
        return report(e);
    }
    if let Some(pid_file) = &options.pid_file
        && let Err(e) = std::fs::write(pid_file, std::process::id().to_string())
    {
        return report(format!("java.io.UncheckedIOException: {e}"));
    }

    let settings_file = Path::new("server.properties");
    let settings = match DedicatedServerSettings::new(settings_file) {
        Ok(s) => s,
        Err(e) => return report(e),
    };
    settings.force_save();
    RegionFileVersion::configure(&settings.get_properties().region_file_compression);
    let eula_file = Path::new("eula.txt");
    let eula = Eula::new(eula_file);
    if options.init_settings {
        log::info!(
            "Initialized '{}' and '{}'",
            absolute(settings_file),
            absolute(eula_file)
        );
        return ExitCode::SUCCESS;
    }
    if !eula.has_agreed_to_eula() {
        log::info!(
            "You need to agree to the EULA in order to run the server. Go to eula.txt for more info."
        );
        return ExitCode::SUCCESS;
    }
    if let Some(e) = &options.late_error {
        return report(e);
    }
    start_server(options, settings)
}

fn report(error: impl std::fmt::Display) -> ExitCode {
    fatal(error);
    ExitCode::SUCCESS
}

/// `Path.toAbsolutePath()`: the working directory joined with the relative path, not normalized.
fn absolute(path: &Path) -> String {
    std::env::current_dir()
        .map(|d| d.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// The hand-over to the real server (`MinecraftServer.spin`): `mc_game::server::dedicated::launch`.
fn start_server(options: ServerOptions, settings: DedicatedServerSettings) -> ExitCode {
    match mc_game::server::dedicated::launch(options, settings) {
        0 => ExitCode::SUCCESS,
        code => ExitCode::from(code as u8),
    }
}
