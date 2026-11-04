//! Test harness: the real vanilla server through `scripts/vanilla-server.sh` (offline, fixed seed).
use std::path::{Path, PathBuf};
use std::process::Command;

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
        let port = free_port();
        let out = Command::new(script())
            .args(["start", name, "--port", &port.to_string(), "--seed", &seed.to_string(), "--gamemode", gamemode])
            .args(["--view-distance", "4", "--simulation-distance", "4", "--xmx", "1G", "--reset"])
            .args(props)
            .output()?;
        if !out.status.success() {
            return Err(std::io::Error::other(format!(
                "vanilla-server.sh start failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(Self { name: name.to_string(), port })
    }

    /// Console command, e.g. `op Bot`.
    pub fn cmd(&self, command: &str) {
        let _ = Command::new(script()).args(["cmd", &self.name, command]).status();
    }

    /// Waits for an extended regex in the server log; returns the last matching line.
    pub fn wait_log(&self, regex: &str, timeout_s: u32) -> Option<String> {
        let out = Command::new(script()).args(["wait", &self.name, regex, &timeout_s.to_string()]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn log_path(&self) -> PathBuf {
        repo_root().join("run/servers").join(&self.name).join("logs/latest.log")
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    pub fn stop(&self) {
        let _ = Command::new(script()).args(["stop", &self.name]).output();
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
