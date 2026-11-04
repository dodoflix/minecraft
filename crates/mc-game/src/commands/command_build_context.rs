//! `net.minecraft.commands.CommandBuildContext`: what command registration and argument types may consult
//! (registries and the enabled feature flags).

use mc_core::ident::Identifier;
use mc_world::level_storage::FeatureFlagSet;
use std::sync::Arc;

/// The read side of one registry that arguments use (`HolderLookup.RegistryLookup`): membership and listing of
/// elements and tags.
pub trait RegistryView: Send + Sync {
    fn contains(&self, id: &Identifier) -> bool;
    /// Element ids in registry order.
    fn element_ids(&self) -> Vec<Identifier>;
    /// Tag ids (`listTagIds`).
    fn tag_ids(&self) -> Vec<Identifier>;
    fn has_tag(&self, tag: &Identifier) -> bool;
}

/// `HolderLookup.Provider`: registries by key (`minecraft:item`, `minecraft:worldgen/biome`, ...).
pub trait HolderLookupProvider: Send + Sync {
    fn lookup(&self, registry_key: &str) -> Option<Arc<dyn RegistryView>>;
    fn list_registry_keys(&self) -> Vec<String>;
}

#[derive(Clone)]
pub struct CommandBuildContext {
    enabled_features: FeatureFlagSet,
    registries: Arc<dyn HolderLookupProvider>,
}

impl CommandBuildContext {
    /// `CommandBuildContext.simple(access, enabledFeatures)`.
    pub fn simple(registries: Arc<dyn HolderLookupProvider>, enabled_features: FeatureFlagSet) -> Self {
        Self { enabled_features, registries }
    }

    pub fn enabled_features(&self) -> &FeatureFlagSet {
        &self.enabled_features
    }

    pub fn lookup(&self, registry_key: &str) -> Option<Arc<dyn RegistryView>> {
        self.registries.lookup(registry_key)
    }

    pub fn list_registry_keys(&self) -> Vec<String> {
        self.registries.list_registry_keys()
    }
}
