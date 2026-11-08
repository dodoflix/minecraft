//! Golden tests (T0/T1): replays of vanilla recordings and hermetic checks; one module per topic.

extern crate mc_game_core as mc_game;
mod advancement_predicates;
mod attribute_golden;
mod ban_date;
mod beacon_golden;
mod border_golden;
mod brain_golden;
mod chat_golden;
mod chunk_block_entity_order;
mod chunk_generation_pipeline;
mod chunk_level_golden;
mod chunk_tickets;
mod chunk_tracking;
#[path = "../../../mc-game/tests/golden/command_common/mod.rs"]
mod command_common;
#[path = "../../../mc-game/tests/golden/common/mod.rs"]
mod common;
#[path = "../../../mc-game/tests/golden/decoration_common/mod.rs"]
mod decoration_common;
mod decoration_golden;
mod decoration_loop_golden;
mod dedicated_properties;
mod distance_golden;
mod effects_golden;
mod entity_base_golden;
mod entity_oracle_fixtures;
mod food_golden;
mod gamerule_golden;
mod gametest_golden;
mod gen_level_golden;
mod gen_level_semantics;
mod java_hash_set;
mod jsonrpc_management;
mod level_mutation;
#[path = "../../../mc-game/tests/golden/net_common/mod.rs"]
mod net_common;
mod network_handshake_status;
mod oracle_drivers_wellformed;
mod play_chunk_sender_golden;
mod player_roundtrip;
mod saved_data_golden;
mod scoreboard_build;
mod scoreboard_golden;
mod setblock_differential;
mod spawn_framework_golden;
mod stats_golden;
mod structure_settings_golden;
mod user_lists_vanilla;
