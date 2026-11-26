//! `/perf start` ... `/perf stop` on the real vanilla server and on `mc-server`: what the report zip holds. The rows of
//! `system.txt` that `MinecraftServer.fillSystemReport` writes (the JVM rows cannot agree) and, per case, a data pack
//! and `--serverId` that change two of them.

use std::io::Read as _;
use std::path::Path;

use mc_testkit::Server;

/// The rows `MinecraftServer.fillSystemReport` writes itself, in vanilla's order; `Player Count` and the server's own
/// rows (`Type`, `Is Modded`, ...) are the same on both sides, JVM details are not part of the report here.
const ROWS: [&str; 9] = [
    "Server Running",
    "Player Count",
    "Active Data Packs",
    "Available Data Packs",
    "Enabled Feature Flags",
    "World Generation",
    "World Seed",
    "Suppressed Exceptions",
    "Server Id",
];

/// A data pack that adds one world generation entry: its entries are not from a known pack, so the lifecycle of the
/// world generation settings is no longer stable.
fn world_gen_pack(server_dir: &Path) {
    let pack = server_dir.join("world/datapacks/extra");
    let density = pack.join("data/extra/worldgen/density_function");
    std::fs::create_dir_all(&density).expect("pack directories");
    std::fs::write(
        pack.join("pack.mcmeta"),
        r#"{"pack":{"description":"extra","min_format":121,"max_format":121}}"#,
    )
    .expect("pack.mcmeta");
    std::fs::write(density.join("flat.json"), "0.5").expect("density function");
}

fn nothing(_: &Path) {}

/// The rows of `system.txt` of one `/perf` run, as `key: value` lines.
fn system_rows(server: &Server) -> Vec<String> {
    server.cmd("perf start");
    server
        .wait_log("Started a 10-second performance profiling run", 30)
        .expect("perf started");
    server.cmd("perf stop");
    let saved = server
        .wait_log(r"Created debug report in \S+\.zip", 60)
        .expect("debug report");
    let name = saved
        .rsplit_once("in ")
        .expect("zip name")
        .1
        .trim()
        .to_owned();
    let file = std::fs::File::open(server.dir().join("debug/profiling").join(&name)).expect("zip");
    let mut zip = zip::ZipArchive::new(file).expect("zip archive");
    let mut system = String::new();
    zip.by_name("system.txt")
        .expect("system.txt")
        .read_to_string(&mut system)
        .expect("utf-8");
    let mut rows = Vec::new();
    let mut keep = false;
    for line in system.lines() {
        // the lines of a multi-line value (the suppressed exceptions) are indented twice
        if line.starts_with("\t\t") {
            if keep {
                rows.push(mask_ages(line));
            }
            continue;
        }
        let Some((key, value)) = line.trim_start().split_once(": ") else {
            keep = false;
            continue;
        };
        keep = ROWS.contains(&key);
        if keep {
            rows.push(format!("{key}: {value}"));
        }
    }
    rows
}

/// `(1234ms ago)` of the latest suppressed exceptions.
fn mask_ages(line: &str) -> String {
    match line.rsplit_once(" (") {
        Some((head, tail)) if tail.ends_with("ms ago)") => format!("{head} (<n>ms ago)"),
        _ => line.to_owned(),
    }
}

async fn run_both(
    props: &[&str],
    args: &[&str],
    prepare: fn(&Path),
    before_report: impl AsyncFn(&Server),
) {
    let mut vanilla = mc_testbot::harness::vanilla(7, "survival", props).prepare(prepare);
    let mut ours = crate::common::ours(7, "survival", props).prepare(prepare);
    for arg in args {
        vanilla = vanilla.arg(arg);
        ours = ours.arg(arg);
    }
    let vanilla = vanilla.start().expect("start vanilla");
    before_report(&vanilla).await;
    let theirs = system_rows(&vanilla);
    vanilla.stop();
    drop(vanilla);
    let ours = ours.start().expect("start our server");
    before_report(&ours).await;
    let mine = system_rows(&ours);
    ours.stop();
    println!("{}", theirs.join("\n"));
    assert!(!theirs.is_empty(), "no rows");
    assert_eq!(theirs, mine, "vanilla (left) vs ours (right)");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn system_rows_of_the_default_world_equal_vanilla() {
    run_both(&[], &[], nothing, async |_| {}).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn system_rows_with_a_server_id_and_a_data_pack_equal_vanilla() {
    run_both(
        &["initial-enabled-packs=vanilla,file/extra"],
        &["--serverId", "perf-dual"],
        world_gen_pack,
        async |_| {},
    )
    .await;
}

/// A handler that throws on the main thread (a negative block-change sequence) is suppressed and counted under
/// `packet/serverbound/<packet>` in the suppressed exceptions of the system report.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn suppressed_packet_exception_is_in_the_system_rows() {
    use mc_protocol::packets::play::{
        InteractionHand, ServerboundPlayPacket, ServerboundUseItemPacket,
    };
    run_both(&[], &[], nothing, async |server: &Server| {
        let bot = crate::common::join("127.0.0.1", server.port, "Bot").await;
        bot.send(ServerboundPlayPacket::UseItem(ServerboundUseItemPacket {
            hand: InteractionHand::MainHand,
            sequence: -1,
            y_rot: 0.0,
            x_rot: 0.0,
        }));
        server
            .wait_log("Failed to handle packet .*suppressing error", 20)
            .expect("handler error is logged and suppressed");
        bot.disconnect().await;
    })
    .await;
}
