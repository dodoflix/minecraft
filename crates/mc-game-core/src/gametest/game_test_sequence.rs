//! `GameTestSequence` and `GameTestEvent`.

use super::game_test_exception::GameTestException;
use super::game_test_helper::GameTestHelper;
use super::game_test_info::GameTestInfo;
use crate::server::MinecraftServer;
use crate::translatable;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type TestResult = Result<(), GameTestException>;

/// An event's body: the helper and the sequence's `lastTick` at the time it runs.
type EventBody = Rc<dyn Fn(&mut GameTestHelper<'_>, i32) -> TestResult>;

/// `GameTestEvent`.
struct GameTestEvent {
    expected_delay: Option<i64>,
    minimum_delay: Option<i64>,
    assertion: EventBody,
}

pub struct GameTestSequence {
    events: RefCell<Vec<GameTestEvent>>,
    last_tick: Cell<i32>,
}

/// `executeWithoutFail(assertion)`: a failed assertion fails the test instead of aborting the sequence step.
fn execute_without_fail(
    helper: &mut GameTestHelper<'_>,
    assertion: impl FnOnce(&mut GameTestHelper<'_>) -> TestResult,
) -> TestResult {
    match assertion(helper) {
        Err(e) if e.is_assert() => {
            helper.info().fail_with(e);
            Ok(())
        }
        other => other,
    }
}

impl GameTestSequence {
    pub fn new(last_tick: i32) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            last_tick: Cell::new(last_tick),
        }
    }

    fn push(&self, expected_delay: Option<i64>, minimum_delay: Option<i64>, assertion: EventBody) {
        self.events.borrow_mut().push(GameTestEvent {
            expected_delay,
            minimum_delay,
            assertion,
        });
    }

    /// `thenWaitUntil(assertion)`.
    pub fn then_wait_until(
        &self,
        assertion: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(None, None, Rc::new(move |h, _| assertion(h)));
        self
    }

    /// `thenWaitUntil(expectedDelay, assertion)`.
    pub fn then_wait_until_after(
        &self,
        expected_delay: i64,
        assertion: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(
            Some(expected_delay),
            None,
            Rc::new(move |h, _| assertion(h)),
        );
        self
    }

    /// `thenWaitAtLeast(minimumDelay, assertion)`.
    pub fn then_wait_at_least(
        &self,
        minimum_delay: i64,
        assertion: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(None, Some(minimum_delay), Rc::new(move |h, _| assertion(h)));
        self
    }

    /// `thenIdle(delta)`.
    pub fn then_idle(&self, delta: i32) -> &Self {
        self.then_execute_after(delta, |_| Ok(()))
    }

    /// `thenExecute(assertion)`.
    pub fn then_execute(
        &self,
        assertion: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(
            None,
            None,
            Rc::new(move |h, _| execute_without_fail(h, &assertion)),
        );
        self
    }

    /// `thenExecuteAfter(delta, after)`.
    pub fn then_execute_after(
        &self,
        delta: i32,
        after: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(
            None,
            None,
            Rc::new(move |h, last_tick| {
                if h.info().get_tick() < last_tick + delta {
                    Err(GameTestException::assert(
                        translatable!("test.error.sequence.not_completed"),
                        h.info().get_tick(),
                    ))
                } else {
                    execute_without_fail(h, &after)
                }
            }),
        );
        self
    }

    /// `thenExecuteFor(delta, check)`.
    pub fn then_execute_for(
        &self,
        delta: i32,
        check: impl Fn(&mut GameTestHelper<'_>) -> TestResult + 'static,
    ) -> &Self {
        self.push(
            None,
            None,
            Rc::new(move |h, last_tick| {
                if h.info().get_tick() < last_tick + delta {
                    execute_without_fail(h, &check)?;
                    Err(GameTestException::assert(
                        translatable!("test.error.sequence.not_completed"),
                        h.info().get_tick(),
                    ))
                } else {
                    Ok(())
                }
            }),
        );
        self
    }

    /// `thenSucceed()`.
    pub fn then_succeed(&self) {
        self.push(
            None,
            None,
            Rc::new(|h, _| {
                h.succeed();
                Ok(())
            }),
        );
    }

    /// `thenFail(supplier)`.
    pub fn then_fail(&self, e: impl Fn() -> GameTestException + 'static) {
        self.push(
            None,
            None,
            Rc::new(move |h, _| {
                h.info().fail_with(e());
                Ok(())
            }),
        );
    }

    /// `thenTrigger()`.
    pub fn then_trigger(&self) -> Condition {
        let condition = Condition::default();
        let trigger = condition.clone();
        self.push(
            None,
            None,
            Rc::new(move |h, _| trigger.trigger(h.info().get_tick())),
        );
        condition
    }

    /// `tickAndContinue(tick)`: an unfinished sequence is not a failure.
    pub fn tick_and_continue(
        &self,
        server: &mut MinecraftServer,
        parent: &Rc<GameTestInfo>,
        tick: i32,
    ) {
        if let Err(e) = self.tick(server, parent, tick)
            && !e.is_assert()
        {
            parent.fail_with(e);
        }
    }

    /// `tickAndFailIfNotComplete(tick)`.
    pub fn tick_and_fail_if_not_complete(
        &self,
        server: &mut MinecraftServer,
        parent: &Rc<GameTestInfo>,
        tick: i32,
    ) {
        if let Err(e) = self.tick(server, parent, tick) {
            parent.fail_with(e);
        }
    }

    fn tick(
        &self,
        server: &mut MinecraftServer,
        parent: &Rc<GameTestInfo>,
        tick: i32,
    ) -> TestResult {
        loop {
            let Some((assertion, expected_delay, minimum_delay)) = self
                .events
                .borrow()
                .first()
                .map(|e| (e.assertion.clone(), e.expected_delay, e.minimum_delay))
            else {
                return Ok(());
            };
            let last_tick = self.last_tick.get();
            parent.run_guarded(server, |helper| assertion(helper, last_tick))?;
            self.events.borrow_mut().remove(0);
            let delay = i64::from(tick - last_tick);
            let prev_tick = last_tick;
            self.last_tick.set(tick);
            if let Some(minimum) = minimum_delay
                && minimum > delay
            {
                parent.fail_with(GameTestException::assert(
                    translatable!(
                        "test.error.sequence.minimum_tick",
                        i64::from(prev_tick) + minimum
                    ),
                    tick,
                ));
                break;
            }
            if let Some(expected) = expected_delay
                && expected != delay
            {
                parent.fail_with(GameTestException::assert(
                    translatable!(
                        "test.error.sequence.invalid_tick",
                        i64::from(prev_tick) + expected
                    ),
                    tick,
                ));
                break;
            }
        }
        Ok(())
    }
}

/// `GameTestSequence.Condition`.
#[derive(Clone)]
pub struct Condition {
    trigger_time: Rc<Cell<i32>>,
}

const NOT_TRIGGERED: i32 = -1;

impl Default for Condition {
    fn default() -> Self {
        Self {
            trigger_time: Rc::new(Cell::new(NOT_TRIGGERED)),
        }
    }
}

impl Condition {
    /// `trigger(time)`.
    pub fn trigger(&self, time: i32) -> TestResult {
        if self.trigger_time.get() != NOT_TRIGGERED {
            Err(GameTestException::Unknown {
                reason: format!("Condition already triggered at {}", self.trigger_time.get()),
            })
        } else {
            self.trigger_time.set(time);
            Ok(())
        }
    }

    /// `assertTriggeredThisTick()`.
    pub fn assert_triggered_this_tick(&self, helper: &GameTestHelper<'_>) -> TestResult {
        let tick = helper.info().get_tick();
        if self.trigger_time.get() != tick {
            return Err(if self.trigger_time.get() == NOT_TRIGGERED {
                GameTestException::assert(
                    translatable!("test.error.sequence.condition_not_triggered"),
                    tick,
                )
            } else {
                GameTestException::assert(
                    translatable!(
                        "test.error.sequence.condition_already_triggered",
                        self.trigger_time.get()
                    ),
                    tick,
                )
            });
        }
        Ok(())
    }
}
