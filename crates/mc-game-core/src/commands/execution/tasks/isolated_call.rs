//! `IsolatedCall`: runs `task_producer` in a fresh frame one level deeper whose result goes to `output`.

use crate::commands::execution::{EntryAction, ExecutionContext, ExecutionControl, Frame};
use crate::commands::CommandResultCallback;

pub fn isolated_call(
    task_producer: impl FnOnce(&mut ExecutionControl<'_>) + 'static,
    output: CommandResultCallback,
) -> EntryAction {
    Box::new(move |context: &mut ExecutionContext, frame: &Frame| {
        let new_frame_depth = frame.depth() + 1;
        let new_frame = Frame::new(new_frame_depth, output, ExecutionContext::frame_control_for_depth(new_frame_depth));
        task_producer(&mut ExecutionControl::create(context, new_frame));
    })
}
