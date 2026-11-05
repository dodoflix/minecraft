//! `FailedTestTracker`: the tests that failed in the last run (`/test runfailed`).

use super::game_test_instance::TestHolder;
use std::cell::RefCell;

thread_local! {
    // `LAST_FAILED_TESTS` is a `HashSet`: iteration order is unspecified; first-failure order here.
    static LAST_FAILED_TESTS: RefCell<Vec<TestHolder>> = const { RefCell::new(Vec::new()) };
}

/// `getLastFailedTests()`.
pub fn get_last_failed_tests() -> Vec<TestHolder> {
    LAST_FAILED_TESTS.with(|t| t.borrow().clone())
}

/// `rememberFailedTest(test)`.
pub fn remember_failed_test(test: TestHolder) {
    LAST_FAILED_TESTS.with(|t| {
        let mut tests = t.borrow_mut();
        if !tests.contains(&test) {
            tests.push(test);
        }
    });
}

/// `forgetFailedTests()`.
pub fn forget_failed_tests() {
    LAST_FAILED_TESTS.with(|t| t.borrow_mut().clear());
}
