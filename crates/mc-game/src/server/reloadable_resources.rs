//! `net.minecraft.server.ReloadableServerResources` + `ReloadableServerRegistries`: everything data-driven that `/reload`
//! replaces — the `RELOADABLE` registries (loot tables, predicates, item modifiers, advancements, recipes, ...), the
//! tags of every registry, and the [`ReloadListener`]s (function library, ... registered by their own modules).

use std::any::Any;
use std::sync::Arc;

use mc_world::level_storage::FeatureFlagSet;

use super::registry_layer::RegistryLayer;
use super::permissions::PermissionSet;
use crate::commands::command_build_context::AccessRegistries;
use crate::commands::{CommandBuildContext, CommandSelection, Commands};
use crate::registry::data_loader::{self, RegistryLoadFailure};
use crate::registry::keys::RELOADABLE_REGISTRIES;
use crate::registry::{LayeredRegistryAccess, PendingTags, RegistryAccess, build_updated_lookups, load_tags_for_existing_registries};
use crate::server::packs::{MultiPackResourceManager, PackRepository, PackType, ResourceManager};

/// What a [`ReloadListener`] sees while reloading (`PreparableReloadListener.reload` context).
pub struct ReloadContext<'a> {
    pub resource_manager: &'a dyn ResourceManager,
    /// All registries (static ones with the new tags, the new `RELOADABLE` layer included).
    pub registries: &'a RegistryAccess,
    pub enabled_features: FeatureFlagSet,
    /// The `Commands` built for this reload (`ReloadableServerResources.commands`): what functions compile against.
    pub commands: &'a Commands,
    /// `functionCompilationPermissions` (`function-permission-level`).
    pub function_compilation_permissions: &'a PermissionSet,
}

/// A reloadable piece of server state that is rebuilt from the resource manager on every (re)load
/// (`PreparableReloadListener` as used in `ReloadableServerResources.listeners()`).
pub trait ReloadListener: Any + Send + Sync {
    fn name(&self) -> &'static str;

    /// Rebuild from `ctx`; an `Err` fails the whole reload (the old resources stay in place).
    fn reload(&mut self, ctx: &ReloadContext) -> Result<(), String>;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Registration of a listener type, from the module that implements it:
///
/// ```ignore
/// #[linkme::distributed_slice(RELOAD_LISTENERS)]
/// static FUNCTION_LIBRARY: ReloadListenerRegistration =
///     ReloadListenerRegistration { name: "function_library", order: ORDER_FUNCTION_LIBRARY, create: || Box::new(ServerFunctionLibrary::new()) };
/// ```
pub struct ReloadListenerRegistration {
    pub name: &'static str,
    /// Position in `ReloadableServerResources.listeners()`; lower reloads first.
    pub order: i32,
    pub create: fn() -> Box<dyn ReloadListener>,
}

/// Position of vanilla's `ServerFunctionLibrary` (the only listener of `ReloadableServerResources.listeners()` in 26.3).
pub const ORDER_FUNCTION_LIBRARY: i32 = 0;
/// Position of the `RecipeManager` (`ReloadableServerResources.recipes`, finalized after the registries are loaded).
pub const ORDER_RECIPE_MANAGER: i32 = 10;

#[linkme::distributed_slice]
pub static RELOAD_LISTENERS: [ReloadListenerRegistration];

/// Failure of `ReloadableServerResources.loadResources`.
#[derive(Debug)]
pub enum ReloadError {
    Registries(RegistryLoadFailure),
    Listener { name: &'static str, message: String },
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReloadError::Registries(e) => e.fmt(f),
            ReloadError::Listener { name, message } => write!(f, "Failed to reload {name}: {message}"),
        }
    }
}

impl std::error::Error for ReloadError {}

impl From<RegistryLoadFailure> for ReloadError {
    fn from(e: RegistryLoadFailure) -> ReloadError {
        ReloadError::Registries(e)
    }
}

/// `ReloadableServerResources`.
pub struct ReloadableServerResources {
    commands: Arc<Commands>,
    layers: LayeredRegistryAccess<RegistryLayer>,
    postponed_tags: Vec<PendingTags>,
    enabled_features: FeatureFlagSet,
    listeners: Vec<Box<dyn ReloadListener>>,
}

impl ReloadableServerResources {
    /// `loadResources(resourceManager, contextLayers, updatedContextTags, enabledFeatures, ...)`: reload the `RELOADABLE`
    /// registries against `context_layers`, then run every registered listener in order.
    pub fn load_resources(
        manager: &dyn ResourceManager,
        context_layers: &LayeredRegistryAccess<RegistryLayer>,
        updated_context_tags: Vec<PendingTags>,
        enabled_features: FeatureFlagSet,
        function_compilation_permissions: &PermissionSet,
    ) -> Result<ReloadableServerResources, ReloadError> {
        // ReloadableServerRegistries.reload
        let context = build_updated_lookups(&context_layers.get_access_for_loading(RegistryLayer::Reloadable), &updated_context_tags);
        let reloadable = data_loader::load(manager, &context, RELOADABLE_REGISTRIES)?;
        let layers = context_layers.replace_from(RegistryLayer::Reloadable, vec![reloadable]);

        let full_with_tags = build_updated_lookups(layers.composite_access(), &updated_context_tags);
        crate::world::level::storage::loot::loading::validate_loot_registries(&full_with_tags).map_err(|message| ReloadError::Listener { name: "loot tables", message })?;
        let commands = Arc::new(Commands::new(
            CommandSelection::Dedicated,
            &CommandBuildContext::simple(Arc::new(AccessRegistries(full_with_tags.clone())), enabled_features),
        ));
        let mut registrations: Vec<&ReloadListenerRegistration> = RELOAD_LISTENERS.iter().collect();
        registrations.sort_by_key(|r| r.order);
        let mut listeners = Vec::with_capacity(registrations.len());
        let ctx = ReloadContext {
            resource_manager: manager,
            registries: &full_with_tags,
            enabled_features,
            commands: &commands,
            function_compilation_permissions,
        };
        for registration in registrations {
            let mut listener = (registration.create)();
            listener.reload(&ctx).map_err(|message| ReloadError::Listener { name: registration.name, message })?;
            listeners.push(listener);
        }
        Ok(ReloadableServerResources { commands, layers, postponed_tags: updated_context_tags, enabled_features, listeners })
    }

    /// `getCommands()`.
    pub fn commands(&self) -> &Arc<Commands> {
        &self.commands
    }

    /// `fullRegistries()`: every registry of every layer.
    pub fn full_registries(&self) -> &RegistryAccess {
        self.layers.composite_access()
    }

    /// The layers with the `RELOADABLE` registries of this load (static tags not applied until
    /// [`update_components_and_static_registry_tags`](Self::update_components_and_static_registry_tags)).
    pub fn registry_layers(&self) -> &LayeredRegistryAccess<RegistryLayer> {
        &self.layers
    }

    pub fn enabled_features(&self) -> FeatureFlagSet {
        self.enabled_features
    }

    /// `listeners()`.
    pub fn listeners(&self) -> &[Box<dyn ReloadListener>] {
        &self.listeners
    }

    /// The listener of type `T` (`getFunctionLibrary()`, `getRecipeManager()`, ... of vanilla).
    pub fn listener<T: ReloadListener>(&self) -> Option<&T> {
        self.listeners.iter().find_map(|l| l.as_any().downcast_ref::<T>())
    }

    pub fn listener_mut<T: ReloadListener>(&mut self) -> Option<&mut T> {
        self.listeners.iter_mut().find_map(|l| l.as_any_mut().downcast_mut::<T>())
    }

    /// `updateComponentsAndStaticRegistryTags()`: bind the tags loaded for the already existing registries (static ones, and
    /// on `/reload` the dynamic ones). Registries are immutable values here, so the layers are replaced.
    pub fn update_components_and_static_registry_tags(&mut self) {
        let pending = std::mem::take(&mut self.postponed_tags);
        if pending.is_empty() {
            return;
        }
        let values: Vec<RegistryAccess> = RegistryLayer::VALUES.iter().map(|l| build_updated_lookups(self.layers.get_layer(*l), &pending)).collect();
        let layers = LayeredRegistryAccess::new(RegistryLayer::VALUES.to_vec()).replace_from(RegistryLayer::Static, values);
        self.layers = layers;
    }
}

/// `MinecraftServer.ReloadableResources`: a resource manager with the resources loaded from it.
pub struct ReloadedResources {
    pub resource_manager: MultiPackResourceManager,
    pub managers: ReloadableServerResources,
}

/// `MinecraftServer.reloadResources(packsToEnable)` up to the point where the server swaps the result in: open the given packs
/// of `pack_repository`, reload the tags of every existing registry and the `RELOADABLE` registries / listeners.
///
/// On `Err` nothing was changed (the caller logs `Failed to execute reload` and keeps its old resources); on `Ok` the caller closes
/// the old resources, calls `pack_repository.set_selected(packs_to_enable)`, stores
/// `WorldDataConfiguration { data_packs: get_selected_packs(repo, true), enabled_features }` and replaces its registries by
/// `managers.registry_layers()`.
pub fn reload_resources(
    pack_repository: &PackRepository,
    packs_to_enable: &[String],
    registries: &LayeredRegistryAccess<RegistryLayer>,
    enabled_features: FeatureFlagSet,
    function_compilation_permissions: &PermissionSet,
) -> Result<ReloadedResources, ReloadError> {
    let packs: Vec<_> = packs_to_enable.iter().filter_map(|id| pack_repository.get_pack(id)).flat_map(|p| p.open()).collect();
    let resource_manager = MultiPackResourceManager::new(PackType::ServerData, packs);
    let postponed_tags = load_tags_for_existing_registries(&resource_manager, registries.composite_access());
    match ReloadableServerResources::load_resources(&resource_manager, registries, postponed_tags, enabled_features, function_compilation_permissions) {
        Ok(mut managers) => {
            managers.update_components_and_static_registry_tags();
            Ok(ReloadedResources { resource_manager, managers })
        }
        Err(e) => {
            resource_manager.close();
            Err(e)
        }
    }
}
