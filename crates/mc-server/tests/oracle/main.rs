//! Oracle tests (T2): scripted clients against the real vanilla server and our `mc-server` binary; one module per topic.
//! Run with `MC_ACCEPT_EULA=1 cargo test -p mc-server --test oracle -- --ignored`.

mod common;
mod entity_dual;
mod join_ours;
mod join_ours_state;
mod join_phase;
mod net_support;
mod network_vanilla_diff;
mod ours_play;
mod persistence;
mod play_scenario;
mod playerdata_diff;
mod portal_dual;
mod record_setblock;
mod vanilla_play;
mod world_commands_dual;
mod world_settings;
