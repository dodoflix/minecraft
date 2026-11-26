//! Launches the real vanilla server (`ref/server.jar`) or our `mc-server` binary for tests.
//!
//! One launcher for every test that needs a live server:
//!
//! - The child is owned by a [`Server`] guard. Dropping it, also while a test panics, kills the child's whole process
//!   group. The child also dies when the test process does, even on `kill -9` (`PR_SET_PDEATHSIG`, Linux).
//! - The port is chosen by the OS and passed to the server. If another process takes it before the server binds,
//!   the start is retried with a new port.
//! - Readiness is the `Done (` log line followed by a successful status ping, with a timeout; there are no fixed sleeps.
//! - The server runs in a temporary directory of its own (or in one the caller passes), never in a shared one.
//! - A missing prerequisite (`java`, `ref/server.jar`, the EULA opt-in) is an error with the command that fixes it.
//!   `start` returns it; tests `expect` it, so they fail instead of skipping.
//!
//! Starting a server writes `eula=true` into its directory, which accepts the Minecraft EULA on the caller's behalf, so
//! it needs `MC_ACCEPT_EULA=1` (`docs/testing.md`). This is the only Rust code that writes `eula=true`.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant};

use tempfile::TempDir;

mod deflate;
mod fixture;
mod mock_mojang;
mod permit;
mod query;
mod units;
mod worldref;
pub use deflate::{Corrupt, Deflate};
pub use fixture::{gunzip, read_gz};
pub use mock_mojang::{Certificate, Join, MockMojang, Request, offline_uuid};
pub use query::{Answer, QueryClient};
pub use units::{hex, rows, unesc_units, unhex};
pub use worldref::worldref;

/// Time for `Done (` and the first answered status ping, cold JVM on a loaded machine included.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
/// Time a clean `stop` may take before the process group is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(60);
/// A free port can be taken by another process before the server binds it; this many ports are tried.
const PORT_ATTEMPTS: usize = 5;
const EULA_URL: &str = "https://aka.ms/MinecraftEULA";

/// Why a server could not be started or did not become ready. `Debug` prints the plain message, so `expect` stays readable.
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A port the OS reported free a moment ago. Not reserved: [`Builder::start`] retries when the server cannot bind it.
pub fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("bind a loopback port")
        .local_addr()
        .expect("local address")
        .port()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---- process ownership ------------------------------------------------------------------------------------------

/// Spawns `cmd` as the leader of a new process group and arranges for the kernel to kill it when the calling process dies.
///
/// The parent-death signal is tied to the spawning *thread*, so the spawn happens on a helper thread that is parked
/// until the process exits, not on a test thread that ends with its test.
pub fn spawn_in_group(mut cmd: Command) -> io::Result<Child> {
    cmd.process_group(0);
    #[cfg(target_os = "linux")]
    {
        // SAFETY: between fork and exec the closure only makes the `prctl` system call: no allocation, no lock.
        unsafe {
            cmd.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let spawned = cmd.spawn();
        drop(cmd); // closes the parent's copies of the child's pipe ends
        let alive = spawned.is_ok();
        let _ = tx.send(spawned);
        if alive {
            loop {
                std::thread::park();
            }
        }
    });
    rx.recv().expect("spawner thread")
}

#[derive(Default)]
struct LogState {
    lines: Vec<String>,
    /// The child closed its output: it exited.
    eof: bool,
}

#[derive(Default)]
struct Log {
    state: Mutex<LogState>,
    changed: Condvar,
}

impl Log {
    fn fill(&self, out: impl Read) {
        let mut out = BufReader::new(out);
        let mut buf = Vec::new();
        while out.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
            let line = String::from_utf8_lossy(&buf).trim_end().to_owned();
            buf.clear();
            lock(&self.state).lines.push(line);
            self.changed.notify_all();
        }
        lock(&self.state).eof = true;
        self.changed.notify_all();
    }

    /// Blocks until `f` returns `Some`, the child's output ends, or `timeout` passes.
    fn wait<T>(&self, timeout: Duration, mut f: impl FnMut(&LogState) -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + timeout;
        let mut state = lock(&self.state);
        loop {
            if let Some(found) = f(&state) {
                return Some(found);
            }
            if state.eof {
                return None;
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// The child and its captured output. Killing the process group is in `Drop`.
struct Proc {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    log: Arc<Log>,
    pgid: libc::pid_t,
}

impl Proc {
    /// Starts `cmd` in `dir` with piped stdin, stdout captured into the log and stderr in `dir/stderr.log`.
    fn start(mut cmd: Command, dir: &Path) -> io::Result<Proc> {
        let (out, out_writer) = io::pipe()?;
        cmd.current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(out_writer)
            .stderr(File::create(dir.join("stderr.log"))?);
        let mut child = spawn_in_group(cmd)?;
        let stdin = child.stdin.take();
        let log = Arc::new(Log::default());
        let sink = log.clone();
        std::thread::spawn(move || sink.fill(out));
        Ok(Proc {
            pgid: child.id() as libc::pid_t,
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            log,
        })
    }

    fn cmd(&self, command: &str) {
        if let Some(stdin) = lock(&self.stdin).as_mut() {
            let _ = writeln!(stdin, "{command}").and_then(|()| stdin.flush());
        }
    }

    fn wait_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        self.log.wait(timeout, |s| s.eof.then_some(()))?;
        // The output ends when the process exits; polling instead of `wait` keeps the lock free for `kill`.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(Some(status)) = lock(&self.child).try_wait() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Kills the whole process group (if the child is still unreaped) and waits for the output to end.
    fn kill(&self) {
        let mut child = lock(&self.child);
        if child.try_wait().ok().flatten().is_none() {
            // SAFETY: plain syscall. The group id is the pid of our own unreaped child, so it cannot name a stranger.
            unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
            let _ = child.wait();
        }
        drop(child);
        self.log
            .wait(Duration::from_secs(5), |s| s.eof.then_some(()));
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        self.kill();
    }
}

// ---- the server -------------------------------------------------------------------------------------------------

/// A running server. Dropping it kills the process group and removes its temporary directory.
pub struct Server {
    pub port: u16,
    /// The RCON port of a server with `enable-rcon=true` and no `rcon.port` entry: a fresh free port per launch.
    pub rcon_port: Option<u16>,
    /// False with `enable-status=false`: the server closes status connections instead of answering them.
    replies_to_status: bool,
    proc: Proc,
    dir: PathBuf,
    /// Dropped after `proc`: the directory is removed only once the process is dead.
    _temp: Option<TempDir>,
    /// Released last: the next test starts its server only after this one is gone (`permit.rs`).
    _permit: Option<Arc<permit::Permit>>,
}

impl Server {
    /// The real vanilla server. Needs `java`, `ref/server.jar` and `MC_ACCEPT_EULA=1`.
    pub fn vanilla() -> Builder {
        Builder::new(Program::Vanilla)
    }

    /// Our own server binary (`mc-server`). Needs `MC_ACCEPT_EULA=1`, because it reads the same `eula.txt`.
    pub fn ours(bin: impl Into<PathBuf>) -> Builder {
        Builder::new(Program::Ours(bin.into()))
    }

    /// The server's working directory (`world/`, `logs/`, `server.properties`).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `logs/latest.log` of the server.
    pub fn log_path(&self) -> PathBuf {
        self.dir.join("logs/latest.log")
    }

    /// Everything the server printed so far.
    pub fn log(&self) -> String {
        lock(&self.proc.log.state)
            .lines
            .iter()
            .fold(String::new(), |all, l| all + l + "\n")
    }

    /// Runs a console command, e.g. `op Bot`.
    pub fn cmd(&self, command: &str) {
        self.proc.cmd(command);
    }

    /// Waits up to `timeout_s` for a line matching `regex` (anywhere in the output so far or later); returns the last match.
    pub fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        let re = regex::Regex::new(regex).expect("wait_log regex");
        self.proc
            .log
            .wait(Duration::from_secs(timeout_s.into()), |s| {
                s.lines.iter().rev().find(|l| re.is_match(l)).cloned()
            })
    }

    /// Waits for the process to exit by itself (for example a server that rejects its data packs).
    pub fn wait_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        self.proc.wait_exit(timeout)
    }

    /// Sends `stop` and waits for a clean exit; kills the process group after 60 s. True when it stopped by itself.
    pub fn stop(&self) -> bool {
        self.stop_within(STOP_TIMEOUT)
    }

    /// [`Self::stop`] with a custom limit.
    pub fn stop_within(&self, timeout: Duration) -> bool {
        self.proc.cmd("stop");
        let stopped = self.proc.wait_exit(timeout).is_some();
        if !stopped {
            self.proc.kill();
        }
        stopped
    }

    /// Kills the process group now. The log is complete afterwards.
    pub fn kill(&self) {
        self.proc.kill();
    }

    fn failure(&self, what: &str) -> Error {
        let log = lock(&self.proc.log.state)
            .lines
            .iter()
            .rev()
            .take(30)
            .rev()
            .fold(String::new(), |all, l| all + l + "\n");
        let stderr = fs::read_to_string(self.dir.join("stderr.log")).unwrap_or_default();
        Error(format!(
            "{what}\n--- server output (tail) ---\n{log}--- stderr ---\n{stderr}"
        ))
    }

    /// `Done (` is printed when the listener opens, but the main loop is not ticking yet and drops early
    /// connections; a status ping is only answered by a ticking server.
    fn wait_ready(&self) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        if self
            .proc
            .log
            .wait(READY_TIMEOUT, |s| {
                s.lines.iter().any(|l| l.contains("Done (")).then_some(())
            })
            .is_none()
        {
            return Err(self.failure("the server exited or timed out before printing `Done (`"));
        }
        while status_ping(self.port, self.replies_to_status).is_err() {
            if Instant::now() >= deadline || lock(&self.proc.log.state).eof {
                return Err(
                    self.failure("the server printed `Done (` but never answered a status ping")
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
}

type Prepare = Box<dyn FnOnce(&Path)>;

enum Program {
    Vanilla,
    Ours(PathBuf),
}

/// Configures and starts a [`Server`].
pub struct Builder {
    program: Program,
    args: Vec<String>,
    props: Vec<(String, String)>,
    envs: Vec<(String, String)>,
    java_properties: Vec<(String, String)>,
    dir: Option<PathBuf>,
    prepare: Option<Prepare>,
    xmx: String,
    jar: Option<PathBuf>,
    java: Option<PathBuf>,
    file_size_limit: Option<u64>,
}

impl Builder {
    fn new(program: Program) -> Self {
        Builder {
            args: Vec::new(),
            program,
            props: Vec::new(),
            envs: Vec::new(),
            java_properties: Vec::new(),
            dir: None,
            prepare: None,
            xmx: "1G".into(),
            jar: None,
            java: None,
            file_size_limit: None,
        }
    }

    /// One `server.properties` entry. `server-port` and `server-ip=127.0.0.1` (and `rcon.port` when RCON is enabled
    /// without one, see [`Server::rcon_port`]) are set by the launcher; anything else is the vanilla default unless set here. A later entry replaces an earlier one of the same key.
    pub fn property(mut self, key: &str, value: &str) -> Self {
        self.props.retain(|(k, _)| k != key);
        self.props.push((key.into(), value.into()));
        self
    }

    /// `key=value` entries, see [`Self::property`].
    pub fn properties<'a>(mut self, entries: impl IntoIterator<Item = &'a str>) -> Self {
        for entry in entries {
            let (key, value) = entry
                .split_once('=')
                .unwrap_or_else(|| panic!("property `{entry}` is not key=value"));
            self = self.property(key, value);
        }
        self
    }

    /// Runs in `dir` (created if missing) and leaves it behind. Without it the server gets a temporary directory that
    /// is removed when the [`Server`] is dropped.
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Called with the server directory before the first launch, for example to put a world into it.
    pub fn prepare(mut self, f: impl FnOnce(&Path) + 'static) -> Self {
        self.prepare = Some(Box::new(f));
        self
    }

    /// A command-line argument after `nogui` (`--demo`, `--bonusChest`), passed to the vanilla jar and to `mc-server`.
    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.into());
        self
    }

    /// An environment variable of the server process.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    /// A JVM system property (`-Dkey=value`) of the vanilla server. Vanilla only.
    pub fn java_property(mut self, key: &str, value: &str) -> Self {
        self.java_properties.push((key.into(), value.into()));
        self
    }

    /// Points the server's Mojang services at `mock`, see [`Self::discovery_url`].
    pub fn mojang(self, mock: &MockMojang) -> Self {
        self.discovery_url(&mock.discovery_url())
    }

    /// Points the server's Mojang services at the discovery document `url`: the discovery property for the vanilla jar
    /// (session server, profile keys, profile lookups), `MINECRAFT_API_DISCOVERY_HOST` for our own server (profile keys
    /// and lookups only: its `hasJoined` request still goes to the fixed session server URL).
    pub fn discovery_url(self, url: &str) -> Self {
        match self.program {
            Program::Vanilla => self.java_property("minecraft.api.discovery.host", url),
            Program::Ours(_) => self.env("MINECRAFT_API_DISCOVERY_HOST", url),
        }
    }

    /// JVM heap (`-Xmx`) of the vanilla server; default `1G`.
    pub fn xmx(mut self, xmx: &str) -> Self {
        self.xmx = xmx.into();
        self
    }

    /// Runs this server jar instead of `ref/server.jar` (an older vanilla release). Vanilla only.
    pub fn jar(mut self, jar: impl Into<PathBuf>) -> Self {
        self.jar = Some(jar.into());
        self
    }

    /// Runs the vanilla jar with this `java` executable instead of the one on `PATH` (releases up to 1.16.5 need
    /// JDK 8). Vanilla only.
    pub fn java(mut self, java: impl Into<PathBuf>) -> Self {
        self.java = Some(java.into());
        self
    }

    /// Caps the size of any file the server process writes (`RLIMIT_FSIZE`; the write that passes it fails with
    /// `File too large`). The vanilla jar then runs unbundled (its bundler would extract the libraries under the limit).
    pub fn file_size_limit(mut self, bytes: u64) -> Self {
        self.file_size_limit = Some(bytes);
        self
    }

    /// Starts the server and returns once it answers status pings.
    pub fn start(mut self) -> Result<Server> {
        let permit = permit::acquire();
        let (dir, temp) = self.prepare_dir()?;
        let mut last = None;
        for _ in 0..PORT_ATTEMPTS {
            let server = self.launch(&dir)?;
            match server.wait_ready() {
                // the RCON port came from a second `free_port()` and was taken before the server bound it
                Ok(())
                    if server.rcon_port.is_some()
                        && server.log().contains("Unable to initialise RCON") =>
                {
                    last = Some(Error("Unable to initialise RCON".into()));
                }
                Ok(()) => {
                    return Ok(Server {
                        _temp: temp,
                        _permit: Some(permit),
                        ..server
                    });
                }
                Err(e) if server.log().contains("FAILED TO BIND") => last = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last.expect("at least one attempt"))
    }

    /// Starts the server without waiting for readiness: for a server that is expected to fail while starting.
    /// Use [`Server::wait_exit`] and [`Server::log`] on the result.
    pub fn start_unready(mut self) -> Result<Server> {
        let permit = permit::acquire();
        let (dir, temp) = self.prepare_dir()?;
        Ok(Server {
            _temp: temp,
            _permit: Some(permit),
            ..self.launch(&dir)?
        })
    }

    fn prepare_dir(&mut self) -> Result<(PathBuf, Option<TempDir>)> {
        match (&self.program, &self.jar) {
            (Program::Vanilla, None) if !repo_root().join("ref/server.jar").is_file() => {
                return fail("ref/server.jar is missing: run scripts/setup-ref.sh");
            }
            (Program::Vanilla, Some(jar)) if !jar.is_file() => {
                return fail(format!("{} is missing", jar.display()));
            }
            _ => {}
        }
        eula_opt_in(std::env::var("MC_ACCEPT_EULA").ok().as_deref())?;
        let io = |what: &str, e: io::Error| Error(format!("{what}: {e}"));
        let (dir, temp) = match &self.dir {
            Some(dir) => (dir.clone(), None),
            None => {
                let temp = tempfile::Builder::new()
                    .prefix("mc-server-")
                    .tempdir()
                    .map_err(|e| io("create the server directory", e))?;
                (temp.path().to_path_buf(), Some(temp))
            }
        };
        fs::create_dir_all(&dir).map_err(|e| io("create the server directory", e))?;
        // Guarded by the opt-in above; our own server reads the same file.
        fs::write(dir.join("eula.txt"), "eula=true\n").map_err(|e| io("write eula.txt", e))?;
        if let Some(prepare) = self.prepare.take() {
            prepare(&dir);
        }
        Ok((dir, temp))
    }

    /// One launch on a fresh port; retries share the directory.
    fn launch(&self, dir: &Path) -> Result<Server> {
        let port = free_port();
        let mut props = vec![
            ("server-port".to_owned(), port.to_string()),
            ("server-ip".to_owned(), "127.0.0.1".to_owned()),
        ];
        for (k, v) in &self.props {
            props.retain(|(pk, _)| pk != k);
            props.push((k.clone(), v.clone()));
        }
        let rcon_enabled = props.contains(&("enable-rcon".to_owned(), "true".to_owned()));
        let rcon_port =
            (rcon_enabled && !props.iter().any(|(k, _)| k == "rcon.port")).then(free_port);
        if let Some(rcon_port) = rcon_port {
            props.push(("rcon.port".to_owned(), rcon_port.to_string()));
        }
        let text: String = props.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        fs::write(dir.join("server.properties"), text)
            .map_err(|e| Error(format!("write server.properties: {e}")))?;
        let (program, cmd) = match &self.program {
            Program::Vanilla => {
                let jar = self
                    .jar
                    .clone()
                    .unwrap_or_else(|| repo_root().join("ref/server.jar"));
                let jar = jar
                    .canonicalize()
                    .map_err(|e| Error(format!("{}: {e}", jar.display())))?;
                // A differently named link keeps `pkill -f server.jar` from touching it.
                let link = dir.join("vanilla-under-test.jar");
                let _ = fs::remove_file(&link);
                std::os::unix::fs::symlink(jar, &link)
                    .map_err(|e| Error(format!("link the jar: {e}")))?;
                // Canonical: the server starts in `dir`, which would otherwise be what a relative path resolves against.
                let java = match &self.java {
                    // a bare name is looked up on PATH
                    Some(java) if java.components().count() > 1 => java
                        .canonicalize()
                        .map_err(|e| Error(format!("{}: {e}", java.display())))?,
                    Some(java) => java.clone(),
                    None => PathBuf::from("java"),
                };
                let mut cmd = Command::new(&java);
                cmd.arg(format!("-Xmx{}", self.xmx));
                cmd.args(
                    self.java_properties
                        .iter()
                        .map(|(k, v)| format!("-D{k}={v}")),
                );
                if self.file_size_limit.is_some() && self.jar.is_none() {
                    // the bundler extracts its libraries into the server directory, files the limit would cut
                    cmd.arg("-cp")
                        .arg(unbundled_classpath()?)
                        .arg("net.minecraft.server.Main");
                } else {
                    cmd.arg("-jar").arg(link);
                }
                cmd.arg("nogui");
                (java.display().to_string(), cmd)
            }
            Program::Ours(bin) => {
                let mut cmd = Command::new(bin);
                cmd.arg("nogui");
                (bin.display().to_string(), cmd)
            }
        };
        let mut cmd = cmd;
        cmd.args(&self.args);
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)));
        if let Some(bytes) = self.file_size_limit {
            // SAFETY: between fork and exec the closure only calls `setrlimit` (a plain system call without allocation or
            // lock) and `signal` (async-signal-safe).
            unsafe {
                cmd.pre_exec(move || {
                    let limit = libc::rlimit {
                        rlim_cur: bytes as libc::rlim_t,
                        rlim_max: bytes as libc::rlim_t,
                    };
                    // the write fails with `EFBIG` instead of the process dying of `SIGXFSZ`
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) == -1
                        || libc::signal(libc::SIGXFSZ, libc::SIG_IGN) == libc::SIG_ERR
                    {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let proc = Proc::start(cmd, dir).map_err(|e| match (e.kind(), &self.program) {
            (io::ErrorKind::NotFound, Program::Vanilla) if self.java.is_none() => Error(
                "java was not found on PATH: install JDK 25 (scripts/setup-ref.sh needs it too)"
                    .into(),
            ),
            _ => Error(format!("cannot start {program}: {e}")),
        })?;
        Ok(Server {
            port,
            rcon_port,
            replies_to_status: !props
                .iter()
                .any(|(k, v)| k == "enable-status" && v == "false"),
            proc,
            dir: dir.to_path_buf(),
            _temp: None,
            _permit: None,
        })
    }
}

/// The classpath of the unbundled vanilla server (`scripts/vanilla-cp.sh`).
fn unbundled_classpath() -> Result<std::ffi::OsString> {
    fn jars(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                jars(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "jar") {
                out.push(path);
            }
        }
        Ok(())
    }
    let bundle = repo_root().join("ref/bundle/META-INF");
    let mut all = Vec::new();
    for part in ["versions", "libraries"] {
        jars(&bundle.join(part), &mut all).map_err(|e| {
            Error(format!(
                "{}: {e} (run scripts/setup-ref.sh)",
                bundle.display()
            ))
        })?;
    }
    all.sort();
    std::env::join_paths(
        all.iter()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone())),
    )
    .map_err(|e| Error(format!("classpath: {e}")))
}

/// `Ok` only for `MC_ACCEPT_EULA=1`.
fn eula_opt_in(value: Option<&str>) -> Result<()> {
    if value == Some("1") {
        return Ok(());
    }
    fail(format!(
        "starting a server accepts the Minecraft EULA ({EULA_URL}) in your name: set MC_ACCEPT_EULA=1 if you accept it"
    ))
}

// ---- readiness probe --------------------------------------------------------------------------------------------

fn varint(mut v: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_varint(r: &mut impl Read) -> io::Result<u32> {
    let (mut value, mut byte) = (0u32, [0u8; 1]);
    for shift in (0..35).step_by(7) {
        r.read_exact(&mut byte)?;
        value |= u32::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(io::Error::other("varint too long"))
}

/// A server list ping: handshake with the status intent, status request, read the response packet. A server that
/// ignores status requests (`replies` false) is ready once it closes the connection.
fn status_ping(port: u16, replies: bool) -> io::Result<()> {
    let mut s =
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut handshake = vec![0, 0]; // packet id, protocol version (not checked for status)
    varint(9, &mut handshake);
    handshake.extend(b"localhost");
    handshake.extend(port.to_be_bytes());
    handshake.push(1); // intent: status
    let mut request = Vec::new();
    varint(handshake.len() as u32, &mut request);
    request.extend(handshake);
    request.extend([1, 0]); // status request: length 1, id 0
    s.write_all(&request)?;
    if !replies {
        return match s.read(&mut [0u8; 1]) {
            Ok(0) => Ok(()),
            Ok(_) => Err(io::Error::other("status reply with enable-status=false")),
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => Ok(()),
            Err(e) => Err(e),
        };
    }
    let len = read_varint(&mut s)?;
    if len == 0 || len > 1 << 20 {
        return Err(io::Error::other("bad status response length"));
    }
    io::copy(&mut s.take(u64::from(len)), &mut io::sink()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eula_needs_the_exact_opt_in() {
        assert!(eula_opt_in(Some("1")).is_ok());
        for v in [None, Some(""), Some("0"), Some("true")] {
            let e = eula_opt_in(v).unwrap_err().to_string();
            assert!(
                e.contains("MC_ACCEPT_EULA=1") && e.contains(EULA_URL),
                "{e}"
            );
        }
    }

    /// With `enable-status=false` vanilla closes the connection after the handshake: that is the ready signal.
    #[test]
    fn a_server_that_ignores_status_is_ready_once_it_closes_the_connection() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || drop(listener.accept()));
        assert!(status_ping(port, false).is_ok());
    }

    #[test]
    fn a_missing_jar_is_an_error_naming_it() {
        let e = Server::vanilla()
            .jar("/nonexistent/old-server.jar")
            .java("/nonexistent/bin/java")
            .start_unready()
            .err()
            .expect("no such jar")
            .to_string();
        assert!(e.contains("/nonexistent/old-server.jar"), "{e}");
    }

    #[test]
    fn varint_round_trips() {
        for v in [0, 1, 127, 128, 25565, u32::MAX >> 4] {
            let mut b = Vec::new();
            varint(v, &mut b);
            assert_eq!(read_varint(&mut &b[..]).unwrap(), v);
        }
    }

    /// Dropping the guard kills grandchildren too: the shell backgrounds a `sleep` and reports its pid.
    #[test]
    fn drop_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let mut sh = Command::new("sh");
        sh.args(["-c", "sleep 300 & echo grandchild $!; wait"]);
        let proc = Proc::start(sh, dir.path()).unwrap();
        let line = proc
            .log
            .wait(Duration::from_secs(10), |s| {
                s.lines
                    .iter()
                    .find(|l| l.starts_with("grandchild "))
                    .cloned()
            })
            .expect("pid line");
        let pid: u32 = line["grandchild ".len()..].parse().unwrap();
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));
        assert!(proc_dir.exists());
        drop(proc);
        let deadline = Instant::now() + Duration::from_secs(5);
        while proc_dir.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!proc_dir.exists(), "grandchild {pid} survived the guard");
    }
}
