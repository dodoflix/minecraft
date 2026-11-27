//! The dual-server command corpora (`crates/mc-game/tests/fixtures/commands_corpus/*.txt`, runner
//! `scripts/command-diff.sh`) as committed transcripts: each test runs one corpus through two bots against OUR server
//! only and compares the normalised output command by command with the transcript recorded from the real vanilla
//! server (`commands_corpus/golden/<corpus>.vanilla.txt`). A command whose output is a known difference carries a
//! `!known <reason>` line in the corpus.
//!
//! Re-record after a corpus change (vanilla needs Java and takes a few minutes per file; record twice and diff):
//! `scripts/command-diff.sh crates/mc-game/tests/fixtures/commands_corpus/<name>.txt --record
//! crates/mc-game/tests/fixtures/commands_corpus/golden`.
use std::path::Path;

fn corpus_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game/tests/fixtures/commands_corpus")
}

async fn check(name: &str) {
    let dir = corpus_dir();
    let ours = Path::new(env!("CARGO_BIN_EXE_mc-server"));
    mc_testbot::corpus::check_ours(&dir.join(format!("{name}.txt")), &dir.join("golden"), ours)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"));
}

macro_rules! corpora {
    ($($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        #[ignore = "oracle: starts the mc-server binary"]
        async fn $name() {
            check(stringify!($name)).await;
        }
    )*};
}

corpora!(
    advancements,
    block_entities,
    clone_matrix,
    command_forms,
    data_modify,
    entity_commands,
    execute,
    facing_angles,
    functions,
    gametest_bad,
    gametest_cmd,
    gametest_gui,
    gametest_multi,
    gametest_pack,
    gametest_run,
    item_fill_override,
    items,
    loot_matrix,
    operating,
    packet_order,
    place,
    recipes,
    scoreboard,
    sounds,
    state_bossbar,
    state_misc,
    state_title,
    tag_limit,
    world_border_tick_spawn,
    worldedit,
    worldedit_normal,
);
