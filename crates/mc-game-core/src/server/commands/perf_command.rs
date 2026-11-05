//! `PerfCommand`.
use crate::commands::message::simple_translatable;
use crate::commands::server_access::enter_server;
use crate::commands::{CommandSourceStack, LEVEL_OWNERS, has_permission, literal};
use crate::crash_report::SystemReport;
use crate::util::file_util::find_available_name;
use crate::util::java_format::format_fixed;
use crate::util::profiling::ProfileResults;
use crate::util::profiling::metrics::PROFILING_RESULTS_DIR;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, SimpleCommandExceptionType};
use std::io::Write as _;
use std::path::Path;
use std::sync::LazyLock;

static ERROR_NOT_RUNNING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.perf.notRunning"));
static ERROR_ALREADY_RUNNING: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.perf.alreadyRunning"));

register_command!("PerfCommand", |ctx| {
    ctx.dispatcher.register(
        literal("perf")
            .requires(has_permission(LEVEL_OWNERS))
            .then(literal("start").executes(|c| start_profiling_dedicated_server(c.get_source())))
            .then(literal("stop").executes(|c| stop_profiling_dedicated_server(c.get_source()))),
    );
});

fn start_profiling_dedicated_server(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if source.with_server(|server| server.is_recording_metrics()) {
        return Err(ERROR_ALREADY_RUNNING.create());
    }
    let (stopped_source, finished_source) = (source.clone(), source.clone());
    source.with_server(|server| {
        let shared = server.shared.clone();
        let level_name = server.world_data.lock().settings.level_name.clone();
        server.start_recording_metrics(
            move |results| when_stopped(&stopped_source, &results),
            move |report| save_results(finished_source, report, &shared, &level_name),
        );
    });
    source.send_success(|| translatable!("commands.perf.started"), false);
    Ok(0)
}

fn stop_profiling_dedicated_server(source: &CommandSourceStack) -> Result<i32, CommandSyntaxException> {
    if !source.with_server(|server| server.is_recording_metrics()) {
        return Err(ERROR_NOT_RUNNING.create());
    }
    source.with_server(|server| server.finish_recording_metrics());
    Ok(0)
}

/// `saveResults(source, report, server)` on the IO thread: `<date>-<level>-<version>.zip` in `debug/profiling` holding
/// `system.txt` and the report directory; feedback goes back to the game thread.
fn save_results(source: CommandSourceStack, report: &Path, shared: &std::sync::Arc<crate::server::shared::ServerShared>, level_name: &str) {
    let feedback = |run: Box<dyn FnOnce(&CommandSourceStack) + Send>| {
        shared.execute(move |server| enter_server(server, || run(&source)));
    };
    let profiling_name = format!("{}-{}-{}", crate::crash_report::filename_formatted_date_time(), level_name, crate::server::packs::GAME_VERSION);
    let results_dir = Path::new(PROFILING_RESULTS_DIR);
    let zip_file = match find_available_name(results_dir, &profiling_name, ".zip") {
        Ok(name) => name,
        Err(e) => {
            log::error!("Failed to create report name\n{e}");
            feedback(Box::new(|source| source.send_failure(translatable!("commands.perf.reportFailed"))));
            return;
        }
    };
    let system_report = shared
        .execute_blocking(|server| {
            let mut report = SystemReport::new();
            server.fill_system_report(&mut report);
            report.to_line_separated_string()
        })
        .unwrap_or_default();
    if let Err(e) = zip_report(&results_dir.join(&zip_file), &system_report, report) {
        log::error!("Failed to write the profiling report\n{e}");
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(report) {
        log::warn!("Failed to delete temporary profiling file {}\n{e}", report.display());
    }
    feedback(Box::new(move |source| source.send_success(|| translatable!("commands.perf.reportSaved", zip_file.clone()), false)));
}

/// `FileZipper`: writes `output + "_tmp"`, then moves it to `output`.
fn zip_report(output: &Path, system_report: &str, report: &Path) -> std::io::Result<()> {
    let mut temp_name = output.file_name().unwrap_or_default().to_os_string();
    temp_name.push("_tmp");
    let temp_file = output.with_file_name(temp_name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&temp_file)?);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let to_io = |e: zip::result::ZipError| std::io::Error::other(e);
    zip.start_file("system.txt", options).map_err(to_io)?;
    zip.write_all(system_report.as_bytes())?;
    let mut stack = vec![report.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path.strip_prefix(report).map_err(std::io::Error::other)?.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
                zip.start_file(relative, options).map_err(to_io)?;
                zip.write_all(&std::fs::read(&path)?)?;
            }
        }
    }
    zip.finish().map_err(to_io)?;
    std::fs::rename(&temp_file, output)?;
    log::info!("Compressed to {}", output.display());
    Ok(())
}

/// `whenStopped(source, results)`.
fn when_stopped(source: &CommandSourceStack, results: &ProfileResults) {
    if matches!(results, ProfileResults::Empty) {
        return;
    }
    let ticks = results.get_tick_duration();
    let duration_in_seconds = results.get_nano_duration() as f64 / 1_000_000_000.0;
    source.send_success(
        || translatable!("commands.perf.stopped", format_fixed(duration_in_seconds, 2), ticks, format_fixed(f64::from(ticks) / duration_in_seconds, 2)),
        false,
    );
}
