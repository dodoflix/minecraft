//! Registries, holders, dynamic (data-driven) registry loading and tags — `net.minecraft.core` /
//! `net.minecraft.resources` / `net.minecraft.tags`.
//!
//! * [`access`]: `Registry`, `RegistryAccess`, `LayeredRegistryAccess`.
//! * [`data_loader`]: `RegistryDataLoader` (load a stage of dynamic registries from the pack stack).
//! * [`tags`]: `TagLoader`; [`synchronization`]: `registry_data` / `update_tags` content.
//! * [`codec`], [`decoders`], [`typed`]: typed decoding (`DYNAMIC_REGISTRY_DECODERS` linkme hook) and the NBT codec model.

pub mod access;
pub mod codec;
pub mod data_loader;
pub mod decoders;
pub mod feature_flags;
pub mod keys;
pub mod synchronization;
pub mod tags;
pub mod typed;

use std::collections::HashMap;
use std::sync::Arc;

pub use access::{LayeredRegistryAccess, RegistrationInfo, Registry, RegistryAccess, RegistryEntry};
use mc_core::ident::Identifier;

use crate::server::packs::ResourceManager;

/// `RegistryLayer.STATIC_ACCESS`: every code-defined registry (`BuiltInRegistries`), without tags.
pub fn static_registries() -> RegistryAccess {
    RegistryAccess::from_registries(mc_data::registry::registries::ALL.iter().map(|r| Registry::from_static(r)))
}

/// `Registry.PendingTags`: the tags to bind to one registry once loading succeeded.
#[derive(Clone)]
pub struct PendingTags {
    pub registry: String,
    pub tags: HashMap<Identifier, Arc<[u32]>>,
}

/// `TagLoader.loadTagsForExistingRegistries(manager, layer)`: tags of the registries of `layer` (the static ones) from every
/// selected pack. Registries without any tag file produce nothing.
pub fn load_tags_for_existing_registries(manager: &dyn ResourceManager, layer: &RegistryAccess) -> Vec<PendingTags> {
    let mut pending = Vec::new();
    for registry in layer.registries() {
        let mut lookup = |id: &Identifier, _required: bool| match registry.id_of(id) {
            Some(i) => tags::ElementRef::Found(i),
            None => tags::ElementRef::Missing,
        };
        let mut loader = tags::TagLoader::new(&mut lookup, &keys::tags_dir_path(registry.key()));
        let builders = loader.load(manager);
        let built = loader.build(&builders);
        if !built.is_empty() {
            pending.push(PendingTags { registry: registry.key().to_owned(), tags: built.into_iter().map(|(k, v)| (k, Arc::from(v))).collect() });
        }
    }
    pending
}

/// `TagLoader.buildUpdatedLookups(registries, tags)`: `registries` with the pending tags applied.
pub fn build_updated_lookups(registries: &RegistryAccess, pending: &[PendingTags]) -> RegistryAccess {
    RegistryAccess::from_arcs(registries.registries().map(|r| match pending.iter().find(|p| p.registry == r.key()) {
        Some(p) => Arc::new((**r).clone().with_tags(p.tags.clone())),
        None => r.clone(),
    }))
}

use typed::{biome::Biome, chat_type::ChatType, damage_type::DamageType, dimension_type::DimensionType, level_stem::LevelStem};

impl RegistryAccess {
    /// Typed `dimension_type` entry (`registryAccess.lookupOrThrow(DIMENSION_TYPE).getValue(id)`).
    pub fn dimension_type(&self, id: &Identifier) -> Option<&DimensionType> {
        self.get::<DimensionType>(keys::DIMENSION_TYPE, id)
    }

    /// Typed `worldgen/biome` entry.
    pub fn biome(&self, id: &Identifier) -> Option<&Biome> {
        self.get::<Biome>(keys::BIOME, id)
    }

    pub fn damage_type(&self, id: &Identifier) -> Option<&DamageType> {
        self.get::<DamageType>(keys::DAMAGE_TYPE, id)
    }

    pub fn chat_type(&self, id: &Identifier) -> Option<&ChatType> {
        self.get::<ChatType>(keys::CHAT_TYPE, id)
    }

    /// Typed `dialog` entry.
    pub fn dialog(&self, id: &Identifier) -> Option<&mc_protocol::dialog::Dialog> {
        self.get::<mc_protocol::dialog::Dialog>(keys::DIALOG, id)
    }

    /// Installs the process-wide `mc_protocol::registry_context` (protocol ids and tags of the data-driven registries that
    /// packet/NBT/JSON codecs resolve by name) from this access. Fixed once per process, like the registries themselves
    /// (vanilla's `RegistryAccess` is frozen after startup); a context installed earlier is kept.
    pub fn install_protocol_context(&self) {
        let mut context = mc_protocol::registry_context::RegistryContext::new();
        for registry in self.registries().filter(|r| r.is_dynamic()) {
            context = context.with_registry(registry.key(), registry.names());
        }
        let access = self.clone();
        let context = context.with_tag_lookup(Box::new(move |registry, tag| access.tag(registry, tag).is_some()));
        let _ = mc_protocol::registry_context::set(context);
    }

    /// Typed `dimension` (level stem) entry.
    pub fn level_stem(&self, id: &Identifier) -> Option<&LevelStem> {
        self.get::<LevelStem>(keys::LEVEL_STEM, id)
    }
}
