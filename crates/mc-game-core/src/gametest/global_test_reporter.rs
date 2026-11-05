//! `GlobalTestReporter`, `TestReporter` and `LogTestReporter`. (`JUnitLikeTestReporter` only serves the headless
//! `GameTestServer` (`--report`) and is not part of the dedicated server.)

use super::game_test_info::GameTestInfo;
use std::rc::Rc;

/// `LogTestReporter.onTestFailed`.
pub fn on_test_failed(test_info: &Rc<GameTestInfo>) {
    let test_position = test_info.get_test_block_pos().map(|p| p.to_short_string()).unwrap_or_default();
    let error = test_info.get_error().map(|e| e.get_message()).unwrap_or_default();
    if test_info.is_required() {
        log::error!("{} failed at {}! {}", test_info.id(), test_position, error);
    } else {
        log::warn!("(optional) {} failed at {}. {}", test_info.id(), test_position, error);
    }
}

/// `LogTestReporter.onTestSuccess`: nothing.
pub fn on_test_success(_test_info: &Rc<GameTestInfo>) {}
