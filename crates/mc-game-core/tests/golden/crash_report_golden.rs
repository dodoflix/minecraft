//! `CrashReport.getFriendlyReport` for every report type against the vanilla output (`fixtures/CrashReportGolden.java`):
//! the layout, the extra comments, the trailing blank lines of the exception, duplicate system rows. Left out of the
//! comparison on both sides, because a JVM and this server cannot agree on them: the witty comment, the time, the
//! `-- Head --` and `Stacktrace:` blocks, and every system row but the version, the operating system and the CPUs.

use mc_game::crash_report::{CrashReport, ReportType};
use std::collections::HashSet;

const KEPT_ROWS: [&str; 6] = [
    "Minecraft Version",
    "Minecraft Version ID",
    "Operating System",
    "CPUs",
    "Server Running",
    "Players",
];
const MASKED_ROWS: [&str; 2] = ["Operating System", "CPUs"];

/// The same normalisation as the harness.
fn normalise(report: &str) -> String {
    let kept: HashSet<&str> = KEPT_ROWS.into_iter().collect();
    let masked: HashSet<&str> = MASKED_ROWS.into_iter().collect();
    let (mut head, mut stack, mut system) = (false, false, false);
    let mut out: Vec<String> = Vec::new();
    for (i, line) in report.split('\n').enumerate() {
        let mut line = line.to_owned();
        if line == "-- Head --" {
            head = true;
            continue;
        }
        if head {
            head = !line.is_empty();
            continue;
        }
        if line == "Stacktrace:" {
            stack = true;
            continue;
        }
        if stack {
            if line.starts_with("\tat ") {
                continue;
            }
            stack = false;
        }
        if i == 1 {
            line = "// <comment>".to_owned();
        } else if line.starts_with("Time: ") {
            line = "Time: <time>".to_owned();
        } else if line == "-- System Details --" {
            system = true;
        } else if system && line.starts_with('\t') {
            let key = &line[1..line.find(':').unwrap()];
            if !kept.contains(key) {
                continue;
            }
            if masked.contains(key) {
                line = format!("\t{key}: <value>");
            }
        }
        out.push(line);
    }
    out.join("\n")
}

fn report(title: &str, categories: bool) -> CrashReport {
    let mut report = CrashReport::new(
        title,
        "java.lang.IllegalStateException: boom\n\tat com.example.Server.tick(Server.java:42)\n\tat java.lang.Thread.run(Thread.java:1583)",
    );
    if categories {
        report
            .add_category("Performance stats")
            .set_detail("Random tick rate", "3")
            .set_detail("Level stats", "minecraft:overworld: 12");
        report.add_category("Empty category");
        report
            .add_category("Detail kinds")
            .set_detail("Number", 5)
            .set_detail("Text", "two\nlines");
    }
    let system = report.system_report();
    system.set_detail("Server Running", "true");
    system.set_detail("Server Running", "false");
    system.set_detail("Players", "0 / 20; []");
    report
}

#[test]
fn friendly_reports_equal_vanilla() {
    let mut out = String::new();
    let mut print = |name: &str, text: String| {
        out.push_str(&format!("## {name}\n{}\n", normalise(&text)));
    };
    let crash = report("Exception in server tick loop", true);
    print(
        "crash with categories",
        crash.get_friendly_report(ReportType::CRASH),
    );
    print("crash details", crash.get_details());
    print(
        "crash without categories",
        report("Watching Server", false).get_friendly_report(ReportType::CRASH),
    );
    print(
        "profile with extra comments",
        report("Profiling", false)
            .get_friendly_report_with(ReportType::PROFILE, &["first extra", "second extra"]),
    );
    print(
        "test",
        report("Test failed", true).get_friendly_report(ReportType::TEST),
    );
    print(
        "network protocol error",
        report("Packet", true)
            .get_friendly_report_with(ReportType::NETWORK_PROTOCOL_ERROR, &["one extra"]),
    );
    print(
        "chunk io error",
        report("Chunk save failure", true).get_friendly_report(ReportType::CHUNK_IO_ERROR),
    );
    assert_eq!(out, include_str!("../fixtures/crash_report_golden.txt"));
}

#[test]
fn reports_have_the_head_and_stack_traces_of_vanilla() {
    let text = report("Exception in server tick loop", true).get_friendly_report(ReportType::CRASH);
    assert!(text.contains("-- Head --\nThread: "), "{text}");
    assert!(text.contains("\nStacktrace:\n\tat "), "{text}");
}
