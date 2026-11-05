//! `net.minecraft.gametest.framework`: the game test framework behind `/test`.

pub mod block_based_test_instance;
pub mod builtin_test_functions;
pub mod failed_test_tracker;
pub mod game_test_batch;
pub mod game_test_exception;
pub mod game_test_helper;
pub mod game_test_info;
pub mod game_test_instance;
pub mod game_test_listener;
pub mod game_test_runner;
pub mod game_test_sequence;
pub mod game_test_ticker;
pub mod global_test_reporter;
pub mod multiple_test_tracker;
pub mod play_handlers;
pub mod player_view;
pub mod report_game_listener;
pub mod retry_options;
pub mod structure_grid_spawner;
pub mod structure_utils;
pub mod test_command;
pub mod test_environment_definition;
pub mod test_finder;

/// `SharedConstants.IS_RUNNING_IN_IDE`: `false` in the server jar (`/test export*` exist only in IDE runs).
pub const IS_RUNNING_IN_IDE: bool = false;
