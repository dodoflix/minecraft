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
    /// Members of a tag (`HolderSet.Named` contents) in tag order, `None` for an unknown tag.
    fn tag_elements(&self, tag: &Identifier) -> Option<Vec<Identifier>>;
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

/// The built-in (static) registries of `mc_data` with vanilla's tags: what a vanilla server without data packs has.
/// The server replaces it with its `RegistryAccess` once data packs are loaded.
pub struct StaticRegistries;

struct StaticRegistryView(&'static mc_data::registry::Registry);

impl RegistryView for StaticRegistryView {
    fn contains(&self, id: &Identifier) -> bool {
        self.0.id_of(&id.to_string()).is_some()
    }

    fn element_ids(&self) -> Vec<Identifier> {
        self.0.entries().iter().filter_map(|n| Identifier::try_parse(n)).collect()
    }

    fn tag_ids(&self) -> Vec<Identifier> {
        mc_data::tags::list(self.0.tag_dir()).iter().filter_map(|n| Identifier::try_parse(n)).collect()
    }

    fn has_tag(&self, tag: &Identifier) -> bool {
        mc_data::tags::names(self.0.tag_dir(), &tag.to_string()).is_some()
    }

    fn tag_elements(&self, tag: &Identifier) -> Option<Vec<Identifier>> {
        mc_data::tags::names(self.0.tag_dir(), &tag.to_string()).map(|v| v.iter().filter_map(|n| Identifier::try_parse(n)).collect())
    }
}

impl HolderLookupProvider for StaticRegistries {
    fn lookup(&self, registry_key: &str) -> Option<Arc<dyn RegistryView>> {
        mc_data::registry::by_name(registry_key).map(|r| Arc::new(StaticRegistryView(r)) as Arc<dyn RegistryView>)
    }

    fn list_registry_keys(&self) -> Vec<String> {
        mc_data::registry::registries::ALL.iter().map(|r| r.name().to_owned()).collect()
    }
}
