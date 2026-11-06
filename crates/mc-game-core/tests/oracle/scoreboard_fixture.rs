//! Regenerates `crates/mc-game/tests/fixtures/scoreboard_in.snbt`, the file `ScoreboardGolden.java` re-reads.

use crate::scoreboard_build::build;

/// A generator, not a check: it does nothing unless `MC_UPDATE_FIXTURES=1` is set, so an `--ignored` oracle run never
/// rewrites tracked files. Regenerate with
/// `MC_UPDATE_FIXTURES=1 cargo test -p mc-game-core --test oracle regen_scoreboard_in -- --ignored`, then rerun
/// `scripts/vanilla-java.sh ScoreboardGolden.java` (see the header of that file) to refresh the `.golden` files.
#[test]
#[ignore = "oracle: rewrites scoreboard_in.snbt (only with MC_UPDATE_FIXTURES=1)"]
fn regen_scoreboard_in() {
    if !std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        return;
    }
    let ours = build().store_to_tag().unwrap();
    std::fs::write(
        format!(
            "{}/../mc-game/tests/fixtures/scoreboard_in.snbt",
            env!("CARGO_MANIFEST_DIR")
        ),
        format!("{ours}\n"),
    )
    .unwrap();
}
