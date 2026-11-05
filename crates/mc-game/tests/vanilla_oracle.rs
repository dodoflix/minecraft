//! Differential tests against the real vanilla server (`ref/server.jar`).
//!
//! `regen_oracle_fixtures` (ignored) starts the vanilla server on three worlds (default, custom data packs, broken data pack),
//! connects as a client that knows no packs (so `registry_data` carries every entry) and stores the raw configuration packets
//! and the server log under `tests/fixtures/oracle/`. The other tests compare our loader against those fixtures.
//! Regenerate: `cargo test -p mc-game --test vanilla_oracle -- --ignored regen_oracle_fixtures --nocapture`.
mod common;

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::ident::Identifier;
use mc_game::registry::keys;
use mc_game::registry::synchronization::{pack_registries, serialize_tags_to_network};
use mc_nbt::Tag;
use mc_protocol::net::Connection;
use mc_protocol::packets::{configuration::*, handshake::*, login::*};
use mc_protocol::PROTOCOL_VERSION;
use mc_world::level_storage::WorldDataConfiguration;
use tokio::net::TcpStream;
use tokio::time::timeout;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oracle")
}

fn jar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/server.jar")
}

struct Captured {
    /// Raw payloads of `registry_data` / `update_tags` packets in arrival order.
    packets: Vec<Vec<u8>>,
    log: Vec<String>,
}

/// Run the vanilla server in a scratch dir after `setup(world_dir)`; with `connect` also capture the configuration packets.
async fn run_vanilla(setup: impl FnOnce(&Path), connect: bool) -> Captured {
    let dir = std::env::temp_dir().join(format!("mc-game-oracle-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(
        dir.join("server.properties"),
        format!("online-mode=false\nwhite-list=false\nenforce-whitelist=false\nenforce-secure-profile=false\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\nenable-rcon=false\nenable-query=false\nrate-limit=0\n"),
    )
    .unwrap();
    setup(&dir.join("world"));
    let link = dir.join("oracle-vanilla.jar");
    std::os::unix::fs::symlink(jar().canonicalize().unwrap(), &link).unwrap();
    let mut child = Command::new("java")
        .args(["-Xmx1G", "-jar"])
        .arg(&link)
        .arg("nogui")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn java");
    let _stdin = child.stdin.take();
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let reader = {
        let log = log.clone();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let done = line.contains("Done (");
                log.lock().unwrap().push(line);
                if done {
                    let _ = tx.send(());
                }
            }
        })
    };
    let mut packets = Vec::new();
    if connect {
        rx.recv_timeout(Duration::from_secs(180)).expect("vanilla did not finish starting");
        tokio::time::sleep(Duration::from_secs(3)).await;
        packets = capture_configuration(port).await;
        let _ = child.kill();
    }
    let _ = child.wait_timeout_or_kill();
    let _ = reader.join();
    let _ = std::fs::remove_dir_all(&dir);
    let log = log.lock().unwrap().clone();
    Captured { packets, log }
}

trait WaitOrKill {
    fn wait_timeout_or_kill(&mut self) -> std::io::Result<()>;
}

impl WaitOrKill for std::process::Child {
    fn wait_timeout_or_kill(&mut self) -> std::io::Result<()> {
        for _ in 0..600 {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.kill()?;
        self.wait().map(|_| ())
    }
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as u64
}

macro_rules! send {
    ($conn:expr, $enum:ident :: $variant:ident($packet:expr)) => {{
        let mut buf = Vec::new();
        $enum::$variant($packet).encode_payload(&mut buf).unwrap();
        $conn.write_payload(&buf).await.unwrap();
        $conn.flush().await.unwrap();
    }};
}

async fn capture_configuration(port: u16) -> Vec<Vec<u8>> {
    let mut c = Connection::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap());
    send!(
        c,
        ServerboundHandshakePacket::ClientIntention(ClientIntentionPacket { protocol_version: PROTOCOL_VERSION, host_name: "localhost".into(), port, intention: ClientIntent::Login })
    );
    send!(c, ServerboundLoginPacket::Hello(ServerboundHelloPacket { name: "OracleBot".into(), profile_id: uuid::Uuid::from_u128(1) }));
    loop {
        let payload = timeout(Duration::from_secs(30), c.read_payload()).await.unwrap().unwrap();
        match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
            ClientboundLoginPacket::LoginCompression(p) => c.set_compression(p.compression_threshold, false),
            ClientboundLoginPacket::LoginFinished(_) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    send!(c, ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket));
    send!(c, ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket { information: Default::default() }));
    let mut out = Vec::new();
    loop {
        let payload = timeout(Duration::from_secs(30), c.read_payload()).await.unwrap().unwrap();
        match ClientboundConfigurationPacket::decode_payload(&payload).unwrap() {
            ClientboundConfigurationPacket::SelectKnownPacks(_) => send!(c, ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks { known_packs: vec![] })),
            ClientboundConfigurationPacket::KeepAlive(p) => send!(c, ServerboundConfigurationPacket::KeepAlive(ServerboundKeepAlivePacket { id: p.id })),
            ClientboundConfigurationPacket::Ping(p) => send!(c, ServerboundConfigurationPacket::Pong(ServerboundPongPacket { id: p.id })),
            ClientboundConfigurationPacket::FinishConfiguration(_) => break,
            ClientboundConfigurationPacket::RegistryData(_) | ClientboundConfigurationPacket::UpdateTags(_) => out.push(payload),
            _ => {}
        }
    }
    out
}

fn write_fixture(name: &str, captured: &Captured) {
    let dir = fixture_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut bin = Vec::new();
    for p in &captured.packets {
        bin.extend((p.len() as u32).to_be_bytes());
        bin.extend(p);
    }
    std::fs::write(dir.join(format!("{name}.bin")), bin).unwrap();
    // `.log` is git-ignored: the server logs would silently be missing from a clone
    std::fs::write(dir.join(format!("{name}.server-log.txt")), captured.log.join("\n") + "\n").unwrap();
}

fn read_packets(name: &str) -> Vec<ClientboundConfigurationPacket> {
    let bin = std::fs::read(fixture_dir().join(format!("{name}.bin"))).unwrap();
    let mut out = Vec::new();
    let mut rest = &bin[..];
    while !rest.is_empty() {
        let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        out.push(ClientboundConfigurationPacket::decode_payload(&rest[4..4 + len]).unwrap());
        rest = &rest[4 + len..];
    }
    out
}

fn read_log(name: &str) -> Vec<String> {
    std::fs::read_to_string(fixture_dir().join(format!("{name}.server-log.txt"))).unwrap().lines().map(str::to_owned).collect()
}

#[test]
#[ignore]
fn regen_oracle_fixtures() {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    rt.block_on(async {
        write_fixture("default", &run_vanilla(|_| {}, true).await);
        write_fixture("custom", &run_vanilla(common::write_custom_packs, true).await);
        write_fixture("broken", &run_vanilla(common::write_broken_packs, false).await);
    });
}

// ---- comparisons ------------------------------------------------------------------------------------------------

/// Vanilla's `registry_data` + `update_tags` of a scenario as `registry -> [(entry, nbt)]` and `registry -> tag -> ids`.
type VanillaView = (Vec<(String, Vec<(Identifier, Option<Tag>)>)>, BTreeMap<String, BTreeMap<Identifier, Vec<i32>>>);

fn vanilla_view(name: &str) -> VanillaView {
    let mut registries = Vec::new();
    let mut tags = BTreeMap::new();
    for p in read_packets(name) {
        match p {
            ClientboundConfigurationPacket::RegistryData(r) => registries.push((r.registry.to_string(), r.entries.into_iter().map(|e| (e.id, e.data)).collect())),
            ClientboundConfigurationPacket::UpdateTags(t) => {
                for (reg, payload) in t.tags {
                    tags.insert(reg.to_string(), payload.tags.into_iter().collect::<BTreeMap<_, _>>());
                }
            }
            _ => {}
        }
    }
    (registries, tags)
}

fn compare_scenario(name: &str, setup: impl FnOnce(&Path)) -> Vec<String> {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let _ = common::take_logs();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    setup(&world);
    let (_, stem) = common::load_world(&world, WorldDataConfiguration::DEFAULT, true).expect("loads");
    let ours = common::take_logs();
    let (vanilla_registries, vanilla_tags) = vanilla_view(name);

    // registry_data: same registries in the same order, same entries with the same ids
    let access = stem.registries.composite_access();
    let packed = pack_registries(access, &HashSet::new());
    assert_eq!(packed.iter().map(|p| &p.0).collect::<Vec<_>>(), vanilla_registries.iter().map(|p| &p.0).collect::<Vec<_>>(), "registry order");
    let typed = [keys::DIMENSION_TYPE, keys::BIOME, keys::DAMAGE_TYPE, keys::CHAT_TYPE, keys::ENCHANTMENT];
    let mut approx: Vec<String> = Vec::new();
    for ((key, entries), (vkey, ventries)) in packed.iter().zip(&vanilla_registries) {
        assert_eq!(key, vkey);
        assert_eq!(entries.iter().map(|e| &e.id).collect::<Vec<_>>(), ventries.iter().map(|e| &e.0).collect::<Vec<_>>(), "{key}: entry names/ids");
        for (e, (id, vdata)) in entries.iter().zip(ventries) {
            if e.data == *vdata {
                continue;
            }
            if typed.contains(&key.as_str()) {
                panic!("{key} {id}: network NBT differs at {}\nours:    {:?}\nvanilla: {vdata:?}", first_diff(e.data.as_ref(), vdata.as_ref(), ""), e.data);
            }
            if std::env::var("DUMP_DIFFS").is_ok_and(|v| key.contains(&v)) {
                eprintln!("{key} {id}\n ours:    {:?}\n vanilla: {vdata:?}", e.data);
            }
            approx.push(format!("{key} {id}"));
        }
    }
    let mut per_registry: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &approx {
        *per_registry.entry(a.split(' ').next().unwrap()).or_default() += 1;
    }
    eprintln!("[{name}] untyped registry entries whose NBT differs from vanilla's codec output: {} {per_registry:?}", approx.len());

    // update_tags: every tag of every registry vanilla sent, same ids in the same order
    let ours_tags: BTreeMap<String, BTreeMap<Identifier, Vec<i32>>> = serialize_tags_to_network(&stem.registries)
        .into_iter()
        .map(|(k, p)| (k, p.tags.into_iter().map(|(t, ids)| (t, ids.into_iter().map(|i| i as i32).collect())).collect()))
        .collect();
    assert_eq!(ours_tags.keys().collect::<Vec<_>>(), vanilla_tags.keys().collect::<Vec<_>>(), "registries with tags");
    for (k, v) in &vanilla_tags {
        assert_eq!(&ours_tags[k], v, "tags of {k}");
    }
    ours
}

/// `[HH:MM:SS] [Server thread/INFO]: message` -> `INFO: message` (continuation lines are dropped).
fn vanilla_messages(log: &[String]) -> Vec<String> {
    log.iter()
        .filter_map(|l| {
            let rest = l.split_once("] [")?.1;
            let (thread_level, msg) = rest.split_once("]: ")?;
            let level = thread_level.rsplit_once('/')?.1;
            Some(format!("{level}: {msg}"))
        })
        .collect()
}

fn normalize(s: &str, world: &str) -> String {
    s.replace(world, "world")
}

#[test]
fn default_world_matches_vanilla() {
    compare_scenario("default", |_| {});
    // vanilla only logs the pack selection when something is found; a default world is quiet
    let v = vanilla_messages(&read_log("default"));
    assert!(!v.iter().any(|m| m.contains("data pack") || m.contains("Registry loading")), "{v:?}");
}

#[test]
fn custom_packs_world_matches_vanilla() {
    let mut world_path = String::new();
    let ours = compare_scenario("custom", |w| {
        world_path = w.display().to_string();
        common::write_custom_packs(w);
    });
    let ours: Vec<String> = ours.iter().map(|l| normalize(l.lines().next().unwrap(), &world_path)).collect();
    let ours: Vec<String> = ours.iter().map(|l| l.replace("world/datapacks", "./world/datapacks").replace("././", "./")).collect();
    let wanted = |m: &String| m.contains("pack") || m.contains("Registry loading");
    let vanilla: Vec<String> = vanilla_messages(&read_log("custom")).into_iter().filter(wanted).collect();
    assert_eq!(ours.into_iter().filter(wanted).collect::<Vec<_>>(), vanilla);
}

#[test]
fn broken_pack_fails_like_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::write_broken_packs(&world);
    let err = match common::load_world(&world, WorldDataConfiguration::DEFAULT, true) {
        Err(e) => e,
        Ok(_) => panic!("vanilla refuses this world, so must we"),
    };
    let _ = common::take_logs();
    let mc_game::server::world_loader::WorldLoadError::Registries(failure) = err else { panic!("{err}") };

    // what vanilla reported: (registry, element, "Caused by" lines)
    let log = read_log("broken");
    let mut expected: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut registry = String::new();
    let mut in_errors = false;
    for line in &log {
        if line.contains("Registry loading errors:") {
            in_errors = true;
        } else if in_errors {
            if let Some(r) = line.strip_prefix("> Errors in registry ") {
                registry = r.trim_end_matches(':').to_owned();
            } else if let Some(e) = line.strip_prefix(">> Errors in element ") {
                expected.push((registry.clone(), e.trim_end_matches(':').to_owned(), Vec::new()));
            } else if let Some(c) = line.strip_prefix("Caused by: ") {
                expected.last_mut().unwrap().2.push(c.to_owned());
            } else if line.starts_with('[') {
                in_errors = false;
            }
        }
    }
    assert_eq!(expected.len(), 3);
    let ours: Vec<(String, String, Vec<String>)> = failure
        .errors
        .iter()
        .map(|((r, id), e)| (r.clone(), id.to_string(), e.cause.iter().map(|(c, m)| format!("{c}: {m}")).collect()))
        .collect();
    assert_eq!(ours.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>(), expected.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>());
    for (o, v) in ours.iter().zip(&expected) {
        if o.1.ends_with("bad_syntax") {
            assert!(o.2[0].starts_with("com.google.gson.JsonSyntaxException: ") && v.2[0].starts_with("com.google.gson.JsonSyntaxException: "));
        } else {
            assert_eq!(o.2[0], v.2[0], "{}", o.1);
        }
    }
    // the first lines of the full report are the same shape as vanilla's
    let details = failure.full_details();
    assert!(details.starts_with("> Errors in registry minecraft:chat_type:\n>> Errors in element test:missing_field:\njava.lang.IllegalStateException: Failed to parse test:missing_field from pack file/broken\nCaused by: "));
    let _ = Tag::End;
}

/// Path of the first difference between two NBT trees (debugging aid for the typed registries).
fn first_diff(a: Option<&Tag>, b: Option<&Tag>, path: &str) -> String {
    match (a, b) {
        (Some(Tag::Compound(x)), Some(Tag::Compound(y))) => {
            for (k, v) in x.iter() {
                match y.get(k) {
                    None => return format!("{path}/{k} (only ours)"),
                    Some(w) if w != v => return first_diff(Some(v), Some(w), &format!("{path}/{k}")),
                    _ => {}
                }
            }
            for (k, _) in y.iter() {
                if x.get(k).is_none() {
                    return format!("{path}/{k} (only vanilla)");
                }
            }
            path.to_owned()
        }
        (Some(Tag::List(x)), Some(Tag::List(y))) => {
            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
                if v != w {
                    return first_diff(Some(v), Some(w), &format!("{path}[{i}]"));
                }
            }
            format!("{path} (list length)")
        }
        (a, b) => format!("{path}: ours {a:?} vanilla {b:?}"),
    }
}
