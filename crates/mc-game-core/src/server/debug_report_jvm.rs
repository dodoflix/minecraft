//! The files of `MinecraftServer.saveDebugReport` that describe the JVM (`classpath.txt`, `threads.txt`, `modules.txt`)
//! and the `queue:` line of `stats.txt` (`Util.backgroundExecutor()`), written from what this process has instead.

use std::io;
use std::path::Path;

/// `dumpClasspath(path)`. analogue: `java.class.path` is one entry per line; the process has one, its executable.
pub(super) fn dump_classpath(path: &Path) -> io::Result<()> {
    let executable = std::env::current_exe().unwrap_or_default();
    std::fs::write(path, format!("{}\n", executable.display()))
}

/// The `queue: ...` line of `dumpMiscStats`: `String.format("queue: %s\n", Util.backgroundExecutor())`, a
/// `TracingExecutor` record around a `ForkJoinPool`. analogue: only the worker count is known here; the pool's load
/// counters read zero.
pub(super) fn queue_line() -> String {
    let (parallelism, hash) = super::level::chunk_io::background_executor_state();
    format!("queue: {}\n", format_background_executor(parallelism, hash))
}

/// `TracingExecutor.toString()` over `ForkJoinPool.toString()`.
fn format_background_executor(parallelism: usize, identity_hash: u32) -> String {
    format!(
        "TracingExecutor[service=java.util.concurrent.ForkJoinPool@{identity_hash:x}[Running, parallelism = \
         {parallelism}, size = {parallelism}, active = 0, running = 0, steals = 0, tasks = 0, submissions = 0]]"
    )
}

/// `dumpThreads(path)`: every live thread sorted by name, as the first line of `ThreadInfo.toString()` followed by the
/// blank lines that `toString()` and the writer add. analogue: the kernel thread id and name stand in for the Java
/// ones, the Java state is derived from the kernel's, and no stack trace or lock information is listed.
pub(super) fn dump_threads(path: &Path) -> io::Result<()> {
    let mut threads = live_threads();
    threads.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (name, tid, state) in threads {
        out.push_str(&format_thread_info(&name, tid, state));
        out.push('\n');
    }
    std::fs::write(path, out)
}

/// `ThreadInfo.toString()` without frames: `"name" Id=<id> <STATE>` and the empty line that ends it.
fn format_thread_info(name: &str, tid: u32, state: &str) -> String {
    format!("\"{name}\" Id={tid} {state}\n\n")
}

/// `(name, thread id, Java state)` of the threads of this process; empty where `/proc` is unavailable.
fn live_threads() -> Vec<(String, u32, &'static str)> {
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| {
            let tid = task.file_name().to_str()?.parse::<u32>().ok()?;
            // a thread that ended since the listing has no files any more
            let name = std::fs::read_to_string(task.path().join("comm")).ok()?;
            let stat = std::fs::read_to_string(task.path().join("stat")).ok()?;
            let state = java_thread_state(parse_stat_state(&stat)?);
            Some((name.trim_end_matches('\n').to_owned(), tid, state))
        })
        .collect()
}

/// The state letter of a `/proc/<pid>/task/<tid>/stat` line: the first field after the parenthesised command name.
fn parse_stat_state(stat: &str) -> Option<char> {
    stat.rsplit_once(')')?.1.trim_start().chars().next()
}

/// `Thread.State` closest to a kernel state letter.
fn java_thread_state(kernel_state: char) -> &'static str {
    match kernel_state {
        'R' => "RUNNABLE",
        'D' => "BLOCKED",
        'Z' | 'X' | 'x' => "TERMINATED",
        _ => "WAITING",
    }
}

/// `dumpNativeModules(path)`: vanilla's `NativeModuleLister.listModules()` lists the loaded DLLs on Windows only and
/// returns nothing elsewhere, so the file is empty. shortcut: the Windows module list is not produced.
pub(super) fn dump_native_modules(path: &Path) -> io::Result<()> {
    std::fs::write(path, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_line_has_the_fork_join_pool_shape() {
        let line = format!("queue: {}\n", format_background_executor(7, 0x1a2b3c4));
        assert_eq!(
            line,
            "queue: TracingExecutor[service=java.util.concurrent.ForkJoinPool@1a2b3c4[Running, parallelism = 7, size = 7, \
             active = 0, running = 0, steals = 0, tasks = 0, submissions = 0]]\n"
        );
    }

    #[test]
    fn thread_info_header_names_the_thread_id_and_state() {
        assert_eq!(
            format_thread_info("Server thread", 42, "RUNNABLE"),
            "\"Server thread\" Id=42 RUNNABLE\n\n"
        );
    }

    #[test]
    fn state_letter_is_read_after_the_command_name() {
        // the command name may contain spaces and parentheses
        let stat = "123 (Server (main)) S 1 123 123 0 -1 4194560";
        assert_eq!(parse_stat_state(stat), Some('S'));
        assert_eq!(java_thread_state('S'), "WAITING");
        assert_eq!(java_thread_state('R'), "RUNNABLE");
        assert_eq!(java_thread_state('Z'), "TERMINATED");
    }

    #[test]
    fn the_calling_thread_is_listed_as_runnable() {
        if !Path::new("/proc/self/task").exists() {
            return;
        }
        let threads = live_threads();
        assert!(!threads.is_empty());
        assert!(threads.iter().any(|(_, _, state)| *state == "RUNNABLE"));
    }
}
