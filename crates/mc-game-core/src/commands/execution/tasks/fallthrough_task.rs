//! `FallthroughTask`: a `return` that matched no source fails the frame.

use crate::commands::execution::EntryAction;

pub fn instance() -> EntryAction {
    Box::new(|context, frame| {
        frame.return_failure();
        frame.discard(context);
    })
}
