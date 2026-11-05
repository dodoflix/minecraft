//! Interop with the REAL vanilla classes (`fixtures/level_storage/VanillaLevelStorage.java`, run through
//! `scripts/vanilla-java.sh`): vanilla reads the files this module writes, resolves identical paths, and
//! `session.lock` excludes in both directions. Also loads a level.dat rewritten by this module into the vanilla server.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag, snbt};
use mc_world::level_storage::*;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn require_java() {
    assert!(root().join("scripts/vanilla-java.sh").exists() && root().join("ref/bundle").exists(), "ref/bundle or scripts/vanilla-java.sh missing: run scripts/setup-ref.sh");
    assert!(Command::new("java").arg("-version").output().is_ok(), "java not found on PATH");
}

fn oracle() -> PathBuf {
    root().join("crates/mc-world/tests/fixtures/level_storage/VanillaLevelStorage.java")
}

fn java_command(args: &[&str]) -> Command {
    let mut cmd = Command::new(root().join("scripts/vanilla-java.sh"));
    cmd.arg(oracle()).args(args);
    cmd
}

/// stdout lines, with vanilla's `[time] [thread/INFO]: [STDOUT]: ` logger prefix removed (it appears once
/// Bootstrap redirected System.out).
fn run_java(args: &[&str]) -> Vec<String> {
    let out = java_command(args).output().expect("run java");
    assert!(out.status.success(), "java failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| match l.find("[STDOUT]: ") {
            Some(i) => Some(l[i + "[STDOUT]: ".len()..].to_owned()),
            None if l.contains("/INFO]") || l.contains("/WARN]") => None,
            None => Some(l.to_owned()),
        })
        .collect()
}

fn field<'a>(lines: &'a [String], prefix: &str) -> &'a str {
    lines.iter().find_map(|l| l.strip_prefix(prefix)).unwrap_or_else(|| panic!("no `{prefix}` in {lines:#?}"))
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn vanilla_reads_what_we_write() {
    require_java();
    let tmp = tempfile::tempdir().unwrap();
    let saves = tmp.path().join("saves");
    copy_dir(&root().join("crates/mc-world/tests/fixtures/level_storage/world"), &saves.join("world"));
    let source = LevelStorageSource::create_default(&saves).unwrap();

    let player = uuid::Uuid::from_u128(0x1234_5678_9abc_4def_8123_4567_89ab_cdef);
    let our_data;
    let expected_player;
    {
        let mut access = source.create_access("world").unwrap();
        let data = access.get_unfixed_data_tag(false).unwrap();
        let mut level = PrimaryLevelData::parse(&data, SpecialWorldProperty::None);
        level.settings.game_type = GameType::Adventure;
        level.settings.allow_commands = true;
        level.settings.difficulty_settings = DifficultySettings { difficulty: Difficulty::Hard, hardcore: true, locked: true };
        level.settings.level_name = "Ünïcode ☃ world".to_owned();
        level.game_time = 123_456_789_012;
        level.set_modded_info("rust-server", true);
        level.respawn_data = RespawnData { dimension: Identifier::parse("minecraft:the_nether").unwrap(), pos: mc_core::pos::block_pos::BlockPos::new(-5, 70, 12), yaw: 90.5, pitch: -12.25 };
        level.settings.data_configuration.enabled_features = FeatureFlagSet::VANILLA.join(FeatureFlagSet::TRADE_REBALANCE);
        level.removed_feature_flags.insert("minecraft:old_flag".to_owned());
        access.save_data_tag(&level, Some(player)).unwrap();
        our_data = access.get_unfixed_data_tag(false).unwrap();

        let mut tag = CompoundTag::new();
        tag.put_float("Health", 12.5);
        tag.put("Pos", Tag::List([Tag::Double(1.5), Tag::Double(64.0), Tag::Double(-3.25)].into_iter().collect()));
        tag.put_string("Dimension", "minecraft:overworld");
        access.create_player_storage().unwrap().save(&player, tag.clone()).unwrap();
        tag.put_int("DataVersion", WORLD_VERSION);
        expected_player = tag;
        access.close().unwrap();
    }

    let lines = run_java(&["summary", saves.to_str().unwrap(), "world", &player.to_string()]);
    assert_eq!(field(&lines, "hasWorldData="), "true");

    // vanilla's view of level.dat is exactly what we wrote
    let vanilla_data = snbt::parse_compound(field(&lines, "DATA ")).unwrap();
    assert_eq!(vanilla_data, our_data);
    assert_eq!(snbt::parse_compound(field(&lines, "PLAYER ")).unwrap(), expected_player);

    // vanilla's summary of it == ours
    let ours = {
        let access = source.create_access("world").unwrap();
        access.fix_and_get_summary().unwrap()
    };
    let expected = format!(
        "summary id=world name={} mode=ADVENTURE hardcore=true commands=true experimental={} lastPlayed={} manual=false fileFix=false compatible=true backup=NONE version=26.3 disabled=false",
        ours.level_name(),
        ours.experimental,
        ours.last_played()
    );
    assert_eq!(field(&lines, "summary "), &expected["summary ".len()..]);
    assert!(ours.experimental, "trade_rebalance is an experimental flag");

    // identical resource and dimension paths
    let access = source.create_access("world").unwrap();
    let world = source.get_level_path("world");
    for res in [
        LevelResource::PLAYER_DATA_DIR,
        LevelResource::PLAYER_STATS_DIR,
        LevelResource::PLAYER_ADVANCEMENTS_DIR,
        LevelResource::DATA,
        LevelResource::LEVEL_DATA_FILE,
        LevelResource::OLD_LEVEL_DATA_FILE,
        LevelResource::ICON_FILE,
        LevelResource::LOCK_FILE,
        LevelResource::DATAPACK_DIR,
        LevelResource::GENERATED_DIR,
    ] {
        let rel = access.get_level_path(res).strip_prefix(&world).unwrap().to_path_buf();
        assert_eq!(field(&lines, &format!("path {} ", res.id())), rel.to_str().unwrap());
    }
    for dim in ["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end", "mymod:foo/bar"] {
        let rel = access.get_dimension_path(&Identifier::parse(dim).unwrap()).unwrap();
        assert_eq!(field(&lines, &format!("dim {dim} ")), rel.strip_prefix(&world).unwrap().to_str().unwrap());
    }
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn session_lock_excludes_vanilla_and_vice_versa() {
    require_java();
    // Rust holds the lock -> vanilla sees it locked and cannot create it (same message as ours)
    let tmp = tempfile::tempdir().unwrap();
    let held = DirectoryLock::create(tmp.path()).unwrap();
    let lines = run_java(&["lock", tmp.path().to_str().unwrap()]);
    assert_eq!(field(&lines, "isLocked="), "true");
    let ours = DirectoryLock::create(tmp.path()).err().unwrap().to_string();
    assert_eq!(field(&lines, "FAILED "), format!("LockException: {ours}"));
    drop(held);
    let lines = run_java(&["lock", tmp.path().to_str().unwrap()]);
    assert_eq!(field(&lines, "isLocked="), "false");
    assert!(lines.iter().any(|l| l == "CREATED"));

    // vanilla holds the lock -> Rust sees it locked and fails with the vanilla outcome
    let dir = tempfile::tempdir().unwrap();
    let mut child = java_command(&["hold", dir.path().to_str().unwrap(), "20000"]).stdout(Stdio::piped()).spawn().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "java exited before holding the lock");
        if line.trim() == "HELD" {
            break;
        }
    }
    let locked = DirectoryLock::is_locked(dir.path());
    let err = DirectoryLock::create(dir.path());
    let level_access = LevelStorageSource::create_default(dir.path().join("..")).unwrap().create_access(dir.path().file_name().unwrap().to_str().unwrap());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(locked.unwrap(), "vanilla-held lock must be visible");
    let err = err.err().expect("create must fail while vanilla holds the lock");
    assert!(err.to_string().ends_with("session.lock: already locked (possibly by other Minecraft instance?)"), "{err}");
    assert!(level_access.is_err());
    // released once the holder is gone
    assert!(!DirectoryLock::is_locked(dir.path()).unwrap());
}

/// The real vanilla server must load a world whose level.dat / saved data were rewritten by this module
/// (runs in a temporary directory on a copy of the reference world from `mc_testkit::worldref`).
#[test]
#[ignore = "oracle: starts the vanilla server on the vanilla reference world (generated by mc_testkit::worldref)"]
fn vanilla_server_loads_our_level_dat() {
    let worldref = mc_testkit::worldref().expect("reference world");
    let tmp = tempfile::tempdir().unwrap();
    let server_dir = tmp.path().to_path_buf();
    let world = server_dir.join("world");
    for sub in ["data", "dimensions", "players", "datapacks"] {
        copy_dir(&worldref.join(sub), &world.join(sub));
    }
    fs::copy(worldref.join("level.dat"), world.join("level.dat")).unwrap();

    // Rewrite everything through this module.
    let source = LevelStorageSource::create_default(&server_dir).unwrap();
    {
        let mut access = source.create_access("world").unwrap();
        let data = access.get_unfixed_data_tag(false).unwrap();
        let mut level = PrimaryLevelData::parse(&data, SpecialWorldProperty::None);
        level.game_time = 5_000_000;
        level.settings.level_name = "world".to_owned();
        access.save_data_tag(&level, None).unwrap();
        access.save_data_tag(&level, None).unwrap(); // second save exercises level.dat_old rotation
        assert!(access.get_level_path(LevelResource::OLD_LEVEL_DATA_FILE).is_file());
        for dir in [world.join("data/minecraft"), world.join("dimensions/minecraft/overworld/data/minecraft")] {
            let storage = SavedDataStorage::new(dir.parent().unwrap());
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let id = Identifier::new("minecraft", path.file_stem().unwrap().to_str().unwrap()).unwrap();
                let tag = storage.get(&id).unwrap();
                storage.save(&id, tag).unwrap();
            }
        }
        let wgs = WorldGenSettings::read_existing(&world).unwrap();
        wgs.write(&world).unwrap();
        access.close().unwrap();
    }

    let server = mc_testkit::Server::vanilla()
        .dir(&server_dir)
        .properties([
            "online-mode=false",
            "enforce-secure-profile=false",
            "white-list=false",
            "spawn-protection=0",
            "view-distance=6",
            "simulation-distance=6",
            "gamemode=creative",
            "difficulty=peaceful",
        ])
        .start()
        .expect("start vanilla");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(DirectoryLock::is_locked(&world).unwrap(), "vanilla server must hold our session.lock");
        assert!(source.create_access("world").is_err());
        server.cmd("seed");
        assert!(server.wait_log(r"Seed: .*12345", 30).is_some(), "seed line missing: {}", server.log());
        server.cmd("time query gametime");
        let line = server.wait_log(r"The game time is [0-9]+ tick", 30).unwrap_or_else(|| panic!("time line missing: {}", server.log()));
        let ticks: i64 = line.rsplit("The game time is ").next().unwrap().split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("bad time line {line:?}"));
        assert!((5_000_000..5_100_000).contains(&ticks), "game time not kept: {line}");
        let log = server.log();
        for bad in ["/ERROR]", "Unable to read or access", "Failed to load", "Failed to save", "corrupt"] {
            assert!(!log.contains(bad), "unexpected `{bad}` in vanilla log:\n{log}");
        }
    }));
    server.stop();
    // what vanilla wrote back must still parse with our code
    if result.is_ok() {
        let access = source.create_access("world").unwrap();
        let data = access.get_unfixed_data_tag(false).unwrap();
        let level = PrimaryLevelData::parse(&data, SpecialWorldProperty::None);
        assert!(level.game_time >= 5_000_000);
        assert!(level.extra.is_empty(), "vanilla wrote unexpected level.dat keys: {:?}", level.extra);
        assert_eq!(PrimaryLevelData::parse(&data, SpecialWorldProperty::None).to_tag_at(None, data.get_long("LastPlayed").unwrap()), data);
        drop(access);
    }
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
