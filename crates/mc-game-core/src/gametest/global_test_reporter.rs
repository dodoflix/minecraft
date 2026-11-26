//! `GlobalTestReporter`, `TestReporter` and `LogTestReporter`. The delegate is replaced by the headless game test
//! server (`--report`, see `junit_like_test_reporter`) on the thread that parses the command line, before the server
//! thread that reports the tests exists, so the delegate is process wide.

use super::game_test_info::GameTestInfo;
use parking_lot::Mutex;
use std::rc::Rc;

/// `TestReporter`.
pub trait TestReporter: Send {
    fn on_test_failed(&mut self, test_info: &Rc<GameTestInfo>);
    fn on_test_success(&mut self, test_info: &Rc<GameTestInfo>);
    /// `finish()` (default: nothing).
    fn finish(&mut self) {}
}

/// `LogTestReporter`.
struct LogTestReporter;

impl TestReporter for LogTestReporter {
    /// `LogTestReporter.onTestFailed`.
    fn on_test_failed(&mut self, test_info: &Rc<GameTestInfo>) {
        let test_position = test_info
            .get_test_block_pos()
            .map(|p| p.to_short_string())
            .unwrap_or_default();
        let error = test_info
            .get_error()
            .map(|e| e.get_message())
            .unwrap_or_default();
        if test_info.is_required() {
            log::error!("{} failed at {}! {}", test_info.id(), test_position, error);
        } else {
            log::warn!(
                "(optional) {} failed at {}. {}",
                test_info.id(),
                test_position,
                error
            );
        }
    }

    /// `LogTestReporter.onTestSuccess`: nothing.
    fn on_test_success(&mut self, _test_info: &Rc<GameTestInfo>) {}
}

/// `DELEGATE`: `None` is the `LogTestReporter`.
static DELEGATE: Mutex<Option<Box<dyn TestReporter>>> = Mutex::new(None);

fn with_delegate(f: impl FnOnce(&mut dyn TestReporter)) {
    match DELEGATE.lock().as_deref_mut() {
        Some(reporter) => f(reporter),
        None => f(&mut LogTestReporter),
    }
}

/// `replaceWith(testReporter)`.
pub fn replace_with(reporter: Box<dyn TestReporter>) {
    *DELEGATE.lock() = Some(reporter);
}

pub fn on_test_failed(test_info: &Rc<GameTestInfo>) {
    with_delegate(|d| d.on_test_failed(test_info));
}

pub fn on_test_success(test_info: &Rc<GameTestInfo>) {
    with_delegate(|d| d.on_test_success(test_info));
}

/// `finish()`.
pub fn finish() {
    with_delegate(|d| d.finish());
}
