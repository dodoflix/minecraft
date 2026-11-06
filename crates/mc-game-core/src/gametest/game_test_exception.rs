//! `GameTestException` and its subclasses (`GameTestAssertException`, `GameTestAssertPosException`,
//! `GameTestTimeoutException`, `UnknownGameTestException`) plus `ExhaustedAttemptsException`.

use crate::translatable;
use mc_core::pos::BlockPos;
use mc_text::Component;

#[derive(Clone, Debug)]
pub enum GameTestException {
    /// `GameTestAssertException(message, tick)`.
    Assert { message: Box<Component>, tick: i32 },
    /// `GameTestAssertPosException(baseMessage, absolutePos, relativePos, tick)`.
    AssertPos {
        message: Box<Component>,
        absolute_pos: BlockPos,
        relative_pos: BlockPos,
        tick: i32,
    },
    /// `GameTestTimeoutException(message)`.
    Timeout { message: Box<Component> },
    /// `UnknownGameTestException(reason)`: the message of the wrapped throwable.
    Unknown { reason: String },
}

impl GameTestException {
    pub fn assert(message: Component, tick: i32) -> Self {
        Self::Assert {
            message: Box::new(message),
            tick,
        }
    }

    /// `getDescription()`.
    pub fn description(&self) -> Component {
        match self {
            Self::Assert { message, tick } => {
                translatable!("test.error.tick", (**message).clone(), *tick)
            }
            Self::AssertPos {
                message,
                absolute_pos,
                relative_pos,
                tick,
            } => translatable!(
                "test.error.position",
                (**message).clone(),
                absolute_pos.x(),
                absolute_pos.y(),
                absolute_pos.z(),
                relative_pos.x(),
                relative_pos.y(),
                relative_pos.z(),
                *tick
            ),
            Self::Timeout { message } => (**message).clone(),
            Self::Unknown { reason } => translatable!("test.error.unknown", reason.clone()),
        }
    }

    /// `getMessage()`: for the assert exceptions the plain text of the description; the timeout keeps its message;
    /// the unknown one the message of the wrapped throwable.
    pub fn get_message(&self) -> String {
        match self {
            Self::Assert { .. } | Self::AssertPos { .. } => self.description().get_string(),
            Self::Timeout { message } => message.get_string(),
            Self::Unknown { reason } => reason.clone(),
        }
    }

    /// `instanceof GameTestAssertException`.
    pub fn is_assert(&self) -> bool {
        matches!(self, Self::Assert { .. } | Self::AssertPos { .. })
    }

    /// `GameTestAssertPosException.getMessageToShowAtBlock()` / `getAbsolutePos()`.
    pub fn pos_info(&self) -> Option<(BlockPos, Component)> {
        match self {
            Self::AssertPos {
                message,
                absolute_pos,
                ..
            } => Some((*absolute_pos, (**message).clone())),
            _ => None,
        }
    }
}

/// What `ReportGameListener.reportFailure` / `visualizeFailedTest` receive as `Throwable`: a game test exception or
/// the `ExhaustedAttemptsException` of a flaky test (whose cause is the test's last error).
#[derive(Clone, Debug)]
pub enum TestFailure {
    Exception(GameTestException),
    /// `new ExhaustedAttemptsException(attempts, successes, testInfo)`: message + cause.
    Exhausted {
        message: String,
        cause: Option<GameTestException>,
    },
}

impl TestFailure {
    /// `Throwable.getMessage()`.
    pub fn get_message(&self) -> String {
        match self {
            Self::Exception(e) => e.get_message(),
            Self::Exhausted { message, .. } => message.clone(),
        }
    }

    /// `Throwable.getCause()`.
    pub fn cause(&self) -> Option<&GameTestException> {
        match self {
            Self::Exception(_) => None,
            Self::Exhausted { cause, .. } => cause.as_ref(),
        }
    }

    /// `Util.describeError(error)`: the message of the root cause.
    pub fn describe_error(&self) -> String {
        match self.cause() {
            Some(cause) => cause.get_message(),
            None => self.get_message(),
        }
    }

    /// `rootCause instanceof GameTestAssertPosException`: its absolute position and the message to show at the block.
    pub fn root_cause_pos_info(&self) -> Option<(BlockPos, Component)> {
        match self {
            Self::Exception(e) => e.pos_info(),
            Self::Exhausted { cause, .. } => cause.as_ref().and_then(GameTestException::pos_info),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_and_messages() {
        let assert = GameTestException::assert(Component::literal("boom"), 7);
        assert_eq!(assert.get_message(), "boom on tick 7");
        let at = GameTestException::AssertPos {
            message: Box::new(Component::literal("bad")),
            absolute_pos: BlockPos::new(1, 2, 3),
            relative_pos: BlockPos::new(0, 1, 2),
            tick: 4,
        };
        assert_eq!(
            at.get_message(),
            "bad at 1, 2, 3 (relative: 0, 1, 2) on tick 4"
        );
        assert_eq!(
            GameTestException::Timeout {
                message: Box::new(Component::literal("late"))
            }
            .get_message(),
            "late"
        );
        assert_eq!(
            GameTestException::Unknown { reason: "x".into() }
                .description()
                .get_string(),
            "Unknown internal error: x"
        );
        let failure = TestFailure::Exhausted {
            message: "m".into(),
            cause: Some(at),
        };
        assert_eq!(
            failure.describe_error(),
            "bad at 1, 2, 3 (relative: 0, 1, 2) on tick 4"
        );
        assert!(failure.root_cause_pos_info().is_some());
    }
}
