//! The real `mc-server` binary with an inline-biome flat world and an inline dimension type: `level-type=flat` with
//! `generator-settings` holding an inline `Biome` object starts, generates the spawn area and stores the inline biome in
//! `world_gen_settings.dat`; a world whose stored overworld uses an inline dimension type (and that inline-biome flat
//! generator) is loaded again (not replaced by the default settings) and keeps both inline holders.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use mc_nbt::{CompoundTag, Tag};

const BIOME: &str = r##"{"has_precipitation":false,"temperature":0.5,"downfall":0.5,"effects":{"water_color":"#3f76e4"},"carvers":[],"features":[]}"##;

const INLINE_TYPE: &str = r##"{has_skylight:1b,has_ceiling:0b,has_ender_dragon_fight:0b,coordinate_scale:1.0d,min_y:-64,height:384,logical_height:384,infiniburn:"#minecraft:infiniburn_overworld",ambient_light:0.0f,monster_spawn_light_level:{type:"minecraft:uniform",min_inclusive:0,max_inclusive:7},monster_spawn_block_light_limit:0}"##;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Runs the server in `dir` until it printed `Done (`, stops it and returns its `latest.log`.
fn run_server(dir: &Path) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mc-server"))
        .arg("nogui")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let log_path = dir.join("logs/latest.log");
    let deadline = Instant::now() + Duration::from_secs(900);
    let log = loop {
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        if log.contains("Done (") {
            break log;
        }
        if child.try_wait().unwrap().is_some() {
            panic!("the server exited during startup:\n{log}");
        }
        assert!(
            Instant::now() < deadline,
            "timeout waiting for Done:\n{log}"
        );
        std::thread::sleep(Duration::from_millis(250));
    };
    let _ = writeln!(child.stdin.as_mut().unwrap(), "stop");
    let stop_deadline = Instant::now() + Duration::from_secs(120);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > stop_deadline {
            let _ = Command::new("kill")
                .args(["-INT", &child.id().to_string()])
                .status();
        }
        assert!(
            Instant::now() < stop_deadline + Duration::from_secs(120),
            "the server did not stop"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    std::fs::read_to_string(&log_path).unwrap_or(log)
}

fn world_gen_settings_path(dir: &Path) -> PathBuf {
    dir.join("world/data/minecraft/world_gen_settings.dat")
}

fn read_dimensions(dir: &Path) -> CompoundTag {
    let root = mc_nbt::nbt_io::read_compressed_file(
        &world_gen_settings_path(dir),
        &mut mc_nbt::NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    root.get_compound("data")
        .unwrap()
        .get_compound("dimensions")
        .unwrap()
        .clone()
}

#[test]
#[ignore = "oracle: starts the mc-server binary"]
fn inline_biome_flat_world_and_inline_dimension_type_start() {
    let dir = tempfile::tempdir().unwrap();
    let generator_settings = format!(
        r#"{{"layers":[{{"block":"minecraft:bedrock","height":1}},{{"block":"minecraft:stone","height":2}}],"biome":{BIOME},"features":true,"lakes":true}}"#
    );
    std::fs::write(
        dir.path().join("server.properties"),
        format!(
            "server-port={}\nonline-mode=false\nlevel-type=minecraft:flat\nview-distance=2\nsimulation-distance=2\ngenerator-settings={generator_settings}\n",
            free_port()
        ),
    )
    .unwrap();
    std::fs::write(dir.path().join("eula.txt"), "eula=true\n").unwrap();

    // 1: the properties' inline-biome flat settings
    let log = run_server(dir.path());
    assert!(log.contains("Preparing spawn area: 100%"), "{log}");
    assert!(
        !log.contains("Unknown biome, defaulting to plains"),
        "{log}"
    );
    let dimensions = read_dimensions(dir.path());
    let overworld = dimensions.get_compound("minecraft:overworld").unwrap();
    let settings = overworld
        .get_compound("generator")
        .unwrap()
        .get_compound("settings")
        .unwrap();
    let biome = settings
        .get_compound("biome")
        .expect("the inline biome is stored as an object");
    assert_eq!(biome.get_float("temperature"), Some(0.5));
    assert_eq!(settings.get_byte("lakes"), Some(1));
    assert_eq!(overworld.get_string("type"), Some("minecraft:overworld"));

    // 2: a stored overworld with an inline dimension type (and the inline-biome flat generator) loads again
    let mut root = mc_nbt::nbt_io::read_compressed_file(
        &world_gen_settings_path(dir.path()),
        &mut mc_nbt::NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    let inline_type = mc_nbt::snbt::parse(INLINE_TYPE).unwrap();
    let Some(Tag::Compound(data)) = root.get_mut("data") else {
        panic!("no data")
    };
    let Some(Tag::Compound(dims)) = data.get_mut("dimensions") else {
        panic!("no dimensions")
    };
    let Some(Tag::Compound(overworld)) = dims.get_mut("minecraft:overworld") else {
        panic!("no overworld")
    };
    overworld.put("type", inline_type);
    mc_nbt::nbt_io::write_compressed_file(&root, &world_gen_settings_path(dir.path())).unwrap();

    let log = run_server(dir.path());
    assert!(
        !log.contains("Unable to read or access the world gen settings file"),
        "{log}"
    );
    let dimensions = read_dimensions(dir.path());
    let overworld = dimensions.get_compound("minecraft:overworld").unwrap();
    assert_eq!(
        overworld.get_compound("type").unwrap().get_int("height"),
        Some(384),
        "the inline dimension type is kept"
    );
    let settings = overworld
        .get_compound("generator")
        .unwrap()
        .get_compound("settings")
        .unwrap();
    assert!(
        settings.get_compound("biome").is_some(),
        "the inline biome is kept"
    );
}
