//! `ReportGameListener`: reports every finished test to the players, the test instance block and the global reporter,
//! and drives retries of flaky tests.

use super::game_test_exception::TestFailure;
use super::game_test_info::GameTestInfo;
use super::game_test_listener::GameTestListener;
use super::game_test_runner::GameTestRunner;
use super::global_test_reporter;
use crate::server::MinecraftServer;
use crate::world::block_entity::test_instance_block_entity;
use mc_text::{ChatFormatting, Component};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
pub struct ReportGameListener {
    attempts: Cell<i32>,
    successes: Cell<i32>,
}

impl ReportGameListener {
    /// `handleRetry(testInfo, runner, passed)`.
    fn handle_retry(
        &self,
        server: &mut MinecraftServer,
        test_info: &Rc<GameTestInfo>,
        runner: &Rc<GameTestRunner>,
        passed: bool,
    ) {
        let retry_options = test_info.retry_options();
        let (attempts, successes) = (self.attempts.get(), self.successes.get());
        let mut report_as = format!(
            "[Run: {attempts:>4}, Ok: {successes:>4}, Fail: {:>4}",
            attempts - successes
        );
        if !retry_options.unlimited_tries() {
            report_as.push_str(&format!(
                ", Left: {:>4}",
                retry_options.number_of_tries - attempts
            ));
        }
        report_as.push(']');
        let name_part = format!(
            "{} {}! {}ms",
            test_info.id(),
            if passed { "passed" } else { "failed" },
            test_info.get_run_time()
        );
        let text = format!("{report_as:<53}{name_part}");
        if passed {
            report_passed(server, test_info, &text);
        } else {
            say(server, ChatFormatting::Red, &text);
        }
        if retry_options.has_tries_left(attempts, successes) {
            runner.rerun_test(server, test_info);
        }
    }
}

impl GameTestListener for ReportGameListener {
    fn test_structure_loaded(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>) {
        self.attempts.set(self.attempts.get() + 1);
    }

    fn test_passed(
        &self,
        server: &mut MinecraftServer,
        test_info: &Rc<GameTestInfo>,
        runner: &Rc<GameTestRunner>,
    ) {
        self.successes.set(self.successes.get() + 1);
        if test_info.retry_options().has_retries() {
            self.handle_retry(server, test_info, runner, true);
        } else if !test_info.is_flaky() {
            report_passed(
                server,
                test_info,
                &format!(
                    "{} passed! ({}ms / {}gameticks)",
                    test_info.id(),
                    test_info.get_run_time(),
                    test_info.get_tick()
                ),
            );
        } else if self.successes.get() >= test_info.required_successes() {
            report_passed(
                server,
                test_info,
                &format!(
                    "{} passed {} times of {} attempts.",
                    test_info,
                    self.successes.get(),
                    self.attempts.get()
                ),
            );
        } else {
            say(
                server,
                ChatFormatting::Green,
                &format!(
                    "Flaky test {} succeeded, attempt: {} successes: {}",
                    test_info,
                    self.attempts.get(),
                    self.successes.get()
                ),
            );
            runner.rerun_test(server, test_info);
        }
    }

    fn test_failed(
        &self,
        server: &mut MinecraftServer,
        test_info: &Rc<GameTestInfo>,
        runner: &Rc<GameTestRunner>,
    ) {
        if !test_info.is_flaky() {
            let error = test_info.get_error().expect("a failed test has an error");
            report_failure(server, test_info, &TestFailure::Exception(error));
            if test_info.retry_options().has_retries() {
                self.handle_retry(server, test_info, runner, false);
            }
        } else {
            let test_function = test_info.get_test();
            let mut text = format!(
                "Flaky test {} failed, attempt: {}/{}",
                test_info,
                self.attempts.get(),
                test_function.max_attempts()
            );
            if test_function.required_successes() > 1 {
                text.push_str(&format!(
                    ", successes: {} ({} required)",
                    self.successes.get(),
                    test_function.required_successes()
                ));
            }
            say(server, ChatFormatting::Yellow, &text);
            if test_info.max_attempts() - self.attempts.get() + self.successes.get()
                >= test_info.required_successes()
            {
                runner.rerun_test(server, test_info);
            } else {
                // `new ExhaustedAttemptsException(attempts, successes, testInfo)`
                let message = format!(
                    "Not enough successes: {} out of {} attempts. Required successes: {}. max attempts: {}.",
                    self.successes.get(),
                    self.attempts.get(),
                    test_info.required_successes(),
                    test_info.max_attempts()
                );
                report_failure(
                    server,
                    test_info,
                    &TestFailure::Exhausted {
                        message,
                        cause: test_info.get_error(),
                    },
                );
            }
        }
    }

    fn test_added_for_rerun(
        &self,
        _server: &mut MinecraftServer,
        listener: &Rc<dyn GameTestListener>,
        _original: &Rc<GameTestInfo>,
        copy: &Rc<GameTestInfo>,
        _runner: &Rc<GameTestRunner>,
    ) {
        copy.add_listener(listener.clone());
    }
}

/// `reportPassed(testInfo, text)`.
pub fn report_passed(server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, text: &str) {
    if let Some(pos) = test_info.get_test_block_pos() {
        test_instance_block_entity::set_success(server, test_info.level(), pos);
    }
    visualize_passed_test(server, test_info, text);
}

fn visualize_passed_test(server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, text: &str) {
    say(server, ChatFormatting::Green, text);
    global_test_reporter::on_test_success(test_info);
}

/// `reportFailure(testInfo, error)`.
fn report_failure(server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, error: &TestFailure) {
    let description = match error {
        TestFailure::Exception(e) if e.is_assert() => e.description(),
        other => Component::literal(other.describe_error()),
    };
    if let Some(pos) = test_info.get_test_block_pos() {
        test_instance_block_entity::set_error_message(server, test_info.level(), pos, description);
    }
    visualize_failed_test(server, test_info, error);
}

/// `visualizeFailedTest(testInfo, error)`.
fn visualize_failed_test(
    server: &mut MinecraftServer,
    test_info: &Rc<GameTestInfo>,
    error: &TestFailure,
) {
    let cause = error
        .cause()
        .map(|c| format!(" cause: {}", c.get_message()))
        .unwrap_or_default();
    let error_message = format!("{}{}", error.get_message(), cause);
    let failure_message = format!(
        "{}{} failed! {}",
        if test_info.is_required() {
            ""
        } else {
            "(optional) "
        },
        test_info.id(),
        error_message
    );
    say(
        server,
        if test_info.is_required() {
            ChatFormatting::Red
        } else {
            ChatFormatting::Yellow
        },
        &failure_message,
    );
    if let Some((absolute_pos, message)) = error.root_cause_pos_info()
        && let Some(pos) = test_info.get_test_block_pos()
    {
        test_instance_block_entity::mark_error(
            server,
            test_info.level(),
            pos,
            absolute_pos,
            message,
        );
    }
    global_test_reporter::on_test_failed(test_info);
}

/// `say(level, format, text)`: `level.getServer().getPlayerList().getPlayers().forEach(player.sendSystemMessage(...))`.
fn say(server: &mut MinecraftServer, format: ChatFormatting, text: &str) {
    let message = Component::literal(text).with_format(format);
    for player in server.player_list.get_players() {
        player.send_system_message(&message, false);
    }
}
