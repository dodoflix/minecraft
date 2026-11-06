//! Process-global registry context for wire/NBT/JSON codecs of *data-driven* (synced) registries.
//!
//! Vanilla threads a `RegistryAccess` through every `RegistryFriendlyByteBuf` / `RegistryOps`. Data-driven registries
//! (enchantment, damage_type, painting_variant, jukebox_song, trim_material, banner_pattern, instrument, dimension_type,
//! worldgen/biome, chat_type, cat_variant, ...) are fixed after server start (`/reload` does not change them), so a
//! single process-global name <-> id table is enough: the server sets it once after loading its registries, the test bot
//! after receiving the registry data. Built-in (static) registries do not need it: their tables come from `mc_data`.
//!
//! If nothing was set, the first use installs the table of the *vanilla* data pack embedded in `mc_data`.

use mc_core::ident::Identifier;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Name <-> protocol-id table of one registry (entries in id order).
#[derive(Debug, Clone, Default)]
pub struct RegistryTable {
    names: Vec<Identifier>,
    ids: HashMap<Identifier, i32>,
}

impl RegistryTable {
    pub fn new(names: Vec<Identifier>) -> Self {
        let ids = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as i32))
            .collect();
        RegistryTable { names, ids }
    }
    pub fn id_of(&self, name: &Identifier) -> Option<i32> {
        self.ids.get(name).copied()
    }
    pub fn name_of(&self, id: i32) -> Option<&Identifier> {
        usize::try_from(id).ok().and_then(|i| self.names.get(i))
    }
    pub fn contains(&self, name: &Identifier) -> bool {
        self.ids.contains_key(name)
    }
    pub fn len(&self) -> usize {
        self.names.len()
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
    pub fn names(&self) -> &[Identifier] {
        &self.names
    }
}

/// Tag contents of a registry, for the codecs that must check a `#tag` exists (`HolderSetCodec`, "Missing tag").
pub type TagLookup = Box<dyn Fn(&str, &Identifier) -> bool + Send + Sync>;

pub struct RegistryContext {
    registries: HashMap<String, RegistryTable>,
    tag_exists: Option<TagLookup>,
}

impl RegistryContext {
    pub fn new() -> Self {
        RegistryContext {
            registries: HashMap::new(),
            tag_exists: None,
        }
    }
    /// Add a data-driven registry (`key` like `minecraft:enchantment`) with its entries in protocol-id order.
    pub fn with_registry(mut self, key: &str, names: Vec<Identifier>) -> Self {
        self.registries
            .insert(key.to_string(), RegistryTable::new(names));
        self
    }
    /// Install the predicate used to check that a tag exists (default: tags of the embedded vanilla data pack).
    pub fn with_tag_lookup(mut self, f: TagLookup) -> Self {
        self.tag_exists = Some(f);
        self
    }
    pub fn registry(&self, key: &str) -> Option<&RegistryTable> {
        self.registries.get(key)
    }
    pub fn tag_exists(&self, registry: &str, tag: &Identifier) -> bool {
        match &self.tag_exists {
            Some(f) => f(registry, tag),
            None => vanilla_tag_exists(registry, tag),
        }
    }

    /// The vanilla data pack's data-driven registries, entries ordered by identifier.
    ///
    /// \[INFERENCE\] This matches vanilla's registry ids for an unmodified data pack (the loader lists resources in
    /// sorted order); the golden tests compare it to the real server's tables.
    pub fn vanilla() -> Self {
        let mut ctx = RegistryContext::new();
        for dir in crate::registry_context::DATA_DRIVEN_REGISTRIES {
            let (key, path) = (format!("minecraft:{dir}"), format!("data/minecraft/{dir}/"));
            let prefix: &'static str = Box::leak(path.into_boxed_str());
            let mut names: Vec<Identifier> = mc_data::datapack::list(prefix)
                .filter_map(|p| {
                    p.strip_prefix(prefix)?
                        .strip_suffix(".json")
                        .map(|n| format!("minecraft:{n}"))
                })
                .filter_map(|n| Identifier::parse(&n).ok())
                .collect();
            names.sort_by_key(|n| n.to_string());
            ctx.registries.insert(key, RegistryTable::new(names));
        }
        ctx
    }
}

impl Default for RegistryContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry keys (= data pack directory names) of every data-driven registry of the vanilla server (all of them, so default
/// item components etc. can resolve any reference); checked against the real server by `vanilla_registry_context_matches_server`.
pub const DATA_DRIVEN_REGISTRIES: &[&str] = &[
    "banner_pattern",
    "block_transformer",
    "cat_sound_variant",
    "cat_variant",
    "chat_type",
    "chicken_sound_variant",
    "chicken_variant",
    "cow_sound_variant",
    "cow_variant",
    "damage_type",
    "decorated_pot_pattern",
    "dialog",
    "dimension",
    "dimension_type",
    "enchantment",
    "enchantment_provider",
    "frog_variant",
    "instrument",
    "jukebox_song",
    "painting_variant",
    "pig_sound_variant",
    "pig_variant",
    "sulfur_cube_archetype",
    "test_environment",
    "test_instance",
    "timeline",
    "trade_set",
    "trial_spawner",
    "trim_material",
    "trim_pattern",
    "villager_trade",
    "wolf_sound_variant",
    "wolf_variant",
    "world_clock",
    "worldgen/biome",
    "worldgen/block_state_provider",
    "worldgen/carver",
    "worldgen/density_function",
    "worldgen/feature",
    "worldgen/flat_level_generator_preset",
    "worldgen/material_condition",
    "worldgen/material_rule",
    "worldgen/multi_noise_biome_source_parameter_list",
    "worldgen/noise",
    "worldgen/noise_settings",
    "worldgen/placed_feature",
    "worldgen/processor_list",
    "worldgen/structure",
    "worldgen/structure_set",
    "worldgen/template_pool",
    "worldgen/world_preset",
    "zombie_nautilus_variant",
];

fn vanilla_tag_exists(registry: &str, tag: &Identifier) -> bool {
    let dir = registry.strip_prefix("minecraft:").unwrap_or(registry);
    // built-in registries use their typed tag dirs; data-driven ones their data-pack dir
    let dir = mc_data::registry::by_name(registry)
        .map(|r| r.tag_dir())
        .unwrap_or(dir);
    mc_data::tags::names(dir, &tag.to_string()).is_some()
}

static CONTEXT: OnceLock<RegistryContext> = OnceLock::new();

/// Install the context; fails (returning it) if one is already set — it is fixed for the process lifetime.
pub fn set(ctx: RegistryContext) -> Result<(), RegistryContext> {
    CONTEXT.set(ctx)
}

/// The installed context, or the vanilla one if nothing was installed yet.
pub fn get() -> &'static RegistryContext {
    CONTEXT.get_or_init(RegistryContext::vanilla)
}

/// Protocol id of `name` in registry `registry` (`minecraft:enchantment`, `minecraft:item`, ...): built-in registries
/// from `mc_data`, data-driven ones from the context.
pub fn registry_id(registry: &str, name: &Identifier) -> Option<i32> {
    if let Some(r) = mc_data::registry::by_name(registry) {
        return r.id_of(&name.to_string()).map(|i| i as i32);
    }
    get().registry(registry)?.id_of(name)
}

/// Name of entry `id` in registry `registry`.
pub fn registry_name(registry: &str, id: i32) -> Option<Identifier> {
    if let Some(r) = mc_data::registry::by_name(registry) {
        return u32::try_from(id)
            .ok()
            .and_then(|i| r.name_of(i))
            .and_then(|n| Identifier::parse(n).ok());
    }
    get().registry(registry)?.name_of(id).cloned()
}

/// Whether `name` is an entry of `registry` (a [scoped lookup](with_scoped_lookup) takes precedence).
pub fn registry_contains(registry: &str, name: &Identifier) -> bool {
    if let Some(found) = scoped(|l| l.contains(registry, name)) {
        return found;
    }
    registry_id(registry, name).is_some()
}

/// Whether the tag `tag` exists in `registry` (a [scoped lookup](with_scoped_lookup) takes precedence).
pub fn tag_exists(registry: &str, tag: &Identifier) -> bool {
    scoped(|l| l.tag_exists(registry, tag)).unwrap_or_else(|| get().tag_exists(registry, tag))
}

/// A `RegistryOps.RegistryInfoLookup` for one thread and one codec run: while a registry is being loaded its entries
/// are not in the process-global context yet, but codecs of that very registry reference each other (and tags).
pub trait ScopedLookup {
    /// `Some(answer)` when this lookup decides about `registry`, `None` to fall through to the global context.
    fn contains(&self, registry: &str, name: &Identifier) -> Option<bool>;
    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Option<bool>;
}

thread_local! {
    static SCOPED: std::cell::Cell<Option<*const (dyn ScopedLookup + 'static)>> = const { std::cell::Cell::new(None) };
}

fn scoped<R>(f: impl FnOnce(&dyn ScopedLookup) -> Option<R>) -> Option<R> {
    // SAFETY: the pointer was stored by `with_scoped_lookup`, which keeps the referent alive and resets the slot
    // before returning (also on unwind); it is only read on the same thread, inside that call.
    SCOPED.with(|s| s.get()).and_then(|p| f(unsafe { &*p }))
}

/// Runs `f` with `lookup` answering [`registry_contains`] / [`tag_exists`] on this thread (nesting restores the outer one).
pub fn with_scoped_lookup<R>(lookup: &dyn ScopedLookup, f: impl FnOnce() -> R) -> R {
    struct Reset(Option<*const (dyn ScopedLookup + 'static)>);
    impl Drop for Reset {
        fn drop(&mut self) {
            SCOPED.with(|s| s.set(self.0));
        }
    }
    // SAFETY: only the lifetime is erased; `Reset` clears the slot before `lookup`'s borrow ends.
    let erased: *const (dyn ScopedLookup + 'static) =
        unsafe { std::mem::transmute::<&dyn ScopedLookup, &'static dyn ScopedLookup>(lookup) };
    let _reset = Reset(SCOPED.with(|s| s.replace(Some(erased))));
    f()
}
