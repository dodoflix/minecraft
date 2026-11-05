//! `net.minecraft.commands.execution.Frame`.

use super::ExecutionContext;
use crate::commands::CommandResultCallback;

/// `Frame.FrameControl`: what discarding a frame does to the command queue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameControl {
    /// The outermost frame: clear the whole queue.
    ClearQueue,
    /// `ExecutionContext.frameControlForDepth`: drop queued entries at this depth or deeper.
    DiscardAtDepth(i32),
}

#[derive(Clone)]
pub struct Frame {
    pub depth: i32,
    pub return_value_consumer: CommandResultCallback,
    pub frame_control: FrameControl,
}

impl Frame {
    pub fn new(depth: i32, return_value_consumer: CommandResultCallback, frame_control: FrameControl) -> Frame {
        Frame { depth, return_value_consumer, frame_control }
    }

    pub fn depth(&self) -> i32 {
        self.depth
    }

    pub fn frame_control(&self) -> FrameControl {
        self.frame_control
    }

    pub fn return_success(&self, value: i32) {
        self.return_value_consumer.on_success(value);
    }

    pub fn return_failure(&self) {
        self.return_value_consumer.on_failure();
    }

    pub fn discard(&self, context: &mut ExecutionContext) {
        context.apply_frame_control(self.frame_control);
    }
}
