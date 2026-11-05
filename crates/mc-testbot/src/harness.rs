//! Test harness: the real vanilla server through `mc-testkit` (offline, fixed seed) and our own server binary.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

/// The vanilla server (offline, peaceful, view/simulation distance 4) for `seed` and `gamemode`; `props` are extra
/// `key=value` server.properties entries that replace the defaults of the same key. Start it with `.start()`.
pub fn vanilla(seed: i64, gamemode: &str, props: &[&str]) -> mc_testkit::Builder {
    mc_testkit::Server::vanilla()
        .properties([
            "online-mode=false",
            "enforce-secure-profile=false",
            "white-list=false",
            "spawn-protection=0",
            "view-distance=4",
            "simulation-distance=4",
            "difficulty=peaceful",
            "level-type=minecraft:normal",
        ])
        .property("level-seed", &seed.to_string())
        .property("gamemode", gamemode)
        .properties(props.iter().copied())
        .xmx("1G")
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
    /// Starts `bin` for `seed` and `gamemode`; `props` are `key=value` entries replacing the defaults of the same key (e.g.
    /// `difficulty=normal`, `level-type=minecraft:flat`). Runs in `dir` (default: a fresh temp dir; the caller wipes/prepares it);
    /// `keep` leaves it on drop.
    pub fn start_in(bin: &Path, seed: i64, gamemode: &str, props: &[&str], dir: Option<PathBuf>, keep: bool) -> std::io::Result<Self> {
        let port = mc_testkit::free_port();
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
        let mut cmd = Command::new(bin);
        cmd.arg("nogui").current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = mc_testkit::spawn_in_group(cmd)?;
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
