//! `net.minecraft.util.FutureChain`: the tasks of one player's chat run on the game thread in the order they were
//! appended, each after its own preparation (a text filter result) is in.
//!
//! Vanilla runs a task through `thenAcceptAsync(.., server)`: [`MinecraftServer.execute`] runs it at once when the
//! caller is the game thread and no task is running, and queues it otherwise (a worker thread completing the filter,
//! or an earlier link still waiting). [`FutureChain::append`] reports the first case as [`Appended::Ready`]: the caller
//! runs the task itself, with the player it holds.

use crate::server::MinecraftServer;
use crate::server::shared::ServerShared;
use crate::util::completion::{Completer, Completion};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct FutureChain {
    server: Option<Arc<ServerShared>>,
    head: Mutex<Completion<()>>,
    closed: Arc<AtomicBool>,
}

/// What [`FutureChain::append`] decided.
pub enum Appended<T> {
    /// Nothing is ahead and the preparation is in: run the task now, on this thread.
    Ready(T),
    /// The task runs later on the game thread: give it to [`QueuedLink::run`].
    Queued(QueuedLink<T>),
}

/// A chain link that has to wait. It must be run: the links behind it wait for it.
pub struct QueuedLink<T> {
    previous: Completion<()>,
    preparation: Completion<T>,
    done: Completer<()>,
    closed: Arc<AtomicBool>,
    server: Option<Arc<ServerShared>>,
}

/// Completes the link when dropped, also when the task panics (`exceptionally(.. continuing to next one)`).
struct Finish(Option<Completer<()>>);

impl Drop for Finish {
    fn drop(&mut self) {
        if let Some(done) = self.0.take() {
            done.complete(());
        }
    }
}

impl FutureChain {
    /// A chain whose queued links run on `server`'s task queue (`None`: every preparation is complete, nothing queues).
    pub fn new(server: Option<Arc<ServerShared>>) -> Self {
        Self {
            server,
            head: Mutex::new(Completion::completed(())),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// `append(preparation, task)`.
    pub fn append<T: Clone + Send + 'static>(&self, preparation: Completion<T>) -> Appended<T> {
        let mut head = self.head.lock();
        if head.is_done()
            && let Some(value) = preparation.get()
        {
            return Appended::Ready(value);
        }
        let (done, next) = Completion::pending();
        let previous = std::mem::replace(&mut *head, next);
        Appended::Queued(QueuedLink {
            previous,
            preparation,
            done,
            closed: self.closed.clone(),
            server: self.server.clone(),
        })
    }

    /// The game thread's task queue the queued links run on.
    pub fn server(&self) -> Option<Arc<ServerShared>> {
        self.server.clone()
    }

    /// `close()`: queued links no longer run their task.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl<T: Clone + Send + 'static> QueuedLink<T> {
    pub fn run(self, task: impl FnOnce(&mut MinecraftServer, T) + Send + 'static) {
        let Self {
            previous,
            preparation,
            done,
            closed,
            server,
        } = self;
        let finish = Finish(Some(done));
        previous.then(move |()| {
            preparation.then(move |value| {
                let Some(server) = server else { return };
                server.execute(move |server| {
                    let _finish = finish;
                    if !closed.load(Ordering::SeqCst) {
                        task(server, value);
                    }
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_only_when_nothing_is_ahead() {
        let chain = FutureChain::new(None);
        assert!(matches!(
            chain.append(Completion::completed(1)),
            Appended::Ready(1)
        ));
        let (_completer, pending) = Completion::<i32>::pending();
        assert!(matches!(chain.append(pending), Appended::Queued(_)));
        // the queued link has not run: everything behind it queues too
        assert!(matches!(
            chain.append(Completion::completed(2)),
            Appended::Queued(_)
        ));
    }
}
