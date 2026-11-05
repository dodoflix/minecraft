//! Golden tests (T0/T1): replays of vanilla recordings and hermetic checks; one module per topic.

extern crate mc_game_core as mc_game;
mod advancement_predicates;
#[path = "../../../mc-game/tests/golden/args_common/mod.rs"]
mod args_common;
mod args_component_golden;
mod args_item_golden;
mod args_nbt_tag_golden;
mod args_resource_or_id_dialog_golden;
mod args_resource_or_id_feature_golden;
mod args_resource_or_id_float_provider_golden;
mod args_resource_or_id_int_provider_golden;
mod args_resource_or_id_loot_modifier_golden;
mod args_resource_or_id_loot_predicate_golden;
mod args_slot_source_golden;
mod args_style_golden;
mod args_swizzle_golden;
mod attribute_golden;
mod ban_date;
mod beacon_golden;
mod border_golden;
mod brain_golden;
mod chat_golden;
mod chunk_generation;
mod chunk_generation_pipeline;
mod chunk_level_golden;
mod chunk_tickets;
mod chunk_tracking;
mod chunk_world;
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
mod entity_roundtrip;
mod farm_animals_scenarios;
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
mod saved_data_worldref;
mod scoreboard_golden;
mod setblock_differential;
mod spawn_framework_golden;
mod stats_golden;
mod structure_settings_golden;
mod user_lists_vanilla;
