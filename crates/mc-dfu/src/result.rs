//! `com.mojang.serialization.DataResult` (lifecycles are not modelled): a success, or an error message with an
//! optional partial result. Messages are lazy (`Supplier<String>` in Java) so that failing alternatives are cheap.

use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct Msg(Arc<dyn Fn() -> String + Send + Sync>);

impl Msg {
    pub fn new(f: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Msg(Arc::new(f))
    }
    pub fn lit(s: impl Into<String>) -> Self {
        let s: String = s.into();
        Msg(Arc::new(move || s.clone()))
    }
    pub fn get(&self) -> String {
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
    pub fn error(message: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Error {
            message: Msg::new(message),
            partial: None,
        }
    }
    /// Error with a constant message.
    pub fn error_str(message: impl Into<String>) -> Self {
        Error {
            message: Msg::lit(message),
            partial: None,
        }
    }
    pub fn error_partial(message: impl Fn() -> String + Send + Sync + 'static, partial: A) -> Self {
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
    /// The error message (`None` for a success).
    pub fn error_message(&self) -> Option<String> {
        match self {
            Success(_) => None,
            Error { message, .. } => Some(message.get()),
        }
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
            Error { message, .. } => panic!("{}", message.get()),
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
                    message: Msg::new(move || format!("{}; {}", message.get(), m2.get())),
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
                message: Msg::new(move || format!("{}; {}", m2.get(), m1.get())),
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

    pub fn map_error(self, f: impl Fn(String) -> String + Send + Sync + 'static) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error { message, partial } => Error {
                message: Msg::new(move || f(message.get())),
                partial,
            },
        }
    }

    /// `promotePartial`: an error with a partial result becomes a success (the callback receives the message).
    pub fn promote_partial(self, on_error: impl FnOnce(&str)) -> DataResult<A> {
        match self {
            Success(a) => Success(a),
            Error {
                message,
                partial: Some(p),
            } => {
                on_error(&message.get());
                Success(p)
            }
            e => e,
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
