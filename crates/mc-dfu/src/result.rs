//! `com.mojang.serialization.DataResult` (lifecycles are not modelled): a success, or an error message with an
//! optional partial result. Messages are lazy (`Supplier<String>` in Java) so that failing alternatives are cheap.

use mc_core::java_string::JavaString;
use std::fmt;
use std::sync::Arc;

/// An error message: a Java string, since it can quote the offending input.
#[derive(Clone)]
pub struct Msg(Arc<dyn Fn() -> JavaString + Send + Sync>);

impl Msg {
    pub fn new<R: Into<JavaString>>(f: impl Fn() -> R + Send + Sync + 'static) -> Self {
        Msg(Arc::new(move || f().into()))
    }
    pub fn lit(s: impl Into<JavaString>) -> Self {
        let s: JavaString = s.into();
        Msg(Arc::new(move || s.clone()))
    }
    /// The message as UTF-8 text, each lone surrogate shown as `?` like `String.getBytes(UTF_8)` does for a logged or
    /// thrown message (`get_java` keeps it).
    pub fn get(&self) -> String {
        self.get_java().into_utf8_string()
    }
    pub fn get_java(&self) -> JavaString {
        (self.0)()
    }
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get())
    }
}

#[derive(Clone, Debug)]
pub enum DataResult<A> {
    Success(A),
    Error { message: Msg, partial: Option<A> },
}

use DataResult::*;

impl<A> DataResult<A> {
    pub fn success(a: A) -> Self {
        Success(a)
    }
    /// `DataResult.error(() -> message)`.
    pub fn error<R: Into<JavaString>>(message: impl Fn() -> R + Send + Sync + 'static) -> Self {
        Error {
            message: Msg::new(message),
            partial: None,
        }
    }
    /// Error with a constant message.
    pub fn error_str(message: impl Into<JavaString>) -> Self {
        Error {
            message: Msg::lit(message),
            partial: None,
        }
    }
    pub fn error_partial<R: Into<JavaString>>(
        message: impl Fn() -> R + Send + Sync + 'static,
        partial: A,
    ) -> Self {
        Error {
            message: Msg::new(message),
            partial: Some(partial),
        }
    }
    pub fn error_msg(message: Msg, partial: Option<A>) -> Self {
        Error { message, partial }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Success(_))
    }
    pub fn is_error(&self) -> bool {
        !self.is_success()
    }
    pub fn has_result_or_partial(&self) -> bool {
        match self {
            Success(_) => true,
            Error { partial, .. } => partial.is_some(),
        }
    }
    /// `result()`: the value if this is a success.
    pub fn result(self) -> Option<A> {
        match self {
            Success(a) => Some(a),
            Error { .. } => None,
        }
    }
    pub fn result_ref(&self) -> Option<&A> {
        match self {
            Success(a) => Some(a),
            Error { .. } => None,
        }
    }
    /// `resultOrPartial()`.
    pub fn result_or_partial(self) -> Option<A> {
        match self {
            Success(a) => Some(a),
            Error { partial, .. } => partial,
        }
    }
    pub fn partial_ref(&self) -> Option<&A> {
        match self {
            Success(a) => Some(a),
            Error { partial, .. } => partial.as_ref(),
        }
    }
    /// The error message (`None` for a success), each lone surrogate shown as `?` (`error_msg_ref().get_java()` keeps it).
    pub fn error_message(&self) -> Option<String> {
        self.error_msg_ref().map(Msg::get)
    }
    pub fn error_msg_ref(&self) -> Option<&Msg> {
        match self {
            Success(_) => None,
            Error { message, .. } => Some(message),
        }
    }
    /// `getOrThrow`: the value or the error message.
    pub fn into_result(self) -> Result<A, String> {
        match self {
            Success(a) => Ok(a),
            Error { message, .. } => Err(message.get()),
        }
    }
    /// `getPartialOrThrow`.
    pub fn into_partial_result(self) -> Result<A, String> {
        match self {
            Success(a) => Ok(a),
            Error {
                partial: Some(a), ..
            } => Ok(a),
            Error { message, .. } => Err(message.get()),
        }
    }
    /// `getOrThrow()`: panics with the message (Java `IllegalStateException`).
    pub fn get_or_throw(self) -> A {
        match self {
            Success(a) => a,
            Error { message, .. } => panic!("java.lang.IllegalStateException: {}", message.get()),
        }
    }

    pub fn map<B>(self, f: impl FnOnce(A) -> B) -> DataResult<B> {
        match self {
            Success(a) => Success(f(a)),
            Error { message, partial } => Error {
                message,
                partial: partial.map(f),
            },
        }
    }

    /// `flatMap`: on an error with a partial result the function is applied to the partial value and the
    /// messages are joined (`DataResult.Error.flatMap`).
    pub fn flat_map<B>(self, f: impl FnOnce(A) -> DataResult<B>) -> DataResult<B> {
        match self {
            Success(a) => f(a),
            Error {
                message,
                partial: None,
            } => Error {
                message,
                partial: None,
            },
            Error {
                message,
                partial: Some(p),
            } => match f(p) {
                Success(b) => Error {
                    message,
                    partial: Some(b),
                },
                Error {
                    message: m2,
                    partial: p2,
                } => Error {
                    message: Msg::new(move || {
                        mc_core::jformat!("{}; {}", message.get_java(), m2.get_java())
                    }),
                    partial: p2,
                },
            },
        }
    }

    /// `apply2` / `apply2stable` (see `DataResult.Instance.ap2`): both successes combine; errors join their
    /// messages (second first, like the Java applicative) and combine the partial values when both exist.
    pub fn apply2<B, C>(self, second: DataResult<B>, f: impl FnOnce(A, B) -> C) -> DataResult<C> {
        match (self, second) {
            (Success(a), Success(b)) => Success(f(a, b)),
            (Error { message, partial }, Success(b)) => Error {
                message,
                partial: partial.map(|a| f(a, b)),
            },
            (Success(a), Error { message, partial }) => Error {
                message,
                partial: partial.map(|b| f(a, b)),
            },
            (
                Error {
                    message: m1,
                    partial: p1,
                },
                Error {
                    message: m2,
                    partial: p2,
                },
            ) => Error {
                message: Msg::new(move || {
                    mc_core::jformat!("{}; {}", m2.get_java(), m1.get_java())
                }),
                partial: match (p1, p2) {
                    (Some(a), Some(b)) => Some(f(a, b)),
                    _ => None,
                },
            },
        }
    }

    /// `setPartial(value)`: only affects errors.
    pub fn set_partial(self, partial: A) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error { message, .. } => Error {
                message,
                partial: Some(partial),
            },
        }
    }
    pub fn set_partial_opt(self, partial: Option<A>) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error { message, .. } => Error { message, partial },
        }
    }

    pub fn map_error(
        self,
        f: impl Fn(JavaString) -> JavaString + Send + Sync + 'static,
    ) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error { message, partial } => Error {
                message: Msg::new(move || f(message.get_java())),
                partial,
            },
        }
    }

    /// `promotePartial`: the callback receives the message of an error; one with a partial result becomes a success.
    pub fn promote_partial(self, on_error: impl FnOnce(&str)) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error { message, partial } => {
                on_error(&message.get());
                match partial {
                    Some(p) => Success(p),
                    None => Error {
                        message,
                        partial: None,
                    },
                }
            }
        }
    }

    pub fn as_ref(&self) -> DataResult<&A> {
        match self {
            Success(a) => Success(a),
            Error { message, partial } => Error {
                message: message.clone(),
                partial: partial.as_ref(),
            },
        }
    }

    /// Same error (message, no partial) with another value type.
    pub fn cast_error<B>(self) -> DataResult<B> {
        match self {
            Error { message, .. } => Error {
                message,
                partial: None,
            },
            Success(_) => panic!("cast_error on a success"),
        }
    }
}

impl<A> From<Result<A, String>> for DataResult<A> {
    fn from(r: Result<A, String>) -> Self {
        match r {
            Ok(a) => Success(a),
            Err(e) => DataResult::error_str(e),
        }
    }
}

impl<A: Clone> DataResult<A> {
    /// `DataResult.success(a.clone())` for use as a `flat_xmap` encode function over references.
    pub fn success_ref(a: &A) -> DataResult<A> {
        Success(a.clone())
    }
}
