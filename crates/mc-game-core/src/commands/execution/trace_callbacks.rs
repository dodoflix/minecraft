//! `net.minecraft.commands.execution.TraceCallbacks`.

use mc_core::ident::Identifier;

pub trait TraceCallbacks {
    fn on_command(&mut self, depth: i32, command: &str);
    fn on_return(&mut self, depth: i32, command: &str, result: i32);
    fn on_error(&mut self, message: &str);
    fn on_call(&mut self, depth: i32, function: &Identifier, size: usize);
    fn close(&mut self);
}
