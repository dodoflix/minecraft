//! `CommandResponseTracker`: accumulates per-element command results and renders the single/multiple feedback message.
//! Vanilla's `Messages`/`Dispatch*` records and the `…WithArg(s)` variants are closures capturing the extra arguments.
use mc_game_core::commands::CommandSourceStack;
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::host;
use mc_brigadier::CommandSyntaxException;
use mc_text::Component;

/// `CommandResponseTracker.ElementType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElementType {
    Any,
    NonZero,
}

pub struct CommandResponseTracker<E> {
    total_value: i32,
    only_element: Option<E>,
    element_count: i32,
    only_non_zero_element: Option<E>,
    non_zero_element_count: i32,
}

impl<E: Clone> Default for CommandResponseTracker<E> {
    fn default() -> Self {
        Self { total_value: 0, only_element: None, element_count: 0, only_non_zero_element: None, non_zero_element_count: 0 }
    }
}

impl<E: Clone> CommandResponseTracker<E> {
    pub fn create() -> Self {
        Self::default()
    }

    pub fn track(&mut self, element: &E, value: i32) {
        self.total_value += value;
        self.element_count += 1;
        self.only_element = if self.element_count == 1 { Some(element.clone()) } else { None };
        if value != 0 {
            self.non_zero_element_count += 1;
            self.only_non_zero_element = if self.non_zero_element_count == 1 { Some(element.clone()) } else { None };
        }
    }

    /// `track(element, boolean)`.
    pub fn track_bool(&mut self, element: &E, value: bool) {
        self.track(element, i32::from(value));
    }

    pub fn total_value(&self) -> i32 {
        self.total_value
    }

    fn first_element(&self, element_type: ElementType) -> Option<&E> {
        match element_type {
            ElementType::Any => self.only_element.as_ref(),
            ElementType::NonZero => self.only_non_zero_element.as_ref(),
        }
    }

    fn element_count(&self, element_type: ElementType) -> i32 {
        match element_type {
            ElementType::Any => self.element_count,
            ElementType::NonZero => self.non_zero_element_count,
        }
    }

    /// `dispatch(elementType, dispatch, args…)`: `on_single(onlyElement, total)` when exactly one element qualifies,
    /// else `on_multiple(elementCount, total)`.
    pub fn dispatch<R>(
        &self,
        element_type: ElementType,
        on_single: impl FnOnce(&E, i32) -> R,
        on_multiple: impl FnOnce(i32, i32) -> R,
    ) -> R {
        match self.first_element(element_type) {
            Some(element) => on_single(element, self.total_value),
            None => on_multiple(self.element_count(element_type), self.total_value),
        }
    }

    /// `sendFeedback(source, broadcast, elementType, messages…)`: throws `on_zero` (when set) if no element qualifies.
    pub fn send_feedback(
        &self,
        source: &CommandSourceStack,
        broadcast: bool,
        element_type: ElementType,
        on_zero: Option<impl FnOnce() -> CommandSyntaxException>,
        on_single: impl FnOnce(&E, i32) -> Component,
        on_multiple: impl FnOnce(i32, i32) -> Component,
    ) -> Result<i32, CommandSyntaxException> {
        if let Some(on_zero) = on_zero {
            if self.element_count(element_type) == 0 {
                return Err(on_zero());
            }
        }
        source.send_success(|| self.dispatch(element_type, on_single, on_multiple), broadcast);
        Ok(self.total_value)
    }
}

/// `player.getDisplayName()` of a selected player (empty when it vanished meanwhile).
pub fn display_name(source: &CommandSourceStack, entity: &EntityRef) -> Component {
    source.with_server(|server| host::get_entity(server, entity).map_or_else(Component::empty, |e| host::entity_display_name(server, e)))
}

impl<E: Clone> CommandResponseTracker<E> {
    /// `track(element)`: value 1.
    pub fn track_one(&mut self, element: &E) {
        self.track(element, 1);
    }

    /// `sendFeedback(source, broadcast, messages)`: the default overload, `ElementType.NON_ZERO`.
    pub fn send_feedback_default(
        &self,
        source: &CommandSourceStack,
        broadcast: bool,
        on_zero: Option<&dyn Fn() -> CommandSyntaxException>,
        on_single: impl FnOnce(&E, i32) -> Component,
        on_multiple: impl FnOnce(i32, i32) -> Component,
    ) -> Result<i32, CommandSyntaxException> {
        self.send_feedback(source, broadcast, ElementType::NonZero, on_zero.map(|f| move || f()), on_single, on_multiple)
    }
}
