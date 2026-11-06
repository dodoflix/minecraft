//! `net.minecraft.commands.CacheableFunction`: a function id that is resolved against the function manager once.

use super::functions::CommandFunction;
use crate::server::server_function_manager::ServerFunctionManager;
use mc_core::ident::Identifier;
use std::sync::Arc;

#[derive(Clone)]
pub struct CacheableFunction {
    id: Identifier,
    resolved: bool,
    function: Option<Arc<dyn CommandFunction>>,
}

impl CacheableFunction {
    pub fn new(id: Identifier) -> CacheableFunction {
        CacheableFunction {
            id,
            resolved: false,
            function: None,
        }
    }

    /// `get(manager)`: the lookup result (also a miss) is kept for later calls.
    pub fn get(&mut self, manager: &ServerFunctionManager) -> Option<Arc<dyn CommandFunction>> {
        if !self.resolved {
            self.function = manager.get(&self.id);
            self.resolved = true;
        }
        self.function.clone()
    }

    pub fn get_id(&self) -> &Identifier {
        &self.id
    }
}

impl PartialEq for CacheableFunction {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for CacheableFunction {}

impl std::hash::Hash for CacheableFunction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl std::fmt::Debug for CacheableFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CacheableFunction").field(&self.id).finish()
    }
}
