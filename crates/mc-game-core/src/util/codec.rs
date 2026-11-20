//! The result types of reading saved data: a message, or a message with the partial value the codec still decoded
//! (`DataResult.error(message, partialResult)`); the saved-data layer logs the message like vanilla.

pub type DataResult<T> = Result<T, String>;

/// `DataResult.error(message, partialResult)`: a failed decode that may still carry a usable partial value
/// (`resultOrPartial` consumers — the saved-data reader — keep it and log the message).
#[derive(Debug)]
pub struct Partial<T> {
    pub value: Option<T>,
    pub error: String,
}

impl<T> From<String> for Partial<T> {
    fn from(error: String) -> Self {
        Partial { value: None, error }
    }
}

/// A decode result whose error may carry a partial value.
pub type DecodeResult<T> = Result<T, Partial<T>>;

/// `DataResult.map` over both the success and the partial value.
pub fn map_decoded<T, U>(result: DecodeResult<T>, f: impl FnOnce(T) -> U) -> DecodeResult<U> {
    match result {
        Ok(v) => Ok(f(v)),
        Err(Partial { value, error }) => Err(Partial {
            value: value.map(f),
            error,
        }),
    }
}
