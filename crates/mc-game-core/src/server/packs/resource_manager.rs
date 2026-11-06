//! `net.minecraft.server.packs.resources`: `Resource`, `ResourceManager`, `FallbackResourceManager` (one namespace over a
//! stack of packs, later packs override earlier ones, `pack.mcmeta` `filter` sections hide lower packs' files) and
//! `MultiPackResourceManager`; `FileToIdConverter` (`net.minecraft.resources`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::sync::Arc;

use mc_core::ident::Identifier;

use super::PackType;
use super::pack_format::ResourceFilterSection;
use super::repository::KnownPack;
use super::resources::{IoSupplier, PackResources, ResourceMetadata};

type MetaSupplier = Arc<dyn Fn() -> io::Result<ResourceMetadata> + Send + Sync>;

/// `Resource`: one file of one pack.
#[derive(Clone)]
pub struct Resource {
    source: Arc<dyn PackResources>,
    stream: IoSupplier,
    metadata: MetaSupplier,
}

impl Resource {
    pub fn new(
        source: Arc<dyn PackResources>,
        stream: IoSupplier,
        metadata: MetaSupplier,
    ) -> Resource {
        Resource {
            source,
            stream,
            metadata,
        }
    }

    pub fn source(&self) -> &Arc<dyn PackResources> {
        &self.source
    }

    pub fn source_pack_id(&self) -> &str {
        self.source.pack_id()
    }

    pub fn known_pack_info(&self) -> Option<&KnownPack> {
        self.source.known_pack_info()
    }

    /// `open()`: the file contents.
    pub fn open(&self) -> io::Result<Vec<u8>> {
        (self.stream)()
    }

    /// `readAllAsString()` (UTF-8, invalid sequences are replaced like `InputStreamReader`).
    pub fn read_all_as_string(&self) -> io::Result<String> {
        Ok(String::from_utf8_lossy(&self.open()?).into_owned())
    }

    /// `metadata()`: the sibling `<file>.mcmeta`.
    pub fn metadata(&self) -> io::Result<ResourceMetadata> {
        (self.metadata)()
    }
}

fn empty_metadata() -> MetaSupplier {
    Arc::new(|| Ok(ResourceMetadata::EMPTY))
}

fn metadata_of(supplier: IoSupplier) -> MetaSupplier {
    Arc::new(move || parse_metadata(&supplier))
}

fn parse_metadata(supplier: &IoSupplier) -> io::Result<ResourceMetadata> {
    ResourceMetadata::from_json_bytes(&supplier()?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// `ResourceManager` (+ `ResourceProvider`).
pub trait ResourceManager: Send + Sync {
    fn namespaces(&self) -> BTreeSet<String>;
    fn get_resource(&self, location: &Identifier) -> Option<Resource>;
    /// All packs' versions of a file, lowest priority first.
    fn get_resource_stack(&self, location: &Identifier) -> Vec<Resource>;
    /// `listResources(directory, selector)`: the top-most file for each id (ids sorted like a `TreeMap`).
    fn list_resources(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Resource>;
    fn list_resource_stacks(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Vec<Resource>>;
    fn list_packs(&self) -> Vec<Arc<dyn PackResources>>;
}

/// `ResourceManager.Empty`.
pub struct EmptyResourceManager;

impl ResourceManager for EmptyResourceManager {
    fn namespaces(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
    fn get_resource(&self, _: &Identifier) -> Option<Resource> {
        None
    }
    fn get_resource_stack(&self, _: &Identifier) -> Vec<Resource> {
        Vec::new()
    }
    fn list_resources(
        &self,
        _: &str,
        _: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Resource> {
        BTreeMap::new()
    }
    fn list_resource_stacks(
        &self,
        _: &str,
        _: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Vec<Resource>> {
        BTreeMap::new()
    }
    fn list_packs(&self) -> Vec<Arc<dyn PackResources>> {
        Vec::new()
    }
}

fn is_metadata(location: &Identifier) -> bool {
    location.path().ends_with(".mcmeta")
}

fn identifier_from_metadata(id: &Identifier) -> Identifier {
    with_path(id, &id.path()[..id.path().len() - ".mcmeta".len()])
}

fn metadata_location(id: &Identifier) -> Identifier {
    with_path(id, &format!("{}.mcmeta", id.path()))
}

fn with_path(id: &Identifier, path: &str) -> Identifier {
    // paths derived from a valid identifier by adding/removing `.mcmeta` stay valid
    Identifier::try_build(id.namespace(), path).unwrap_or_else(|| id.clone())
}

type PathFilter = Arc<dyn Fn(&Identifier) -> bool + Send + Sync>;

struct PackEntry {
    name: String,
    resources: Option<Arc<dyn PackResources>>,
    filter: Option<PathFilter>,
}

impl PackEntry {
    fn is_filtered(&self, location: &Identifier) -> bool {
        self.filter.as_ref().is_some_and(|f| f(location))
    }
}

/// `FallbackResourceManager`: the packs providing one namespace.
pub struct FallbackResourceManager {
    fallbacks: Vec<PackEntry>,
    ty: PackType,
    namespace: String,
}

impl FallbackResourceManager {
    pub fn new(ty: PackType, namespace: &str) -> FallbackResourceManager {
        FallbackResourceManager {
            fallbacks: Vec::new(),
            ty,
            namespace: namespace.to_owned(),
        }
    }

    pub fn push(&mut self, pack: Arc<dyn PackResources>, filter: Option<PathFilter>) {
        self.fallbacks.push(PackEntry {
            name: pack.pack_id().to_owned(),
            resources: Some(pack),
            filter,
        });
    }

    pub fn push_filter_only(&mut self, name: &str, filter: PathFilter) {
        self.fallbacks.push(PackEntry {
            name: name.to_owned(),
            resources: None,
            filter: Some(filter),
        });
    }

    fn stack_metadata_finder(
        &self,
        location: &Identifier,
        final_pack_index: usize,
    ) -> MetaSupplier {
        // the closure must not borrow `self`: capture the (pack, filter) chain it needs
        let metadata_location = metadata_location(location);
        let ty = self.ty;
        let chain: Vec<(Option<Arc<dyn PackResources>>, Option<PathFilter>)> = self.fallbacks
            [final_pack_index..]
            .iter()
            .map(|e| (e.resources.clone(), e.filter.clone()))
            .collect();
        Arc::new(move || {
            for (pack, filter) in chain.iter().rev() {
                if let Some(supplier) = pack
                    .as_ref()
                    .and_then(|p| p.get_resource(ty, &metadata_location))
                {
                    return parse_metadata(&supplier);
                }
                if filter.as_ref().is_some_and(|f| f(&metadata_location)) {
                    break;
                }
            }
            Ok(ResourceMetadata::EMPTY)
        })
    }
}

impl ResourceManager for FallbackResourceManager {
    fn namespaces(&self) -> BTreeSet<String> {
        BTreeSet::from([self.namespace.clone()])
    }

    fn get_resource(&self, location: &Identifier) -> Option<Resource> {
        for i in (0..self.fallbacks.len()).rev() {
            let entry = &self.fallbacks[i];
            if let Some(pack) = &entry.resources {
                if let Some(resource) = pack.get_resource(self.ty, location) {
                    return Some(Resource::new(
                        pack.clone(),
                        resource,
                        self.stack_metadata_finder(location, i),
                    ));
                }
            }
            if entry.is_filtered(location) {
                log::warn!(
                    "Resource {location} not found, but was filtered by pack {}",
                    entry.name
                );
                return None;
            }
        }
        None
    }

    fn get_resource_stack(&self, location: &Identifier) -> Vec<Resource> {
        let metadata_location = metadata_location(location);
        let mut result = Vec::new();
        let mut filter_meta = false;
        let mut last_filter_name: Option<&str> = None;
        for i in (0..self.fallbacks.len()).rev() {
            let entry = &self.fallbacks[i];
            if let Some(pack) = &entry.resources {
                if let Some(resource) = pack.get_resource(self.ty, location) {
                    let metadata: MetaSupplier = if filter_meta {
                        empty_metadata()
                    } else {
                        let (pack, ty, ml) = (pack.clone(), self.ty, metadata_location.clone());
                        Arc::new(move || match pack.get_resource(ty, &ml) {
                            Some(m) => parse_metadata(&m),
                            None => Ok(ResourceMetadata::EMPTY),
                        })
                    };
                    result.push(Resource::new(pack.clone(), resource, metadata));
                }
            }
            if entry.is_filtered(location) {
                last_filter_name = Some(&entry.name);
                break;
            }
            if entry.is_filtered(&metadata_location) {
                filter_meta = true;
            }
        }
        if result.is_empty() {
            if let Some(name) = last_filter_name {
                log::warn!("Resource {location} not found, but was filtered by pack {name}");
            }
        }
        result.reverse();
        result
    }

    fn list_resources(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Resource> {
        struct Top {
            pack: Arc<dyn PackResources>,
            resource: IoSupplier,
            pack_index: usize,
        }
        let mut top_for_file: HashMap<Identifier, Top> = HashMap::new();
        let mut top_for_meta: HashMap<Identifier, Top> = HashMap::new();
        for (i, entry) in self.fallbacks.iter().enumerate() {
            if let Some(filter) = &entry.filter {
                top_for_file.retain(|k, _| !filter(k));
                top_for_meta.retain(|k, _| !filter(k));
            }
            let Some(pack) = &entry.resources else {
                continue;
            };
            pack.list_resources(
                self.ty,
                &self.namespace,
                directory,
                &mut |resource, supplier| {
                    if is_metadata(&resource) {
                        if selector(&identifier_from_metadata(&resource)) {
                            top_for_meta.insert(
                                resource,
                                Top {
                                    pack: pack.clone(),
                                    resource: supplier,
                                    pack_index: i,
                                },
                            );
                        }
                    } else if selector(&resource) {
                        top_for_file.insert(
                            resource,
                            Top {
                                pack: pack.clone(),
                                resource: supplier,
                                pack_index: i,
                            },
                        );
                    }
                },
            );
        }
        let mut result = BTreeMap::new();
        for (location, resource) in top_for_file {
            let meta_getter = match top_for_meta.get(&metadata_location(&location)) {
                Some(meta) if meta.pack_index >= resource.pack_index => {
                    metadata_of(meta.resource.clone())
                }
                _ => empty_metadata(),
            };
            result.insert(
                location,
                Resource::new(resource.pack, resource.resource, meta_getter),
            );
        }
        result
    }

    fn list_resource_stacks(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Vec<Resource>> {
        struct EntryStack {
            file_sources: Vec<(Arc<dyn PackResources>, IoSupplier)>,
            meta_sources: Vec<(Arc<dyn PackResources>, IoSupplier)>,
        }
        let mut found: HashMap<Identifier, EntryStack> = HashMap::new();
        for entry in &self.fallbacks {
            // applyPackFiltersToExistingResources
            for (location, stack) in found.iter_mut() {
                if entry.is_filtered(location) {
                    stack.file_sources.clear();
                } else if entry.is_filtered(&metadata_location(location)) {
                    stack.meta_sources.clear();
                }
            }
            let Some(pack) = &entry.resources else {
                continue;
            };
            pack.list_resources(self.ty, &self.namespace, directory, &mut |id, resource| {
                let new_stack = || EntryStack {
                    file_sources: Vec::new(),
                    meta_sources: Vec::new(),
                };
                if is_metadata(&id) {
                    let actual = identifier_from_metadata(&id);
                    if !selector(&actual) {
                        return;
                    }
                    let stack = found.entry(actual).or_insert_with(new_stack);
                    stack.meta_sources.retain(|(p, _)| !Arc::ptr_eq(p, pack));
                    stack.meta_sources.push((pack.clone(), resource));
                } else {
                    if !selector(&id) {
                        return;
                    }
                    found
                        .entry(id)
                        .or_insert_with(new_stack)
                        .file_sources
                        .push((pack.clone(), resource));
                }
            });
        }
        let mut result = BTreeMap::new();
        for (location, entry) in found {
            if entry.file_sources.is_empty() {
                continue;
            }
            let resources = entry
                .file_sources
                .iter()
                .map(|(source, resource)| {
                    let meta = entry
                        .meta_sources
                        .iter()
                        .find(|(p, _)| Arc::ptr_eq(p, source))
                        .map(|(_, m)| metadata_of(m.clone()));
                    Resource::new(
                        source.clone(),
                        resource.clone(),
                        meta.unwrap_or_else(empty_metadata),
                    )
                })
                .collect();
            result.insert(location, resources);
        }
        result
    }

    fn list_packs(&self) -> Vec<Arc<dyn PackResources>> {
        self.fallbacks
            .iter()
            .filter_map(|p| p.resources.clone())
            .collect()
    }
}

/// `CloseableResourceManager` over a pack stack: `MultiPackResourceManager`.
pub struct MultiPackResourceManager {
    namespaced_managers: HashMap<String, FallbackResourceManager>,
    packs: Vec<Arc<dyn PackResources>>,
}

impl MultiPackResourceManager {
    pub fn new(ty: PackType, packs: Vec<Arc<dyn PackResources>>) -> MultiPackResourceManager {
        let mut namespaced: HashMap<String, FallbackResourceManager> = HashMap::new();
        let mut namespaces: Vec<String> = Vec::new();
        for pack in &packs {
            // `flatMap(...).distinct()`: first-seen order (sets are unordered in Java, only membership matters below)
            let mut ns: Vec<String> = pack.get_namespaces(ty).into_iter().collect();
            ns.sort();
            for n in ns {
                if !namespaces.contains(&n) {
                    namespaces.push(n);
                }
            }
        }
        for pack in &packs {
            let filter_section = MultiPackResourceManager::pack_filter_section(pack.as_ref());
            let provided = pack.get_namespaces(ty);
            let path_filter: Option<PathFilter> = filter_section.clone().map(|f| {
                Arc::new(move |l: &Identifier| f.is_path_filtered(l.path())) as PathFilter
            });
            for namespace in &namespaces {
                let contains = provided.contains(namespace);
                let filter_matches = filter_section
                    .as_ref()
                    .is_some_and(|f| f.is_namespace_filtered(namespace));
                if contains || filter_matches {
                    let manager = namespaced
                        .entry(namespace.clone())
                        .or_insert_with(|| FallbackResourceManager::new(ty, namespace));
                    match (contains, filter_matches, &path_filter) {
                        (true, true, filter) => manager.push(pack.clone(), filter.clone()),
                        (true, false, _) => manager.push(pack.clone(), None),
                        (false, _, Some(filter)) => {
                            manager.push_filter_only(pack.pack_id(), filter.clone())
                        }
                        (false, _, None) => {}
                    }
                }
            }
        }
        MultiPackResourceManager {
            namespaced_managers: namespaced,
            packs,
        }
    }

    fn pack_filter_section(pack: &dyn PackResources) -> Option<ResourceFilterSection> {
        match pack.metadata().map_err(|e| e.to_string()).and_then(|m| {
            m.get("filter", ResourceFilterSection::decode)
                .map_err(|e| e.to_string())
        }) {
            Ok(section) => section,
            Err(_) => {
                log::error!("Failed to get filter section from pack {}", pack.pack_id());
                None
            }
        }
    }

    /// `close()`.
    pub fn close(&self) {
        self.packs.iter().for_each(|p| p.close());
    }
}

fn check_trailing_directory_path(directory: &str) {
    assert!(
        !directory.ends_with('/'),
        "Trailing slash in path {directory}"
    );
}

impl ResourceManager for MultiPackResourceManager {
    fn namespaces(&self) -> BTreeSet<String> {
        self.namespaced_managers.keys().cloned().collect()
    }

    fn get_resource(&self, location: &Identifier) -> Option<Resource> {
        self.namespaced_managers
            .get(location.namespace())?
            .get_resource(location)
    }

    fn get_resource_stack(&self, location: &Identifier) -> Vec<Resource> {
        self.namespaced_managers
            .get(location.namespace())
            .map(|m| m.get_resource_stack(location))
            .unwrap_or_default()
    }

    fn list_resources(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Resource> {
        check_trailing_directory_path(directory);
        self.namespaced_managers
            .values()
            .flat_map(|m| m.list_resources(directory, selector))
            .collect()
    }

    fn list_resource_stacks(
        &self,
        directory: &str,
        selector: &dyn Fn(&Identifier) -> bool,
    ) -> BTreeMap<Identifier, Vec<Resource>> {
        check_trailing_directory_path(directory);
        self.namespaced_managers
            .values()
            .flat_map(|m| m.list_resource_stacks(directory, selector))
            .collect()
    }

    fn list_packs(&self) -> Vec<Arc<dyn PackResources>> {
        self.packs.clone()
    }
}

/// `FileToIdConverter`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileToIdConverter {
    prefix: String,
    extension: String,
}

impl FileToIdConverter {
    pub fn new(prefix: &str, extension: &str) -> FileToIdConverter {
        FileToIdConverter {
            prefix: prefix.to_owned(),
            extension: extension.to_owned(),
        }
    }

    pub fn json(prefix: &str) -> FileToIdConverter {
        FileToIdConverter::new(prefix, ".json")
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn id_to_file(&self, id: &Identifier) -> Identifier {
        with_path(
            id,
            &format!("{}/{}{}", self.prefix, id.path(), self.extension),
        )
    }

    pub fn file_to_id(&self, file: &Identifier) -> Identifier {
        let path = file.path();
        with_path(
            file,
            &path[self.prefix.len() + 1..path.len() - self.extension.len()],
        )
    }

    pub fn extension_matches(&self, id: &Identifier) -> bool {
        id.path().ends_with(&self.extension)
    }

    pub fn list_matching_resources(
        &self,
        manager: &dyn ResourceManager,
    ) -> BTreeMap<Identifier, Resource> {
        manager.list_resources(&self.prefix, &|id| self.extension_matches(id))
    }

    pub fn list_matching_resource_stacks(
        &self,
        manager: &dyn ResourceManager,
    ) -> BTreeMap<Identifier, Vec<Resource>> {
        manager.list_resource_stacks(&self.prefix, &|id| self.extension_matches(id))
    }
}
