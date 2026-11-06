//! By-value front end of [`CommandResponseTracker`] for the scoreboard/team/tag/trigger commands: elements are tracked by
//! value, `on_zero` is an already created exception and `send_feedback` is the `NON_ZERO` form (vanilla's overloads).
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use mc_brigadier::CommandSyntaxException;
use mc_game_core::commands::CommandSourceStack;
use mc_text::Component;

pub struct ScoreTracker<E: Clone>(CommandResponseTracker<E>);

impl<E: Clone> ScoreTracker<E> {
    pub fn new() -> Self {
        Self(CommandResponseTracker::create())
    }

    /// `track(element)`.
    pub fn track(&mut self, element: E) {
        self.0.track(&element, 1);
    }

    /// `track(element, value)`.
    pub fn track_value(&mut self, element: E, value: i32) {
        self.0.track(&element, value);
    }

    /// `track(element, boolean)`.
    pub fn track_bool(&mut self, element: E, value: bool) {
        self.0.track_bool(&element, value);
    }

    pub fn send_feedback_with(
        &self,
        source: &CommandSourceStack,
        broadcast: bool,
        element_type: ElementType,
        on_zero: Option<CommandSyntaxException>,
        on_single: impl FnOnce(&E, i32) -> Component,
        on_multiple: impl FnOnce(i32, i32) -> Component,
    ) -> Result<i32, CommandSyntaxException> {
        self.0.send_feedback(
            source,
            broadcast,
            element_type,
            on_zero.map(|e| move || e),
            on_single,
            on_multiple,
        )
    }

    /// `sendFeedback(source, broadcast, messages…)`: the `NON_ZERO` form.
    pub fn send_feedback(
        &self,
        source: &CommandSourceStack,
        broadcast: bool,
        on_zero: Option<CommandSyntaxException>,
        on_single: impl FnOnce(&E, i32) -> Component,
        on_multiple: impl FnOnce(i32, i32) -> Component,
    ) -> Result<i32, CommandSyntaxException> {
        self.send_feedback_with(
            source,
            broadcast,
            ElementType::NonZero,
            on_zero,
            on_single,
            on_multiple,
        )
    }
}
