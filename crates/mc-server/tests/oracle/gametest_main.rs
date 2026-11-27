//! The headless game test server (`mc-gametest`, vanilla `net.minecraft.gametest.Main`): each test runs one command line
//! through OUR binary and compares exit code, normalised console log, `gametestworld` layout and XML report with the
//! recording of the real vanilla server in `tests/fixtures/gametest_main/<case>.vanilla.txt`.
//!
//! Re-record after a change of a case (vanilla needs Java; record twice and diff):
//! `scripts/gametest-diff.sh --record crates/mc-server/tests/fixtures/gametest_main <case>`.
use std::path::Path;
use std::process::Command;

fn check(case: &str) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("../../scripts/gametest-diff.sh");
    let output = Command::new(&script)
        .arg("--golden")
        .arg(manifest.join("tests/fixtures/gametest_main"))
        .arg("--no-build")
        .arg("--ours-bin")
        .arg(env!("CARGO_BIN_EXE_mc-gametest"))
        .arg(case)
        .output()
        .unwrap_or_else(|e| panic!("run {script:?}: {e}"));
    assert!(
        output.status.success(),
        "{case}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

macro_rules! cases {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        #[ignore = "oracle: starts the mc-gametest binary"]
        fn $name() {
            check(stringify!($name));
        }
    )*};
}

cases!(
    abbrev,
    ambiguous,
    cluster,
    cluster_missing,
    cluster_plain,
    double_dash,
    dup_packs,
    dup_report,
    dup_tests,
    dup_universe,
    dup_verify,
    help,
    lone_dash,
    missing,
    packs,
    pass,
    repeat3,
    repeat_bad,
    single_dash,
    verify,
    verify_alone,
    verify_bad,
);
