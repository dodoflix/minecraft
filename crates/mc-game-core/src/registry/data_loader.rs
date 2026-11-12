//! `net.minecraft.resources.RegistryDataLoader` (+ `RegistryLoadTask` / `ResourceManagerRegistryLoadTask`): load the
//! data-driven registries of one stage from the resource manager.
//!
//! Per registry: every `data/<ns>/<registry path>/**.json` of the pack stack (top-most file wins) is parsed and decoded;
//! the elements are registered sorted by *file* identifier (path, then namespace) — that order defines the protocol ids —
//! and the registry's tags are loaded and bound. After all registries of the stage are in, references that stayed unbound
//! (holders created for a not-yet-registered element) and registry validators fail the load exactly like `freeze()`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_worldgen::data::{
    DENSITY_FUNCTION_REGISTRY, NOISE_REGISTRY, NOISE_SETTINGS_REGISTRY, WorldgenDataSource,
    WorldgenRegistries,
};
use serde_json::Value;

use super::access::{RegistrationInfo, Registry, RegistryAccess, RegistryEntry};
use super::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use super::decoders::{DecodedEntry, decoder_for};
use super::keys::{self, RegistryData, RegistryValidator};
use super::tags::{ElementRef, TagLoader};
use crate::server::packs::{FileToIdConverter, ResourceManager, throwable};
use crate::world::item::crafting::{CustomRecipe, Recipe, RecipeKind};

/// A load failure of one element or registry (a Java `Exception`: class, message and optional cause).
#[derive(Clone, Debug)]
pub struct LoadingError {
    pub class: &'static str,
    pub message: String,
    pub cause: Option<(&'static str, String)>,
}

impl LoadingError {
    fn failed_to_parse(
        id: &Identifier,
        pack: &str,
        cause_class: &'static str,
        cause: String,
    ) -> LoadingError {
        LoadingError {
            class: "java.lang.IllegalStateException",
            message: format!("Failed to parse {id} from pack {pack}"),
            cause: Some((cause_class, cause)),
        }
    }

    fn state(message: String) -> LoadingError {
        LoadingError {
            class: "java.lang.IllegalStateException",
            message,
            cause: None,
        }
    }

    /// `printStackTrace` without the frames.
    pub fn print(&self) -> String {
        let mut out = throwable(self.class, &self.message);
        if let Some((class, message)) = &self.cause {
            out.push_str("\nCaused by: ");
            out.push_str(&throwable(class, message));
        }
        out
    }
}

/// The key of a loading error: `(registry of the key, identifier)`; element keys are `(registry, element id)`, a failing
/// registry itself is `(minecraft:root, registry id)`.
pub type ErrorKey = (String, Identifier);

/// `ReportedException("Failed to load registries due to errors")`.
#[derive(Debug)]
pub struct RegistryLoadFailure {
    pub errors: BTreeMap<ErrorKey, LoadingError>,
    /// An exception thrown out of the load itself (a registry refusing a value it already holds), which replaces the report.
    pub thrown: Option<LoadingError>,
}

impl RegistryLoadFailure {
    /// `printFullDetailsToLog`: the text logged as `Registry loading errors:\n...`.
    pub fn full_details(&self) -> String {
        let mut out = String::new();
        let mut by_registry: BTreeMap<&str, BTreeMap<&Identifier, &LoadingError>> = BTreeMap::new();
        for ((registry, id), e) in &self.errors {
            by_registry.entry(registry).or_default().insert(id, e);
        }
        for (registry, elements) in by_registry {
            out.push_str(&format!("> Errors in registry {registry}:\n"));
            for (id, e) in elements {
                out.push_str(&format!(">> Errors in element {id}:\n{}\n", e.print()));
            }
        }
        out
    }

    /// The crash report's `Loading info` / `Errors` detail: one `\n\t\t<registry>/<id>: <message>` per error.
    pub fn brief(&self) -> String {
        let mut keys: Vec<&ErrorKey> = self.errors.keys().collect();
        // `Comparator.comparing(ResourceKey::registry).thenComparing(ResourceKey::identifier)`
        keys.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        keys.iter()
            .map(|k| format!("\n\t\t{}/{}: {}", k.0, k.1, self.errors[*k].message))
            .collect()
    }

    /// Log like `RegistryDataLoader.logErrors`.
    pub fn log(&self) {
        log::error!("Registry loading errors:\n{}", self.full_details());
    }
}

impl fmt::Display for RegistryLoadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.thrown {
            Some(thrown) => f.write_str(&thrown.print()),
            None => write!(f, "Failed to load registries due to errors{}", self.brief()),
        }
    }
}

impl std::error::Error for RegistryLoadFailure {}

struct ParsedElement {
    id: Identifier,
    json: Arc<Value>,
    source_pack: String,
    info: RegistrationInfo,
}

/// What decoders may look up while a stage loads: context registries are answered exactly, registries of the stage
/// are `Pending` (verified when the stage is frozen).
struct StageLookup<'a> {
    context: &'a RegistryAccess,
    stage: HashSet<&'static str>,
}

impl RegistryLookup for StageLookup<'_> {
    fn contains(&self, registry: &str, id: &Identifier) -> Lookup {
        if self.stage.contains(registry) {
            return Lookup::Pending;
        }
        match self.context.registry(registry) {
            Some(r) => {
                if r.contains(id) {
                    Lookup::Yes
                } else {
                    Lookup::No
                }
            }
            None => Lookup::NoRegistry,
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup {
        if self.stage.contains(registry) {
            return Lookup::Pending;
        }
        match self.context.registry(registry) {
            Some(r) => {
                if r.tag(tag).is_some() {
                    Lookup::Yes
                } else {
                    Lookup::No
                }
            }
            None => Lookup::NoRegistry,
        }
    }
}

/// The parsed JSON of the worldgen registries of a stage as a `WorldgenDataSource`.
struct StageJsonSource {
    entries: HashMap<(&'static str, Identifier), Arc<Value>>,
}

impl WorldgenDataSource for StageJsonSource {
    fn get(&self, registry: &str, id: &Identifier) -> Option<Value> {
        let key = [
            NOISE_REGISTRY,
            DENSITY_FUNCTION_REGISTRY,
            NOISE_SETTINGS_REGISTRY,
        ]
        .into_iter()
        .find(|r| *r == registry)?;
        self.entries.get(&(key, id.clone())).map(|v| (**v).clone())
    }
}

/// A `WorldgenDataSource` over the finished registries of `access` (for the chunk generator, after loading).
pub struct RegistryWorldgenSource(pub RegistryAccess);

impl WorldgenDataSource for RegistryWorldgenSource {
    fn get(&self, registry: &str, id: &Identifier) -> Option<Value> {
        let reg = self.0.registry(&format!("minecraft:{registry}"))?;
        reg.entry_by_key(id).map(|e| (*e.json).clone())
    }

    fn list(&self, registry: &str) -> Vec<Identifier> {
        let Some(reg) = self.0.registry(&format!("minecraft:{registry}")) else {
            return Vec::new();
        };
        let mut ids = reg.names();
        ids.sort_by_key(|id| id.to_string());
        ids
    }
}

impl RegistryAccess {
    /// `WorldgenRegistries` (noises, density functions, noise settings) over this access' loaded JSON.
    pub fn worldgen_registries(&self) -> WorldgenRegistries {
        WorldgenRegistries::new(Arc::new(RegistryWorldgenSource(self.clone())))
    }
}

/// `RegistryDataLoader.load(resourceManager, contextRegistries, registriesToLoad)`: the new registries of this stage.
/// `context` holds every registry of earlier stages (static registries with their tags applied).
pub fn load(
    manager: &dyn ResourceManager,
    context: &RegistryAccess,
    registries_to_load: &[RegistryData],
) -> Result<RegistryAccess, RegistryLoadFailure> {
    let mut errors: BTreeMap<ErrorKey, LoadingError> = BTreeMap::new();
    let stage: HashSet<&'static str> = registries_to_load.iter().map(|r| r.key).collect();
    let lookup = StageLookup { context, stage };

    // listing + JSON parsing of every registry (ResourceManagerRegistryLoadTask.load, first half)
    let mut parsed: Vec<Vec<ParsedElement>> = Vec::with_capacity(registries_to_load.len());
    for data in registries_to_load {
        let lister = FileToIdConverter::json(keys::dir_path(data.key));
        let mut elements = Vec::new();
        for (file_id, resource) in lister.list_matching_resources(manager) {
            let id = lister.file_to_id(&file_id);
            let pack = resource.source_pack_id().to_owned();
            let known_pack = resource.known_pack_info().cloned();
            let info = RegistrationInfo {
                stable: known_pack.as_ref().is_some_and(|k| k.is_vanilla()),
                known_pack,
            };
            let json = resource
                .read_all_as_string()
                .map_err(|e| ("java.io.IOException", e.to_string()))
                .and_then(|text| {
                    serde_json::from_str::<Value>(&text)
                        .map_err(|e| ("com.google.gson.JsonSyntaxException", e.to_string()))
                });
            match json {
                Ok(json) => elements.push(ParsedElement {
                    id,
                    json: Arc::new(json),
                    source_pack: pack,
                    info,
                }),
                Err((class, message)) => {
                    errors.insert(
                        (data.key.to_owned(), id.clone()),
                        LoadingError::failed_to_parse(&id, &pack, class, message),
                    );
                }
            }
        }
        parsed.push(elements);
    }

    // the worldgen registries decode against each other through `mc_worldgen::data`
    let worldgen_source = StageJsonSource {
        entries: registries_to_load
            .iter()
            .zip(&parsed)
            .filter_map(|(d, elements)| {
                let key = [
                    NOISE_REGISTRY,
                    DENSITY_FUNCTION_REGISTRY,
                    NOISE_SETTINGS_REGISTRY,
                ]
                .into_iter()
                .find(|r| keys::dir_path(d.key) == *r)?;
                Some(
                    elements
                        .iter()
                        .map(move |e| ((key, e.id.clone()), e.json.clone())),
                )
            })
            .flatten()
            .collect(),
    };
    let worldgen = WorldgenRegistries::new(Arc::new(worldgen_source));

    // decode + register, in file-identifier order
    let mut unbound_values: Vec<(String, Identifier)> = Vec::new();
    let mut unbound_tags: Vec<(String, Identifier)> = Vec::new();
    let mut stage_entries: Vec<(RegistryData, Vec<RegistryEntry>)> = Vec::new();
    for (data, elements) in registries_to_load.iter().zip(parsed) {
        let decoder = decoder_for(data.key);
        let mut entries = Vec::with_capacity(elements.len());
        let mut has_repair_item = false;
        for element in elements {
            let mut ctx = RegistryDecodeContext::new(&lookup);
            ctx.worldgen = Some(&worldgen);
            ctx.entry = Some(element.id.clone());
            let decoded: Result<Option<DecodedEntry>, String> = match decoder {
                Some(d) => (d.decode)(&element.json, &ctx).map(Some),
                None => generic_validate(data.key, &element.json).and_then(|()| {
                    match super::typed::network::network_schema(data.key) {
                        // no typed value, but the network codec is modelled: validate and keep its NBT
                        Some(schema) => schema.convert(&element.json, &ctx).map(|tag| {
                            Some(DecodedEntry {
                                value: Arc::new(()),
                                network: Some(tag),
                            })
                        }),
                        None => Ok(None),
                    }
                }),
            };
            unbound_values.extend(ctx.unbound_values.take());
            unbound_tags.extend(ctx.unbound_tags.take());
            match decoded {
                Ok(decoded) => {
                    // `RepairItemRecipe.INSTANCE` is the value of every `crafting_special_repairitem` recipe: the registry
                    // refuses the second one (`MappedRegistry.register`) and the exception aborts the whole load
                    let repair_item = decoded
                        .as_ref()
                        .and_then(|d| d.value.downcast_ref::<Recipe>())
                        .is_some_and(|r| {
                            matches!(&r.kind, RecipeKind::Custom(CustomRecipe::RepairItem(_)))
                        });
                    if repair_item && std::mem::replace(&mut has_repair_item, true) {
                        return Err(RegistryLoadFailure {
                            errors: BTreeMap::new(),
                            // the identity hash code of the shared instance is not reproducible
                            thrown: Some(LoadingError::state(
                                "Adding duplicate value 'net.minecraft.world.item.crafting.RepairItemRecipe@0' to registry".to_owned(),
                            )),
                        });
                    }
                    let (value, network) = match decoded {
                        Some(d) if d.value.is::<()>() => (None, d.network),
                        Some(d) => (Some(d.value), d.network),
                        None => (None, None),
                    };
                    entries.push(RegistryEntry {
                        id: element.id,
                        json: element.json,
                        value,
                        network,
                        source_pack: element.source_pack,
                        info: element.info,
                    });
                }
                Err(message) => {
                    errors.insert(
                        (data.key.to_owned(), element.id.clone()),
                        LoadingError::failed_to_parse(
                            &element.id,
                            &element.source_pack,
                            "java.lang.IllegalStateException",
                            message,
                        ),
                    );
                }
            }
        }
        stage_entries.push((*data, entries));
    }

    // tags (ResourceManagerRegistryLoadTask.load, second half)
    let mut registries: Vec<Registry> = Vec::with_capacity(stage_entries.len());
    let mut validators: Vec<(RegistryData, usize)> = Vec::new();
    for (data, entries) in stage_entries {
        let by_id: HashMap<Identifier, u32> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i as u32))
            .collect();
        let key = data.key;
        let mut tag_unbound: Vec<(String, Identifier)> = Vec::new();
        let tags = {
            let mut lookup_element = |id: &Identifier, required: bool| match by_id.get(id) {
                Some(i) => ElementRef::Found(*i),
                // required: `registration lookup` hands out an unbound holder; optional: only registered elements
                None if required => {
                    tag_unbound.push((key.to_owned(), id.clone()));
                    ElementRef::Unbound
                }
                None => ElementRef::Missing,
            };
            let mut loader = TagLoader::new(&mut lookup_element, &keys::tags_dir_path(key));
            let builders = loader.load(manager);
            loader.build(&builders)
        };
        unbound_values.extend(tag_unbound);
        let tags: HashMap<Identifier, Arc<[u32]>> =
            tags.into_iter().map(|(k, v)| (k, Arc::from(v))).collect();
        validators.push((data, entries.len()));
        registries.push(Registry::dynamic(key, entries).with_tags(tags));
    }

    // freezeRegistry: unbound values / tags
    for registry in &registries {
        let unbound: std::collections::BTreeSet<&Identifier> = unbound_values
            .iter()
            .filter(|(r, id)| r == registry.key() && !registry.contains(id))
            .map(|(_, id)| id)
            .collect();
        let unbound_t: std::collections::BTreeSet<&Identifier> = unbound_tags
            .iter()
            .filter(|(r, t)| r == registry.key() && registry.tag(t).is_none())
            .map(|(_, t)| t)
            .collect();
        let registry_error_key = || {
            (
                "minecraft:root".to_owned(),
                Identifier::parse(registry.key()).unwrap_or_else(|_| {
                    Identifier::with_default_namespace("unknown").expect("valid")
                }),
            )
        };
        if !unbound.is_empty() {
            errors.insert(
                registry_error_key(),
                LoadingError::state(format!(
                    "Unbound values in registry ResourceKey[minecraft:root / {}]: [{}]",
                    registry.key(),
                    unbound
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
        } else if !unbound_t.is_empty() {
            errors.insert(
                registry_error_key(),
                LoadingError::state(format!(
                    "Unbound tags in registry ResourceKey[minecraft:root / {}]: [{}]",
                    registry.key(),
                    unbound_t
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
        }
    }
    if !errors.is_empty() {
        let failure = RegistryLoadFailure {
            errors,
            thrown: None,
        };
        failure.log();
        return Err(failure);
    }

    // validateRegistry
    for (data, len) in validators {
        if data.validator == RegistryValidator::NonEmpty && len == 0 {
            let key = (
                "minecraft:root".to_owned(),
                Identifier::parse(data.key).unwrap_or_else(|_| {
                    Identifier::with_default_namespace("unknown").expect("valid")
                }),
            );
            errors.insert(
                key,
                LoadingError::state(format!("Registry must be non-empty: {}", data.key)),
            );
        }
    }
    if !errors.is_empty() {
        let failure = RegistryLoadFailure {
            errors,
            thrown: None,
        };
        failure.log();
        return Err(failure);
    }
    let access = RegistryAccess::from_registries(registries);
    if registries_to_load.iter().any(|r| r.key == keys::FEATURE) {
        validate_feature_layer(&access)?;
    }
    Ok(access)
}

/// The decoders of `worldgen/{feature,placed_feature,structure,structure_set,processor_list,template_pool}` run at the
/// end of the world stage: they need the whole stage (features reference each other, biomes list placed features,
/// structures list biomes), so every entry is decoded against the finished registries and the first failing entry
/// fails the load like any other registry decode error (vanilla collects all of them; the first is reported here).
fn validate_feature_layer(stage: &RegistryAccess) -> Result<(), RegistryLoadFailure> {
    use crate::world::level::levelgen::registries::GenRegistries;
    use crate::world::level::levelgen::structure::registries::StructureRegistries;
    use mc_worldgen::biome::BiomeRegistry;
    use mc_worldgen::structure_placement::StructureSetRegistry;

    let source = Arc::new(RegistryWorldgenSource(stage.clone()));
    let worldgen = stage.worldgen_registries();
    let decoded = BiomeRegistry::load(&worldgen).and_then(|biomes| {
        GenRegistries::load(source.clone(), &worldgen, &biomes)?;
        StructureRegistries::load(source, &worldgen, &biomes)?;
        StructureSetRegistry::load(&worldgen, &biomes).map(|_| ())
    });
    let Err(error) = decoded else { return Ok(()) };
    // `LoadError::in_entry`: `<registry>/<id>: <path>: <message>`
    let text = error.to_string();
    let entry = text.strip_prefix("worldgen/").and_then(|rest| {
        let (registry, rest) = rest.split_once('/')?;
        let (id, message) = rest.split_once(": ")?;
        Some((
            format!("minecraft:worldgen/{registry}"),
            Identifier::try_parse(id)?,
            message.to_owned(),
        ))
    });
    let (key, failure) = match entry {
        Some((registry, id, message)) => {
            let pack = stage
                .registry(&registry)
                .and_then(|r| r.entry_by_key(&id).map(|e| e.source_pack.clone()))
                .unwrap_or_default();
            (
                (registry, id.clone()),
                LoadingError::failed_to_parse(
                    &id,
                    &pack,
                    "java.lang.IllegalStateException",
                    message,
                ),
            )
        }
        None => (
            (
                "minecraft:root".to_owned(),
                Identifier::with_default_namespace("worldgen").expect("valid"),
            ),
            LoadingError::state(text),
        ),
    };
    let failure = RegistryLoadFailure {
        errors: BTreeMap::from([(key, failure)]),
        thrown: None,
    };
    failure.log();
    Err(failure)
}

/// Registries without a typed decoder: the entry must at least be a JSON object, except for the registries whose codec
/// has compact forms (constants / references for density functions and context providers, lists for block transformers,
/// item modifiers and predicates).
fn generic_validate(registry: &str, json: &Value) -> Result<(), String> {
    let allowed = match json {
        Value::Object(_) => true,
        Value::Number(_) => matches!(
            registry,
            keys::DENSITY_FUNCTION | keys::CONTEXT_FLOAT_PROVIDER | keys::CONTEXT_INT_PROVIDER
        ),
        Value::String(_) => registry == keys::DENSITY_FUNCTION,
        Value::Array(_) => matches!(
            registry,
            keys::BLOCK_TRANSFORMER | keys::ITEM_MODIFIER | keys::PREDICATE
        ),
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(format!(
            "Not a JSON object: {}",
            serde_json::to_string(json).unwrap_or_default()
        ))
    }
}

#[cfg(test)]
mod feature_layer_tests {
    use super::*;

    fn access_with(registry: &'static str, id: &str, json: Value, pack: &str) -> RegistryAccess {
        let entry = RegistryEntry {
            id: Identifier::parse(id).unwrap(),
            json: Arc::new(json),
            value: None,
            network: None,
            source_pack: pack.to_owned(),
            info: RegistrationInfo::default(),
        };
        RegistryAccess::from_registries([Registry::dynamic(registry, vec![entry])])
    }

    /// A data pack placed feature that does not decode fails the world load like a vanilla registry decode error, naming
    /// the registry, the entry and the pack.
    #[test]
    fn an_undecodable_placed_feature_fails_the_load() {
        let access = access_with(
            "minecraft:worldgen/placed_feature",
            "test:broken",
            serde_json::json!({"feature": "test:missing", "placement": []}),
            "file/test_pack",
        );
        let failure = validate_feature_layer(&access).expect_err("the entry is invalid");
        let ((registry, id), error) = failure.errors.iter().next().unwrap();
        assert_eq!(
            (registry.as_str(), id.to_string().as_str()),
            ("minecraft:worldgen/placed_feature", "test:broken")
        );
        assert_eq!(
            error.message,
            "Failed to parse test:broken from pack file/test_pack"
        );
        assert!(
            error.cause.as_ref().unwrap().1.contains("test:missing"),
            "{error:?}"
        );
        assert_eq!(failure.errors.len(), 1);
    }
}
