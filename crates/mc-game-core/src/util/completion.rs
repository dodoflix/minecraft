//! The slice of `CompletableFuture` the text filter and the chat message chain use: a value that is set once, and
//! continuations that run on the thread that completes it, or at once on the calling thread when it already is.

use parking_lot::Mutex;
use std::sync::Arc;

type Callback<T> = Box<dyn FnOnce(T) + Send>;

struct Inner<T> {
    value: Option<T>,
    callbacks: Vec<Callback<T>>,
}

/// A future value; clones share it.
pub struct Completion<T>(Arc<Mutex<Inner<T>>>);

impl<T> Clone for Completion<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// The write side of a pending [`Completion`].
pub struct Completer<T>(Completion<T>);

impl<T: Clone + Send + 'static> Completion<T> {
    /// `CompletableFuture.completedFuture(value)`.
    pub fn completed(value: T) -> Self {
        Self(Arc::new(Mutex::new(Inner {
            value: Some(value),
            callbacks: Vec::new(),
        })))
    }

    /// `new CompletableFuture<>()`.
    pub fn pending() -> (Completer<T>, Self) {
        let completion = Self(Arc::new(Mutex::new(Inner {
            value: None,
            callbacks: Vec::new(),
        })));
        (Completer(completion.clone()), completion)
    }

    pub fn is_done(&self) -> bool {
        self.0.lock().value.is_some()
    }

    pub fn get(&self) -> Option<T> {
        self.0.lock().value.clone()
    }

    /// `thenAccept(f)`: runs now when the value is there, else on the thread that completes it.
    pub fn then(&self, f: impl FnOnce(T) + Send + 'static) {
        let mut inner = self.0.lock();
        match inner.value.clone() {
            Some(value) => {
                drop(inner);
                f(value);
            }
            None => inner.callbacks.push(Box::new(f)),
        }
    }

    /// `Util.sequenceFailFast(futures)` without a failure case: the values in order, once all are in.
    pub fn sequence(futures: Vec<Completion<T>>) -> Completion<Vec<T>> {
        if futures.is_empty() {
            return Completion::completed(Vec::new());
        }
        let (done, result) = Completion::pending();
        let state = Arc::new(Mutex::new((
            vec![None; futures.len()],
            futures.len(),
            Some(done),
        )));
        for (index, future) in futures.into_iter().enumerate() {
            let state = state.clone();
            future.then(move |value| {
                let mut state = state.lock();
                state.0[index] = Some(value);
                state.1 -= 1;
                if state.1 == 0 {
                    let values = state.0.drain(..).flatten().collect();
                    if let Some(done) = state.2.take() {
                        drop(state);
                        done.complete(values);
                    }
                }
            });
        }
        result
    }
}

impl<T: Clone + Send + 'static> Completer<T> {
    /// `complete(value)`: the continuations run on this thread, in the order they were added.
    pub fn complete(self, value: T) {
        let callbacks = {
            let mut inner = (self.0).0.lock();
            if inner.value.is_some() {
                return;
            }
            inner.value = Some(value.clone());
            std::mem::take(&mut inner.callbacks)
        };
        for callback in callbacks {
            callback(value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn continuation_runs_inline_when_done_and_on_completion_otherwise() {
        let seen = Arc::new(AtomicUsize::new(0));
        let s = seen.clone();
        Completion::completed(2usize).then(move |v| {
            s.fetch_add(v, Ordering::SeqCst);
        });
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        let (completer, pending) = Completion::pending();
        let s = seen.clone();
        pending.then(move |v: usize| {
            s.fetch_add(v, Ordering::SeqCst);
        });
        assert!(!pending.is_done());
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        completer.complete(5);
        assert_eq!(seen.load(Ordering::SeqCst), 7);
        assert_eq!(pending.get(), Some(5));
    }

    #[test]
    fn sequence_keeps_the_order_whatever_completes_first() {
        let (first, a) = Completion::pending();
        let (second, b) = Completion::pending();
        let all = Completion::sequence(vec![a, b, Completion::completed(3)]);
        second.complete(2);
        assert!(!all.is_done());
        first.complete(1);
        assert_eq!(all.get(), Some(vec![1, 2, 3]));
        assert_eq!(Completion::<u8>::sequence(vec![]).get(), Some(vec![]));
    }
}
