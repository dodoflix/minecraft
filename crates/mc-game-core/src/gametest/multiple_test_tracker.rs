//! `MultipleTestTracker`.

use super::game_test_info::GameTestInfo;
use super::game_test_instance::TestHolder;
use super::game_test_listener::GameTestListener;
use super::game_test_runner::GameTestRunner;
use crate::server::MinecraftServer;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub struct MultipleTestTracker {
    tests: RefCell<Vec<Rc<GameTestInfo>>>,
    listeners: RefCell<Vec<Rc<dyn GameTestListener>>>,
}

impl MultipleTestTracker {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// `new MultipleTestTracker(tests)`.
    pub fn with_tests(tests: Vec<Rc<GameTestInfo>>) -> Rc<Self> {
        Rc::new(Self { tests: RefCell::new(tests), listeners: RefCell::new(Vec::new()) })
    }

    /// `addTestToTrack(testInfo)`.
    pub fn add_test_to_track(&self, test_info: Rc<GameTestInfo>) {
        self.tests.borrow_mut().push(test_info.clone());
        for listener in self.listeners.borrow().iter() {
            test_info.add_listener(listener.clone());
        }
    }

    /// `addListener(listener)`.
    pub fn add_listener(&self, listener: Rc<dyn GameTestListener>) {
        self.listeners.borrow_mut().push(listener.clone());
        for test_info in self.tests.borrow().iter() {
            test_info.add_listener(listener.clone());
        }
    }

    /// `addFailureListener(listener)`.
    pub fn add_failure_listener(&self, listener: impl Fn(&Rc<GameTestInfo>) + 'static) {
        self.add_listener(Rc::new(FailureListener(Box::new(listener))));
    }

    /// Drops every test and listener: the tracker and the listeners it installed reference each other through the tests
    /// (vanilla leaves that cycle to the garbage collector); called once the run reported its summary.
    pub fn release(&self) {
        self.tests.borrow_mut().clear();
        self.listeners.borrow_mut().clear();
    }

    pub fn get_failed_required_count(&self) -> i32 {
        self.tests.borrow().iter().filter(|t| t.has_failed() && t.is_required()).count() as i32
    }

    pub fn get_failed_optional_count(&self) -> i32 {
        self.tests.borrow().iter().filter(|t| t.has_failed() && t.is_optional()).count() as i32
    }

    pub fn get_done_count(&self) -> i32 {
        self.tests.borrow().iter().filter(|t| t.is_done()).count() as i32
    }

    pub fn has_failed_required(&self) -> bool {
        self.get_failed_required_count() > 0
    }

    pub fn has_failed_optional(&self) -> bool {
        self.get_failed_optional_count() > 0
    }

    pub fn get_failed_required(&self) -> Vec<Rc<GameTestInfo>> {
        self.tests.borrow().iter().filter(|t| t.has_failed() && t.is_required()).cloned().collect()
    }

    pub fn get_failed_optional(&self) -> Vec<Rc<GameTestInfo>> {
        self.tests.borrow().iter().filter(|t| t.has_failed() && t.is_optional()).cloned().collect()
    }

    pub fn get_total_count(&self) -> i32 {
        self.tests.borrow().len() as i32
    }

    pub fn is_done(&self) -> bool {
        self.get_done_count() == self.get_total_count()
    }

    /// `getProgressBar()` / `toString()`.
    pub fn get_progress_bar(&self) -> String {
        let mut buf = String::from("[");
        for test in self.tests.borrow().iter() {
            buf.push(if !test.has_started() {
                ' '
            } else if test.has_succeeded() {
                '+'
            } else if test.has_failed() {
                if test.is_required() { 'X' } else { 'x' }
            } else {
                '_'
            });
        }
        buf.push(']');
        buf
    }

    /// `remove(testInfo)`.
    pub fn remove(&self, test_info: &Rc<GameTestInfo>) {
        let mut tests = self.tests.borrow_mut();
        if let Some(index) = tests.iter().position(|t| Rc::ptr_eq(t, test_info)) {
            tests.remove(index);
        }
    }

    /// The tests that failed, as registry holders (`FailedTestTracker` input).
    pub fn failed_holders(&self) -> Vec<TestHolder> {
        self.tests.borrow().iter().filter(|t| t.has_failed()).map(|t| t.get_test_holder().clone()).collect()
    }
}

struct FailureListener(Box<dyn Fn(&Rc<GameTestInfo>)>);

impl GameTestListener for FailureListener {
    fn test_structure_loaded(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>) {}

    fn test_passed(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {}

    fn test_failed(&self, _server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {
        (self.0)(test_info);
    }

    fn test_added_for_rerun(
        &self,
        _server: &mut MinecraftServer,
        _listener: &Rc<dyn GameTestListener>,
        _original: &Rc<GameTestInfo>,
        _copy: &Rc<GameTestInfo>,
        _runner: &Rc<GameTestRunner>,
    ) {
    }
}
