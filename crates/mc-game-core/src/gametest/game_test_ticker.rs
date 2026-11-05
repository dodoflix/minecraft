//! `GameTestTicker`: the singleton that ticks the running game tests from the server tick. The server runs on one
//! game thread, so the singleton is thread local.

use super::game_test_info::GameTestInfo;
use super::game_test_runner::GameTestRunner;
use crate::server::MinecraftServer;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Running,
    Halting,
}

struct GameTestTicker {
    test_infos: Vec<Rc<GameTestInfo>>,
    runner: Option<Rc<GameTestRunner>>,
    state: State,
}

thread_local! {
    static SINGLETON: RefCell<GameTestTicker> = const { RefCell::new(GameTestTicker { test_infos: Vec::new(), runner: None, state: State::Idle }) };
}

/// `add(testInfo)`.
pub fn add(test_info: Rc<GameTestInfo>) {
    SINGLETON.with(|t| t.borrow_mut().test_infos.push(test_info));
}

/// `clear()`.
pub fn clear(server: &mut MinecraftServer) {
    let runner = SINGLETON.with(|t| {
        let mut ticker = t.borrow_mut();
        if ticker.state != State::Idle {
            ticker.state = State::Halting;
            None
        } else {
            ticker.test_infos.clear();
            ticker.runner.take()
        }
    });
    if let Some(runner) = runner {
        runner.stop(server);
    }
}

/// `setRunner(runner)`.
pub fn set_runner(runner: Rc<GameTestRunner>) {
    SINGLETON.with(|t| {
        let mut ticker = t.borrow_mut();
        if ticker.runner.is_some() {
            log::error!("The runner was already set in GameTestTicker");
        }
        ticker.runner = Some(runner);
    });
}

/// `tick()`.
pub fn tick(server: &mut MinecraftServer) {
    let Some((runner, infos)) = SINGLETON.with(|t| {
        let mut ticker = t.borrow_mut();
        let runner = ticker.runner.clone()?;
        ticker.state = State::Running;
        Some((runner, ticker.test_infos.clone()))
    }) else {
        return;
    };
    for info in &infos {
        info.tick(server, &runner);
    }
    let finishing_state = SINGLETON.with(|t| {
        let mut ticker = t.borrow_mut();
        ticker.test_infos.retain(|i| !i.is_done());
        let finishing_state = ticker.state;
        ticker.state = State::Idle;
        finishing_state
    });
    if finishing_state == State::Halting {
        clear(server);
    }
}
