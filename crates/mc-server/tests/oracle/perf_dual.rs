//! `/perf start` ... `/perf stop` on the real vanilla server and on `mc-server`: what the report zip holds. The rows of
//! `system.txt` that `MinecraftServer.fillSystemReport` writes (the JVM rows cannot agree) and, per case, a data pack
//! and `--serverId` that change two of them.

use std::collections::{BTreeMap, BTreeSet};
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
    let system = report_entries(server)
        .remove("system.txt")
        .expect("system.txt");
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

/// Every entry of the report zip of a `/perf start` ... `/perf stop` run, by name, as text. The stop comes two seconds
/// after the start: an immediate one races the next tick (vanilla then ignores it and records the full ten seconds).
fn report_entries(server: &Server) -> BTreeMap<String, String> {
    server.cmd("perf start");
    server
        .wait_log("Started a 10-second performance profiling run", 30)
        .expect("perf started");
    std::thread::sleep(std::time::Duration::from_secs(2));
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
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        if entry.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read entry");
        out.insert(
            entry.name().expect("entry name").into_owned(),
            String::from_utf8_lossy(&bytes).into_owned(),
        );
    }
    out
}

/// Numbers (`12`, `-3.5`, `4.9E7`) replaced by `N`: what a value looks like, not what it is.
fn mask_numbers(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            out.push(c);
            continue;
        }
        if out.ends_with('-') && !out[..out.len() - 1].ends_with(|p: char| p.is_alphanumeric()) {
            out.pop();
        }
        let mut prev = c;
        while let Some(&n) = chars.peek() {
            let exponent_sign = matches!(n, '-' | '+') && matches!(prev, 'E' | 'e');
            if !(n.is_ascii_digit() || n == '.' || n == 'E' || exponent_sign) {
                break;
            }
            prev = n;
            chars.next();
        }
        out.push('N');
    }
    out
}

fn lines_of(zip: &BTreeMap<String, String>, name: &str) -> Vec<String> {
    zip.get(name)
        .map_or_else(Vec::new, |t| t.lines().map(str::to_owned).collect())
}

fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

/// Collects what differs; every difference is printed before the test fails.
#[derive(Default)]
struct Diffs(Vec<String>);

impl Diffs {
    /// Lines in the order of the report: what only one side has (the first few), or the order itself.
    fn eq(&mut self, what: &str, vanilla: Vec<String>, ours: Vec<String>) {
        if vanilla == ours {
            return;
        }
        let only = |a: &[String], b: &[String]| -> Vec<String> {
            a.iter()
                .filter(|l| !b.contains(l))
                .take(6)
                .cloned()
                .collect()
        };
        self.0.push(format!(
            "{what} ({} vs {} lines)\n  only vanilla: {:#?}\n  only ours:    {:#?}",
            vanilla.len(),
            ours.len(),
            only(&vanilla, &ours),
            only(&ours, &vanilla)
        ));
    }
}

/// What the ticks of one metrics csv look like: per row as many cells as the header has, a leading tick number
/// counting from one and numbers in the rest.
fn check_metrics_csv(diffs: &mut Diffs, side: &str, name: &str, text: &str) {
    let mut rows = text.lines();
    let header = rows.next().unwrap_or_default().split(',').count();
    for (i, row) in rows.enumerate() {
        let cells: Vec<&str> = row.split(',').collect();
        let ok = cells.len() == header
            && cells[0] == (i + 1).to_string()
            && cells.iter().all(|c| c.parse::<f64>().is_ok());
        if !ok {
            diffs
                .0
                .push(format!("{side} {name} row {}: {row:?}", i + 1));
            return;
        }
    }
}

/// The names of the profile lines down to the third level, without the counters (`#name n/m`).
fn profile_names(profile: &[String]) -> BTreeSet<String> {
    profile
        .iter()
        .filter(|l| l.starts_with("[00]") || l.starts_with("[01]") || l.starts_with("[02]"))
        .filter(|l| !l.contains('#') && !l.contains("total:"))
        .filter_map(|l| l.split_once("] ").map(|(_, rest)| rest))
        .map(|rest| {
            let name = rest.trim_start_matches(['|', ' ']);
            name.split('(').next().unwrap_or(name).to_owned()
        })
        .collect()
}

/// A quiet flat world with one idle player, settled for a few seconds, then a two-second `/perf` run.
async fn quiet_report(builder: mc_testkit::Builder) -> BTreeMap<String, String> {
    let server = builder.start().expect("start");
    let bot = crate::common::join("127.0.0.1", server.port, "Bot").await;
    server.cmd("tp Bot 8 -60 8");
    std::thread::sleep(std::time::Duration::from_secs(5));
    let entries = report_entries(&server);
    bot.disconnect().await;
    server.stop();
    entries
}

/// The report zip next to vanilla's, for what can agree: the entries, the columns and shapes of every csv, the settled
/// chunk, entity and block entity tables, the stats, the crash-report layout, game rules, server properties and the
/// layout of the profile. What is JVM-internal differs by design and is only checked for its format: `threads.txt`
/// (no daemon flag, priority or frames, thread names cut at 15 bytes), `classpath.txt`, the `deviations/` entries
/// (vanilla writes them for slow ticks only), the stack frames of the crash report and the executor columns (no queues
/// behind them here, so they read zero).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn report_zip_matches_vanilla() {
    let props = ["level-type=minecraft:flat", "generate-structures=false"];
    let vanilla = quiet_report(mc_testbot::harness::vanilla(7, "survival", &props)).await;
    let ours = quiet_report(crate::common::ours(7, "survival", &props)).await;
    let mut diffs = Diffs::default();

    let names = |zip: &BTreeMap<String, String>| -> Vec<String> {
        zip.keys()
            .filter(|n| !n.starts_with("server/deviations/"))
            .cloned()
            .collect()
    };
    diffs.eq("entry names", names(&vanilla), names(&ours));

    for (side, zip) in [("vanilla", &vanilla), ("ours", &ours)] {
        for (name, text) in zip.iter().filter(|(n, _)| n.starts_with("server/metrics/")) {
            check_metrics_csv(&mut diffs, side, name, text);
        }
    }
    // hash-ordered in vanilla (by the host's CPU count and the levels): the columns of these three
    for name in [
        "server/metrics/consecutive-executors.csv",
        "server/metrics/cpu.csv",
        "server/metrics/event-loops.csv",
    ] {
        let header = |zip: &BTreeMap<String, String>| {
            sorted(
                lines_of(zip, name)
                    .first()
                    .map(|h| h.split(',').map(str::to_owned).collect())
                    .unwrap_or_default(),
            )
        };
        diffs.eq(name, header(&vanilla), header(&ours));
    }
    for name in ["server/metrics/jvm.csv", "server/metrics/ticking.csv"] {
        diffs.eq(
            name,
            lines_of(&vanilla, name).into_iter().take(1).collect(),
            lines_of(&ours, name).into_iter().take(1).collect(),
        );
    }

    for level in ["overworld", "the_nether", "the_end"] {
        let dir = format!("server/levels/minecraft/{level}");
        for file in ["block_entities.csv", "entities.csv", "entity_chunks.csv"] {
            let name = format!("{dir}/{file}");
            diffs.eq(
                &name,
                sorted(lines_of(&vanilla, &name)),
                sorted(lines_of(&ours, &name)),
            );
        }
        // Chunks the player keeps loaded (level 33 and below) agree row by row. Beyond that the ring is still being
        // generated for dependencies: how far each chunk got depends on timing, so only its position and level count.
        let chunks = |zip: &BTreeMap<String, String>| {
            let rows = lines_of(zip, &format!("{dir}/chunks.csv"));
            sorted(
                rows.into_iter()
                    .map(
                        |row| match row.split(',').nth(2).and_then(|l| l.parse::<i32>().ok()) {
                            Some(level) if level > 33 => {
                                row.split(',').take(3).collect::<Vec<_>>().join(",")
                            }
                            // the one-tick tickets of the chunk loads that are in flight are there or not
                            _ => row
                                .split(',')
                                .map(|c| match c.starts_with("Ticket[minecraft:unknown ") {
                                    true => "no_ticket",
                                    false => c,
                                })
                                .collect::<Vec<_>>()
                                .join(","),
                        },
                    )
                    .collect(),
            )
        };
        diffs.eq(
            &format!("{dir}/chunks.csv"),
            chunks(&vanilla),
            chunks(&ours),
        );
        let stats = |zip: &BTreeMap<String, String>| {
            lines_of(zip, &format!("{dir}/stats.txt"))
                .iter()
                .map(|l| mask_numbers(l))
                .collect::<Vec<_>>()
        };
        diffs.eq(
            &format!("{dir}/stats.txt"),
            sorted(stats(&vanilla)),
            sorted(stats(&ours)),
        );
        // the random quip, the time, the stack frames and the host rows of the system details (the JVM's and the
        // machine's, after the two Minecraft version rows) are not compared
        let crash = |zip: &BTreeMap<String, String>| {
            let lines = lines_of(zip, &format!("{dir}/example_crash.txt"));
            let end = lines
                .iter()
                .position(|l| l == "-- System Details --")
                .map_or(lines.len(), |i| i + 4);
            lines
                .into_iter()
                .take(end)
                .enumerate()
                .filter(|(i, l)| *i != 1 && !l.trim_start().starts_with("at "))
                .map(|(_, l)| {
                    if ["Time:", "\tLevel time:", "\tServer weather:"]
                        .iter()
                        .any(|p| l.starts_with(p))
                    {
                        mask_numbers(&l)
                    } else {
                        l
                    }
                })
                .collect::<Vec<_>>()
        };
        diffs.eq(
            &format!("{dir}/example_crash.txt"),
            crash(&vanilla),
            crash(&ours),
        );
    }

    for name in [
        "server/gamerules.txt",
        "server/modules.txt",
        "server/server.properties.txt",
    ] {
        diffs.eq(
            name,
            sorted(lines_of(&vanilla, name)),
            sorted(lines_of(&ours, name)),
        );
    }
    for (side, zip) in [("vanilla", &vanilla), ("ours", &ours)] {
        let classpath = lines_of(zip, "server/classpath.txt");
        if classpath.len() != 1 || classpath[0].is_empty() {
            diffs.0.push(format!("{side} classpath.txt: {classpath:?}"));
        }
        let main_thread = lines_of(zip, "server/threads.txt")
            .iter()
            .any(|l| l.starts_with("\"Server thread\" "));
        if !main_thread {
            diffs
                .0
                .push(format!("{side} threads.txt has no Server thread"));
        }
    }

    let profile = |zip: &BTreeMap<String, String>| lines_of(zip, "server/profiling.txt");
    let (theirs, mine) = (profile(&vanilla), profile(&ours));
    // the random quip on the second line is not compared
    let head = |p: &[String]| -> Vec<String> {
        p.iter()
            .take(9)
            .enumerate()
            .map(|(i, l)| {
                if i == 1 {
                    String::new()
                } else {
                    mask_numbers(l)
                }
            })
            .collect()
    };
    diffs.eq("profiling.txt head", head(&theirs), head(&mine));
    let (theirs, mine) = (profile_names(&theirs), profile_names(&mine));
    if !mine.is_subset(&theirs) {
        diffs.0.push(format!(
            "profiling.txt: only ours: {:?}",
            mine.difference(&theirs).collect::<Vec<_>>()
        ));
    }

    assert!(diffs.0.is_empty(), "\n{}", diffs.0.join("\n"));
}
