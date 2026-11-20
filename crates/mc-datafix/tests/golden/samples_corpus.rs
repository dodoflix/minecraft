//! The corpus fixtures: real inputs from the worlds of old vanilla servers (and generated ones for the types no world
//! has files of), one fixture per `DataFixTypes` constant, replayed through our fixer against vanilla's answers (format
//! and skipping rules: `common/corpus_samples.rs`; written by `scripts/datafix-corpus.sh`).

use crate::corpus_samples::assert_replays;

macro_rules! corpus {
    ($($test:ident: $file:literal, $answers:literal;)*) => {
        const FIXTURES: &[&str] = &[$($file),*];
        $(
            #[test]
            fn $test() {
                assert_replays(
                    &mc_testkit::include_gz!(concat!("../fixtures/samples/", $file)),
                    $answers,
                );
            }
        )*
    };
}

corpus! {
    advancements_updates_equal_vanilla: "corpus_advancements.tsv.gz", 240;
    chunk_updates_equal_vanilla: "corpus_chunk.tsv.gz", 1659;
    debug_profile_updates_equal_vanilla: "corpus_debug_profile.tsv.gz", 304;
    entity_chunk_updates_equal_vanilla: "corpus_entity_chunk.tsv.gz", 723;
    hotbar_updates_equal_vanilla: "corpus_hotbar.tsv.gz", 251;
    level_updates_equal_vanilla: "corpus_level.tsv.gz", 211;
    level_summary_updates_equal_vanilla: "corpus_level_summary.tsv.gz", 211;
    options_updates_equal_vanilla: "corpus_options.tsv.gz", 318;
    player_updates_equal_vanilla: "corpus_player.tsv.gz", 313;
    poi_chunk_updates_equal_vanilla: "corpus_poi_chunk.tsv.gz", 249;
    saved_data_command_storage_updates_equal_vanilla: "corpus_saved_data_command_storage.tsv.gz", 315;
    saved_data_custom_boss_events_updates_equal_vanilla: "corpus_saved_data_custom_boss_events.tsv.gz", 3;
    saved_data_ender_dragon_fight_updates_equal_vanilla: "corpus_saved_data_ender_dragon_fight.tsv.gz", 3;
    saved_data_forced_chunks_updates_equal_vanilla: "corpus_saved_data_forced_chunks.tsv.gz", 55;
    saved_data_game_rules_updates_equal_vanilla: "corpus_saved_data_game_rules.tsv.gz", 3;
    saved_data_map_data_updates_equal_vanilla: "corpus_saved_data_map_data.tsv.gz", 29;
    saved_data_map_index_updates_equal_vanilla: "corpus_saved_data_map_index.tsv.gz", 30;
    saved_data_raids_updates_equal_vanilla: "corpus_saved_data_raids.tsv.gz", 48;
    saved_data_random_sequences_updates_equal_vanilla: "corpus_saved_data_random_sequences.tsv.gz", 30;
    saved_data_scheduled_events_updates_equal_vanilla: "corpus_saved_data_scheduled_events.tsv.gz", 3;
    saved_data_scoreboard_updates_equal_vanilla: "corpus_saved_data_scoreboard.tsv.gz", 90;
    saved_data_stopwatches_updates_equal_vanilla: "corpus_saved_data_stopwatches.tsv.gz", 5;
    saved_data_structure_feature_indices_updates_equal_vanilla: "corpus_saved_data_structure_feature_indices.tsv.gz", 24;
    saved_data_wandering_trader_updates_equal_vanilla: "corpus_saved_data_wandering_trader.tsv.gz", 313;
    saved_data_weather_updates_equal_vanilla: "corpus_saved_data_weather.tsv.gz", 3;
    saved_data_world_border_updates_equal_vanilla: "corpus_saved_data_world_border.tsv.gz", 5;
    saved_data_world_clocks_updates_equal_vanilla: "corpus_saved_data_world_clocks.tsv.gz", 3;
    saved_data_world_gen_settings_updates_equal_vanilla: "corpus_saved_data_world_gen_settings.tsv.gz", 6;
    stats_updates_equal_vanilla: "corpus_stats.tsv.gz", 276;
    structure_updates_equal_vanilla: "corpus_structure.tsv.gz", 310;
    world_gen_settings_updates_equal_vanilla: "corpus_world_gen_settings.tsv.gz", 68;
}

#[test]
fn every_corpus_fixture_is_replayed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/samples");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .expect("the samples folder")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("corpus_") && n.ends_with(".tsv.gz"))
        .collect();
    on_disk.sort();
    assert_eq!(on_disk, FIXTURES);
}
