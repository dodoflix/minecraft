//! `DataAccessor`.
//!
//! Vanilla's accessors hold the live object; here they hold a handle (entity ref / dimension + position / storage id)
//! and reach the server through the command source stack, so every method takes the stack.
use crate::commands::CommandSourceStack;
use crate::commands::arguments::NbtPath;
use mc_brigadier::CommandSyntaxException;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;

pub trait DataAccessor {
    fn set_data(&self, source: &CommandSourceStack, tag: CompoundTag) -> Result<(), CommandSyntaxException>;

    fn get_data(&self, source: &CommandSourceStack) -> Result<CompoundTag, CommandSyntaxException>;

    fn get_modified_success(&self, source: &CommandSourceStack) -> Component;

    fn get_print_success(&self, source: &CommandSourceStack, data: &Tag) -> Component;

    fn get_print_success_for_path(&self, source: &CommandSourceStack, path: &NbtPath, scale: f64, value: i32) -> Component;
}

/// `String.format(Locale.ROOT, "%.2f", scale)`.
pub(super) fn format_scale(scale: f64) -> String {
    crate::util::java_format::format_fixed(scale, 2)
}

/// `NbtUtils.toPrettyComponent(tag)`.
pub(super) fn to_pretty_component(tag: &Tag) -> Component {
    mc_text::nbt_text::TextComponentTagVisitor::new("").visit(tag)
}
