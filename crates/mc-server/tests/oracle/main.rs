//! Oracle tests (T2): scripted clients against the real vanilla server and our `mc-server` binary; one module per topic.
//! Run with `MC_ACCEPT_EULA=1 cargo test -p mc-server --test oracle -- --ignored`.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "oracle scenarios report their progress and findings on the test's stdio"
)]

#[path = "../../../mc-game/tests/golden/net_common/mod.rs"]
mod net_support;

mod admin_packets_dual;
mod chunk_delivery_dual;
mod command_corpora;
mod common;
mod damaged_player_files;
mod demo_dual;
mod entity_dual;
mod entity_persistence;
mod file_fix_resume;
mod join_ours;
mod join_ours_state;
mod join_phase;
mod jsonrpc_dual;
mod kick_dual;
mod legacy_users;
mod light_packets_dual;
mod network_vanilla_diff;
mod ours_play;
mod packet_error;
mod persistence;
mod play_policy_dual;
mod play_scenario;
mod playerdata_diff;
mod portal_dual;
mod query_dual;
mod rcon_dual;
mod record_setblock;
mod server_policy_dual;
mod server_properties_dual;
mod services_keys_ours;
mod startup_gates;
mod vanilla_play;
mod world_commands_dual;
mod world_save_compare;
mod world_settings;
mod world_start;
mod world_upgrade;
