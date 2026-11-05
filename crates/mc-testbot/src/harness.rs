//! Test harness: the real vanilla server through `scripts/vanilla-server.sh` (offline, fixed seed).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

/// True when the vanilla oracle can run here (java + `ref/server.jar`).
pub fn vanilla_available() -> bool {
    repo_root().join("ref/server.jar").exists() && Command::new("java").arg("-version").output().is_ok()
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port()
}

/// A running vanilla server (`run/servers/<name>`); stopped (cleanly) on drop.
pub struct VanillaServer {
    pub name: String,
    pub port: u16,
}

impl VanillaServer {
    /// Starts the server and blocks until `Done (`. `props` are extra `key=value` server.properties entries.
    pub fn start(name: &str, seed: i64, gamemode: &str, props: &[&str]) -> std::io::Result<Self> {
        Self::start_with(name, seed, gamemode, props, true)
    }

    /// `reset = false` keeps the scratch dir (an existing world) and only rewrites `server.properties`.
    pub fn start_with(name: &str, seed: i64, gamemode: &str, props: &[&str], reset: bool) -> std::io::Result<Self> {
        let port = free_port();
        // No pipes: the script's detached java keeps inherited stdout/stderr pipes open until the server exits.
        let status = Command::new(script())
            .args(["start", name, "--port", &port.to_string(), "--seed", &seed.to_string(), "--gamemode", gamemode])
            .args(["--view-distance", "4", "--simulation-distance", "4", "--xmx", "1G"])
            .args(reset.then_some("--reset"))
            .args(props)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            let out = std::fs::read_to_string(repo_root().join("run/servers").join(name).join("server.out")).unwrap_or_default();
            return Err(std::io::Error::other(format!("vanilla-server.sh start failed; server.out:\n{out}")));
        }
        Ok(Self { name: name.to_string(), port })
    }

    /// Console command, e.g. `op Bot`.
    pub fn cmd(&self, command: &str) {
        let _ = Command::new(script()).args(["cmd", &self.name, command]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }

    /// Waits for an extended regex in the server log; returns the last matching line.
    pub fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        let out = Command::new(script()).args(["wait", &self.name, regex, &timeout_s.to_string()]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn dir(&self) -> PathBuf {
        repo_root().join("run/servers").join(&self.name)
    }

    pub fn log_path(&self) -> PathBuf {
        repo_root().join("run/servers").join(&self.name).join("logs/latest.log")
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    pub fn stop(&self) {
        let _ = Command::new(script()).args(["stop", &self.name]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

impl Drop for VanillaServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn script() -> PathBuf {
    repo_root().join("scripts/vanilla-server.sh")
}

/// OUR server (`mc-server` binary) in a scratch dir, console over stdin, output captured for `wait_log`.
pub struct OurServer {
    pub port: u16,
    dir: PathBuf,
    child: std::process::Child,
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    out: std::sync::Arc<std::sync::Mutex<String>>,
    keep: bool,
}

impl OurServer {
    /// `MC_SERVER_BIN` or `target/*/debug/mc-server` of the repo. `None` when no binary is available.
    pub fn binary() -> Option<PathBuf> {
        if let Ok(b) = std::env::var("MC_SERVER_BIN") {
            return Some(PathBuf::from(b));
        }
        let root = repo_root();
        std::fs::read_dir(root.join("target"))
            .ok()?
            .flatten()
            .map(|e| e.path().join("debug/mc-server"))
            .find(|p| p.exists())
    }

    pub fn start(bin: &Path, seed: i64, gamemode: &str) -> std::io::Result<Self> {
        Self::start_with_props(bin, seed, gamemode, &[])
    }

    /// Like [`Self::start`]; `props` are `key=value` entries replacing the defaults of the same key (e.g. `difficulty=normal`,
    /// `level-type=minecraft:flat`).
    pub fn start_with_props(bin: &Path, seed: i64, gamemode: &str, props: &[&str]) -> std::io::Result<Self> {
        Self::start_in(bin, seed, gamemode, props, None, false)
    }

    /// [`Self::start_with_props`] in `dir` (default: a fresh temp dir; the caller wipes/prepares it); `keep` leaves it on drop.
    pub fn start_in(bin: &Path, seed: i64, gamemode: &str, props: &[&str], dir: Option<PathBuf>, keep: bool) -> std::io::Result<Self> {
        let port = free_port();
        let dir = dir.unwrap_or_else(|| std::env::temp_dir().join(format!("mc-testbot-ours-{}-{port}", std::process::id())));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("eula.txt"), "eula=true\n")?;
        let mut entries: Vec<(String, String)> = [
            ("online-mode", "false"),
            ("enforce-secure-profile", "false"),
            ("white-list", "false"),
            ("server-ip", "127.0.0.1"),
            ("generate-structures", "false"),
            ("spawn-protection", "0"),
            ("max-tick-time", "-1"),
            ("view-distance", "4"),
            ("simulation-distance", "4"),
            ("enable-rcon", "false"),
            ("enable-query", "false"),
            ("rate-limit", "0"),
            ("difficulty", "peaceful"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        entries.push(("server-port".into(), port.to_string()));
        entries.push(("level-seed".into(), seed.to_string()));
        entries.push(("gamemode".into(), gamemode.to_string()));
        for p in props {
            let (k, v) = p.split_once('=').expect("key=value");
            entries.retain(|(ek, _)| ek != k);
            entries.push((k.to_string(), v.to_string()));
        }
        let text: String = entries.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        std::fs::write(dir.join("server.properties"), text)?;
        let (child, stdin, out) = Self::spawn(bin, &dir)?;
        let server = Self { port, dir, child, stdin: std::sync::Mutex::new(stdin), out, keep };
        server.wait_log(r"Done \(", 240).ok_or_else(|| std::io::Error::other(format!("our server did not start:\n{}", server.log())))?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        Ok(server)
    }

    fn spawn(bin: &Path, dir: &Path) -> std::io::Result<(std::process::Child, Option<std::process::ChildStdin>, std::sync::Arc<std::sync::Mutex<String>>)> {
        use std::io::{BufRead, BufReader};
        let mut child = Command::new(bin)
            .arg("nogui")
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped");
        let stdin = child.stdin.take();
        let out = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = out.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(mut s) = sink.lock() {
                    s.push_str(&line);
                    s.push('\n');
                }
            }
        });
        Ok((child, stdin, out))
    }

    /// The server's working directory (`world/` lives in it).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Sends `stop` and waits for the process to exit; false when it had to be killed after `timeout_s`.
    pub fn stop_and_wait(&mut self, timeout_s: u32) -> bool {
        self.cmd("stop");
        for _ in 0..timeout_s * 10 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = self.child.kill();
        false
    }

    /// Starts the (stopped) server again in the same directory with the same port; waits for `Done (`.
    pub fn restart(&mut self, bin: &Path) -> std::io::Result<()> {
        let (child, stdin, out) = Self::spawn(bin, &self.dir)?;
        self.child = child;
        *self.stdin.lock().expect("stdin lock") = stdin;
        self.out = out;
        self.wait_log(r"Done \(", 240).ok_or_else(|| std::io::Error::other(format!("our server did not restart:\n{}", self.log())))?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        Ok(())
    }

    pub fn cmd(&self, command: &str) {
        use std::io::Write;
        if let Ok(mut g) = self.stdin.lock()
            && let Some(s) = g.as_mut()
        {
            let _ = writeln!(s, "{command}");
            let _ = s.flush();
        }
    }

    pub fn log(&self) -> String {
        self.out.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// `logs/latest.log` of our server (same format as vanilla's).
    pub fn log_path(&self) -> PathBuf {
        self.dir.join("logs/latest.log")
    }

    pub fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        let re = regex::Regex::new(regex).ok()?;
        for _ in 0..timeout_s * 5 {
            if let Some(l) = self.log().lines().rev().find(|l| re.is_match(l)) {
                return Some(l.to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        None
    }
}

impl Drop for OurServer {
    fn drop(&mut self) {
        self.cmd("stop");
        for _ in 0..100 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = self.child.kill();
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
