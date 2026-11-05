//! `net.minecraft.server.ServerAdvancementManager`: every advancement of the `advancement` registry and the tree
//! over them. Built after each (re)load of the reloadable registries.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_dfu::{DataResult, Dyn, JSON};
use serde_json::Value;

use super::advancement::{Advancement, AdvancementHolder};
use super::advancement_tree::AdvancementTree;
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::server::reloadable_resources::{ReloadContext, ReloadListener, ReloadListenerRegistration, RELOAD_LISTENERS};
use crate::world::level::storage::loot::holder::with_decode_context;

/// Position in `ReloadableServerResources` (after recipes, which vanilla builds before the advancements).
pub const ORDER_ADVANCEMENTS: i32 = 100;

#[derive(Default)]
pub struct ServerAdvancementManager {
    advancements: Vec<AdvancementHolder>,
    by_id: HashMap<Identifier, usize>,
    tree: Arc<AdvancementTree>,
}

impl ServerAdvancementManager {
    /// `new ServerAdvancementManager(registries)`.
    pub fn from_registries(registries: &crate::registry::RegistryAccess) -> ServerAdvancementManager {
        let mut advancements = Vec::new();
        let mut by_id = HashMap::new();
        if let Some(registry) = registries.registry(keys::ADVANCEMENT) {
            for entry in registry.entries() {
                let Some(value) = registry.get::<Arc<Advancement>>(&entry.id) else { continue };
                let problems = value.validate(registries);
                if !problems.is_empty() {
                    let report = problems.iter().map(|(path, d)| format!(" at {path}: {d}")).collect::<Vec<_>>().join("\n");
                    log::warn!("Found validation problems in advancement {}: \n{report}", entry.id);
                }
                by_id.insert(entry.id.clone(), advancements.len());
                advancements.push(AdvancementHolder::new(entry.id.clone(), value.clone()));
            }
        }
        let mut tree = AdvancementTree::new();
        tree.add_all(advancements.iter().cloned());
        tree.reposition_nodes();
        ServerAdvancementManager { advancements, by_id, tree: Arc::new(tree) }
    }

    /// `get(id)`.
    pub fn get(&self, id: &Identifier) -> Option<&AdvancementHolder> {
        self.by_id.get(id).map(|i| &self.advancements[*i])
    }

    /// `tree()`.
    pub fn tree(&self) -> &Arc<AdvancementTree> {
        &self.tree
    }

    /// `getAllAdvancements()` in registry order.
    pub fn get_all_advancements(&self) -> &[AdvancementHolder] {
        &self.advancements
    }
}

impl ReloadListener for ServerAdvancementManager {
    fn name(&self) -> &'static str {
        "advancements"
    }

    fn reload(&mut self, ctx: &ReloadContext) -> Result<(), String> {
        *self = ServerAdvancementManager::from_registries(ctx.registries);
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[linkme::distributed_slice(RELOAD_LISTENERS)]
static ADVANCEMENT_MANAGER: ReloadListenerRegistration =
    ReloadListenerRegistration { name: "advancements", order: ORDER_ADVANCEMENTS, create: || Box::new(ServerAdvancementManager::default()) };

fn decode_advancement(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let dyn_value = Dyn::from_json_str(&json.to_string()).map_err(|e| format!("Failed to parse JSON: {e}"))?;
    match with_decode_context(ctx, || Advancement::codec().parse(&JSON, &dyn_value)) {
        DataResult::Success(v) => Ok(DecodedEntry::new(Arc::new(v))),
        DataResult::Error { message, .. } => Err(message.get()),
    }
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static ADVANCEMENT_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::ADVANCEMENT, decode: decode_advancement };
