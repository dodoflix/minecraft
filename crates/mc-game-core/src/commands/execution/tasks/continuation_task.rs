//! `ContinuationTask`: queues one follow-up entry per argument, re-queueing itself in between so a long fan-out does
//! not flood the queue at once.

use crate::commands::execution::{CommandQueueEntry, ExecutionContext, Frame};

type TaskFactory<P> = Box<dyn Fn(&Frame, &P) -> CommandQueueEntry>;

struct ContinuationTask<P> {
    task_factory: TaskFactory<P>,
    arguments: Vec<P>,
    index: usize,
}

impl<P: 'static> ContinuationTask<P> {
    fn into_entry(self, frame: Frame) -> CommandQueueEntry {
        CommandQueueEntry::new(
            frame,
            Box::new(move |context, frame| self.execute(context, frame)),
        )
    }

    fn execute(mut self, context: &mut ExecutionContext, frame: &Frame) {
        let entry = (self.task_factory)(frame, &self.arguments[self.index]);
        context.queue_next(entry);
        self.index += 1;
        if self.index < self.arguments.len() {
            context.queue_next(self.into_entry(frame.clone()));
        }
    }
}

/// `ContinuationTask.schedule`.
pub fn schedule<P: 'static>(
    context: &mut ExecutionContext,
    frame: &Frame,
    arguments: Vec<P>,
    task_factory: impl Fn(&Frame, &P) -> CommandQueueEntry + 'static,
) {
    match arguments.len() {
        0 => {}
        1 | 2 => {
            for argument in &arguments {
                context.queue_next(task_factory(frame, argument));
            }
        }
        _ => context.queue_next(
            ContinuationTask {
                task_factory: Box::new(task_factory),
                arguments,
                index: 0,
            }
            .into_entry(frame.clone()),
        ),
    }
}
