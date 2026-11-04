//! `net.minecraft.commands.functions`.
use super::execution::UnboundEntryAction;
use mc_core::ident::Identifier;
use std::sync::Arc;

/// `InstantiatedFunction` (`PlainTextFunction` is the same value).
pub struct InstantiatedFunction {
    pub id: Identifier,
    pub entries: Vec<Arc<dyn UnboundEntryAction>>,
}
