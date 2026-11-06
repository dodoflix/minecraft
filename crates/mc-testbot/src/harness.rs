//! Test harness: the real vanilla server through `mc-testkit` (offline, fixed seed) and our own server binary.
use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
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

/// OUR server (`mc-server` binary `bin`, offline, peaceful, view/simulation distance 4) for `seed` and `gamemode`;
/// counterpart of [`vanilla`], `props` replace the defaults of the same key. Start it with `.start()`.
pub fn ours(bin: &Path, seed: i64, gamemode: &str, props: &[&str]) -> mc_testkit::Builder {
    mc_testkit::Server::ours(bin)
        .properties([
            "online-mode=false",
            "enforce-secure-profile=false",
            "white-list=false",
            "generate-structures=false",
            "spawn-protection=0",
            "max-tick-time=-1",
            "view-distance=4",
            "simulation-distance=4",
            "enable-rcon=false",
            "enable-query=false",
            "rate-limit=0",
            "difficulty=peaceful",
        ])
        .property("level-seed", &seed.to_string())
        .property("gamemode", gamemode)
        .properties(props.iter().copied())
}
