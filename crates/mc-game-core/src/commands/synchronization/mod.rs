//! `net.minecraft.commands.synchronization`: what the client is told about the command tree.
pub mod argument_type_infos;
pub mod suggestion_providers;

pub use argument_type_infos::{ARGUMENT_TYPE_INFOS, ArgumentTypeInfoEntry, ArgumentTypeInfos};
