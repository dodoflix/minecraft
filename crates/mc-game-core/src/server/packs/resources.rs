//! `PackResources` and its implementations: `PathPackResources` (directory packs), `FilePackResources` (zip packs),
//! `OverlayedPackResources`, and `EmbeddedPackResources` (the built-in vanilla pack and its feature packs, served from
//! `mc_data::datapack` instead of the jar's file system, i.e. vanilla's `FixedPathPackResources` / jar-backed
//! `PathPackResources`).

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mc_core::ident::{self, Identifier};
use serde_json::{Map, Value};
use zip::ZipArchive;

use super::repository::{KnownPack, PackMetadata, ResourcesSupplier};
use super::{PackLocationInfo, PackType};

/// `IoSupplier<InputStream>`: every call produces the resource's full bytes.
pub type IoSupplier = Arc<dyn Fn() -> io::Result<Vec<u8>> + Send + Sync>;

/// Failure of `getMetadataSection`: `Json` is vanilla's `JsonParseException` (the one `Pack.readPackMetadata` retries with the
/// fallback type), `Io` any other exception.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataError {
    Json(String),
    Io(String),
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataError::Json(m) | MetadataError::Io(m) => f.write_str(m),
        }
    }
}

/// `ResourceMetadata`: the sections of a `*.mcmeta` file, plus the built-in sections of the vanilla pack.
#[derive(Clone, Debug, Default)]
pub struct ResourceMetadata {
    sections: Option<Map<String, Value>>,
    built_in: Option<Map<String, Value>>,
}

impl ResourceMetadata {
    pub const EMPTY: ResourceMetadata = ResourceMetadata { sections: None, built_in: None };

    /// `ResourceMetadata.fromJsonStream`: the top level must be a JSON object.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<ResourceMetadata, MetadataError> {
        let text = std::str::from_utf8(bytes).map_err(|e| MetadataError::Json(format!("Malformed input: {e}")))?;
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => Ok(ResourceMetadata { sections: Some(map), built_in: None }),
            Ok(other) => Err(MetadataError::Json(format!(
                "Expected a com.google.gson.JsonObject but was {}",
                match other {
                    Value::Array(_) => "com.google.gson.JsonArray",
                    Value::Null => "com.google.gson.JsonNull",
                    _ => "com.google.gson.JsonPrimitive",
                }
            ))),
            Err(e) => Err(MetadataError::Json(e.to_string())),
        }
    }

    pub fn with_built_in(mut self, built_in: Map<String, Value>) -> ResourceMetadata {
        self.built_in = Some(built_in);
        self
    }

    /// The raw JSON of section `name` (the file's, else the built-in one).
    pub fn section(&self, name: &str) -> Option<&Value> {
        self.sections.as_ref().and_then(|m| m.get(name)).or_else(|| self.built_in.as_ref().and_then(|m| m.get(name)))
    }

    /// Parse the section `name` of this metadata with `codec`; a codec failure is a `JsonParseException`.
    pub fn get<T>(&self, name: &str, codec: impl FnOnce(&Value) -> Result<T, String>) -> Result<Option<T>, MetadataError> {
        self.section(name).map(|v| codec(v).map_err(MetadataError::Json)).transpose()
    }
}

/// `PackResources` (+ `PackMetadataResources`).
pub trait PackResources: Send + Sync {
    fn location(&self) -> &PackLocationInfo;

    fn pack_id(&self) -> &str {
        &self.location().id
    }

    fn known_pack_info(&self) -> Option<&KnownPack> {
        self.location().known_pack_info.as_ref()
    }

    /// `getRootResource(path...)`.
    fn get_root_resource(&self, path: &[&str]) -> Option<IoSupplier>;

    /// `getResource(type, location)`.
    fn get_resource(&self, ty: PackType, location: &Identifier) -> Option<IoSupplier>;

    /// `listResources(type, namespace, directory, output)`.
    fn list_resources(&self, ty: PackType, namespace: &str, directory: &str, output: &mut dyn FnMut(Identifier, IoSupplier));

    fn get_namespaces(&self, ty: PackType) -> HashSet<String>;

    /// `AbstractPackMetadataResources.loadMetadata` (cached on success only, like vanilla).
    fn metadata(&self) -> Result<Arc<ResourceMetadata>, MetadataError>;

    fn close(&self) {}
}

/// `AbstractPackMetadataResources.loadMetadata`.
fn load_metadata(pack: &dyn PackResources, built_in: Option<&Map<String, Value>>) -> Result<ResourceMetadata, MetadataError> {
    let mut meta = match pack.get_root_resource(&["pack.mcmeta"]) {
        None => ResourceMetadata::EMPTY,
        Some(supplier) => {
            let bytes = supplier().map_err(|e| MetadataError::Io(e.to_string()))?;
            ResourceMetadata::from_json_bytes(&bytes)?
        }
    };
    if let Some(b) = built_in {
        meta = meta.with_built_in(b.clone());
    }
    Ok(meta)
}

fn cached_metadata(cache: &Mutex<Option<Arc<ResourceMetadata>>>, load: impl FnOnce() -> Result<ResourceMetadata, MetadataError>) -> Result<Arc<ResourceMetadata>, MetadataError> {
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = guard.as_ref() {
        return Ok(m.clone());
    }
    let m = Arc::new(load()?);
    *guard = Some(m.clone());
    Ok(m)
}

fn file_supplier(path: PathBuf) -> IoSupplier {
    Arc::new(move || fs::read(&path))
}

/// `FileUtil.STRICT_PATH_SEGMENT_CHECK` (`[-._a-z0-9]+`).
fn allowed_segment(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|b| matches!(b, b'-' | b'.' | b'_' | b'a'..=b'z' | b'0'..=b'9'))
}

/// `FileUtil.decomposePath`.
pub fn decompose_path(path: &str) -> Result<Vec<&str>, String> {
    if !path.contains('/') {
        return match path {
            "" | "." | ".." => Err(format!("Invalid path '{path}'")),
            p if !allowed_segment(p) => Err(format!("Invalid path '{path}'")),
            p => Ok(vec![p]),
        };
    }
    let mut out = Vec::new();
    for segment in path.split('/') {
        if matches!(segment, "" | "." | "..") || !allowed_segment(segment) {
            return Err(format!("Invalid segment '{segment}' in path '{path}'"));
        }
        out.push(segment);
    }
    Ok(out)
}

/// `FileUtil.isValidPathSegment`.
fn is_valid_path_segment(s: &str) -> bool {
    s != ".." && s != "." && allowed_segment(s)
}

fn namespace_valid(ns: &str) -> bool {
    ident::is_valid_namespace(ns)
}

// ---- PathPackResources -------------------------------------------------------------------------------------------

/// `PathPackResources`: a pack in a directory.
pub struct PathPackResources {
    location: PackLocationInfo,
    root: PathBuf,
    meta: Mutex<Option<Arc<ResourceMetadata>>>,
}

impl PathPackResources {
    pub fn new(location: PackLocationInfo, root: PathBuf) -> PathPackResources {
        PathPackResources { location, root, meta: Mutex::new(None) }
    }

    fn top_pack_dir(&self, ty: PackType) -> PathBuf {
        self.root.join(ty.directory())
    }

    /// `PathPackResources.listPath`.
    fn list_path(namespace: &str, top_dir: &Path, prefix: &[&str], output: &mut dyn FnMut(Identifier, IoSupplier)) {
        let mut target = top_dir.to_path_buf();
        target.extend(prefix);
        fn walk(dir: &Path, base: &Path, namespace: &str, output: &mut dyn FnMut(Identifier, IoSupplier)) -> io::Result<()> {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                // `Files.find` does not follow links: a symlink is neither a regular file nor a directory to walk
                let meta = fs::symlink_metadata(&path)?;
                if meta.is_dir() {
                    walk(&path, base, namespace, output)?;
                } else if meta.is_file() {
                    let rel = path.strip_prefix(base).unwrap_or(&path);
                    let resource_path = rel.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join("/");
                    match Identifier::try_build(namespace, &resource_path) {
                        Some(id) => output(id, file_supplier(path)),
                        None => log::error!("Invalid path in pack: {namespace}:{resource_path}, ignoring"),
                    }
                }
            }
            Ok(())
        }
        match fs::metadata(&target) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return, // NotDirectoryException
            Err(e) if e.kind() == io::ErrorKind::NotFound => return,
            Err(e) => return log::error!("Failed to list path {}: {e}", target.display()),
        }
        if let Err(e) = walk(&target, top_dir, namespace, output) {
            log::error!("Failed to list path {}: {e}", target.display());
        }
    }

    /// `PathPackResources.getNamespaces(rootDir)`.
    pub fn namespaces_in(root_dir: &Path) -> HashSet<String> {
        let mut namespaces = HashSet::new();
        let entries = match fs::read_dir(root_dir) {
            Ok(e) => e,
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => return namespaces,
            Err(e) => {
                log::error!("Failed to list path {}: {e}", root_dir.display());
                return namespaces;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !path.is_dir() {
                log::warn!("Non-directory entry {} found in namespace directory, rejecting", path.display());
            } else if namespace_valid(&name) {
                namespaces.insert(name);
            } else {
                log::warn!("Non {} character in namespace {} in pack directory {}, ignoring", "[a-z0-9_.-]", name, root_dir.display());
            }
        }
        namespaces
    }
}

impl PackResources for PathPackResources {
    fn location(&self) -> &PackLocationInfo {
        &self.location
    }

    fn get_root_resource(&self, path: &[&str]) -> Option<IoSupplier> {
        if path.is_empty() || !path.iter().all(|s| is_valid_path_segment(s)) {
            // `FileUtil.validatePath` throws IllegalArgumentException; callers only pass literals
            return None;
        }
        let mut p = self.root.clone();
        p.extend(path);
        p.exists().then(|| file_supplier(p))
    }

    fn get_resource(&self, ty: PackType, location: &Identifier) -> Option<IoSupplier> {
        let segments = match decompose_path(location.path()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Invalid path {location}: {e}");
                return None;
            }
        };
        let mut p = self.top_pack_dir(ty).join(location.namespace());
        p.extend(segments);
        p.exists().then(|| file_supplier(p))
    }

    fn list_resources(&self, ty: PackType, namespace: &str, directory: &str, output: &mut dyn FnMut(Identifier, IoSupplier)) {
        match decompose_path(directory) {
            Ok(segments) => PathPackResources::list_path(namespace, &self.top_pack_dir(ty).join(namespace), &segments, output),
            Err(e) => log::error!("Invalid path {directory}: {e}"),
        }
    }

    fn get_namespaces(&self, ty: PackType) -> HashSet<String> {
        PathPackResources::namespaces_in(&self.top_pack_dir(ty))
    }

    fn metadata(&self) -> Result<Arc<ResourceMetadata>, MetadataError> {
        cached_metadata(&self.meta, || load_metadata(self, None))
    }
}

/// `PathPackResources.PathResourcesSupplier`.
pub struct PathResourcesSupplier {
    pub content: PathBuf,
}

impl ResourcesSupplier for PathResourcesSupplier {
    fn open_metadata(&self, location: &PackLocationInfo) -> Arc<dyn PackResources> {
        Arc::new(PathPackResources::new(location.clone(), self.content.clone()))
    }

    fn open_resources(&self, location: &PackLocationInfo, metadata: &PackMetadata) -> Vec<Arc<dyn PackResources>> {
        let primary: Arc<dyn PackResources> = Arc::new(PathPackResources::new(location.clone(), self.content.clone()));
        if metadata.overlays.is_empty() {
            return vec![primary];
        }
        let overlays = metadata
            .overlays
            .iter()
            .map(|o| Arc::new(PathPackResources::new(location.clone(), self.content.join(o))) as Arc<dyn PackResources>)
            .collect();
        vec![Arc::new(OverlayedPackResources::new(primary, overlays))]
    }
}

// ---- FilePackResources -------------------------------------------------------------------------------------------

/// `FilePackResources.SharedZipFileAccess`.
struct SharedZip {
    file: PathBuf,
    state: Mutex<ZipState>,
}

enum ZipState {
    Unopened,
    Open(ZipArchive<File>),
    Failed,
}

impl SharedZip {
    fn new(file: PathBuf) -> Arc<SharedZip> {
        Arc::new(SharedZip { file, state: Mutex::new(ZipState::Unopened) })
    }

    /// Run `f` on the opened archive (`getOrCreateZipFile`); `None` when the zip could not be opened.
    fn with<R>(&self, f: impl FnOnce(&mut ZipArchive<File>) -> R) -> Option<R> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*state, ZipState::Unopened) {
            *state = match File::open(&self.file).map_err(|e| e.to_string()).and_then(|f| ZipArchive::new(f).map_err(|e| e.to_string())) {
                Ok(zip) => ZipState::Open(zip),
                Err(e) => {
                    log::error!("Failed to open pack {}\n{}", self.file.display(), super::throwable("java.util.zip.ZipException", &e));
                    ZipState::Failed
                }
            };
        }
        match &mut *state {
            ZipState::Open(zip) => Some(f(zip)),
            _ => None,
        }
    }

    fn close(&self) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = ZipState::Unopened;
    }

    fn entry_supplier(self: &Arc<SharedZip>, name: String) -> IoSupplier {
        let this = self.clone();
        Arc::new(move || {
            this.with(|zip| {
                let mut entry = zip.by_name(&name).map_err(|e| io::Error::other(e.to_string()))?;
                let mut out = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut out)?;
                Ok(out)
            })
            .unwrap_or_else(|| Err(io::Error::other("zip file is not open")))
        })
    }
}

/// `FilePackResources`: a pack in a `.zip` (optionally restricted to an overlay directory `prefix`).
pub struct FilePackResources {
    location: PackLocationInfo,
    zip: Arc<SharedZip>,
    prefix: String,
    meta: Mutex<Option<Arc<ResourceMetadata>>>,
}

impl FilePackResources {
    fn new(location: PackLocationInfo, zip: Arc<SharedZip>, prefix: String) -> FilePackResources {
        FilePackResources { location, zip, prefix, meta: Mutex::new(None) }
    }

    fn add_prefix(&self, path: &str) -> String {
        if self.prefix.is_empty() { path.to_owned() } else { format!("{}/{path}", self.prefix) }
    }

    fn resource(&self, path: &str) -> Option<IoSupplier> {
        let name = self.add_prefix(path);
        // `ZipFile.getEntry` also finds a directory entry stored as `name/`
        let found = self.zip.with(|z| {
            if z.by_name(&name).is_ok() {
                Some(name.clone())
            } else {
                let dir = format!("{name}/");
                z.by_name(&dir).is_ok().then_some(dir)
            }
        })??;
        Some(self.zip.entry_supplier(found))
    }

    /// `FilePackResources.extractNamespace`.
    pub fn extract_namespace(prefix: &str, name: &str) -> String {
        match name.strip_prefix(prefix) {
            None => String::new(),
            Some(rest) => match rest.find('/') {
                Some(i) => rest[..i].to_owned(),
                None => rest.to_owned(),
            },
        }
    }
}

impl PackResources for FilePackResources {
    fn location(&self) -> &PackLocationInfo {
        &self.location
    }

    fn get_root_resource(&self, path: &[&str]) -> Option<IoSupplier> {
        self.resource(&path.join("/"))
    }

    fn get_resource(&self, ty: PackType, location: &Identifier) -> Option<IoSupplier> {
        self.resource(&format!("{}/{}/{}", ty.directory(), location.namespace(), location.path()))
    }

    fn list_resources(&self, ty: PackType, namespace: &str, directory: &str, output: &mut dyn FnMut(Identifier, IoSupplier)) {
        let root = self.add_prefix(&format!("{}/{namespace}/", ty.directory()));
        let prefix = format!("{root}{directory}/");
        let names: Vec<String> = self.zip.with(|z| z.file_names().filter_map(Result::ok).filter(|n| !n.ends_with('/') && n.starts_with(&prefix)).map(Cow::into_owned).collect()).unwrap_or_default();
        for name in names {
            let path = &name[root.len()..];
            match Identifier::try_build(namespace, path) {
                Some(id) => output(id, self.zip.entry_supplier(name.clone())),
                None => log::warn!("Invalid path in datapack: {namespace}:{path}, ignoring"),
            }
        }
    }

    fn get_namespaces(&self, ty: PackType) -> HashSet<String> {
        let type_prefix = self.add_prefix(&format!("{}/", ty.directory()));
        let names: Vec<String> = self.zip.with(|z| z.file_names().filter_map(Result::ok).map(Cow::into_owned).collect()).unwrap_or_default();
        let mut namespaces = HashSet::new();
        for name in names {
            let ns = FilePackResources::extract_namespace(&type_prefix, &name);
            if !ns.is_empty() {
                if namespace_valid(&ns) {
                    namespaces.insert(ns);
                } else {
                    log::warn!("Non {} character in namespace {} in pack {}, ignoring", "[a-z0-9_.-]", ns, self.zip.file.display());
                }
            }
        }
        namespaces
    }

    fn metadata(&self) -> Result<Arc<ResourceMetadata>, MetadataError> {
        cached_metadata(&self.meta, || load_metadata(self, None))
    }

    fn close(&self) {
        self.zip.close();
    }
}

/// `FilePackResources.FileResourcesSupplier`.
pub struct FileResourcesSupplier {
    pub content: PathBuf,
}

impl ResourcesSupplier for FileResourcesSupplier {
    fn open_metadata(&self, location: &PackLocationInfo) -> Arc<dyn PackResources> {
        Arc::new(FilePackResources::new(location.clone(), SharedZip::new(self.content.clone()), String::new()))
    }

    fn open_resources(&self, location: &PackLocationInfo, metadata: &PackMetadata) -> Vec<Arc<dyn PackResources>> {
        let access = SharedZip::new(self.content.clone());
        let primary: Arc<dyn PackResources> = Arc::new(FilePackResources::new(location.clone(), access.clone(), String::new()));
        if metadata.overlays.is_empty() {
            return vec![primary];
        }
        let overlays = metadata
            .overlays
            .iter()
            .map(|o| Arc::new(FilePackResources::new(location.clone(), access.clone(), o.clone())) as Arc<dyn PackResources>)
            .collect();
        vec![Arc::new(OverlayedPackResources::new(primary, overlays))]
    }
}

// ---- OverlayedPackResources --------------------------------------------------------------------------------------

/// `OverlayedPackResources`: a primary pack plus overlay directories (later overlays win).
pub struct OverlayedPackResources {
    primary: Arc<dyn PackResources>,
    /// `packResourcesStack`: overlays reversed, then the primary pack.
    stack: Vec<Arc<dyn PackResources>>,
}

impl OverlayedPackResources {
    pub fn new(primary: Arc<dyn PackResources>, overlays: Vec<Arc<dyn PackResources>>) -> OverlayedPackResources {
        let mut stack: Vec<_> = overlays.into_iter().rev().collect();
        stack.push(primary.clone());
        OverlayedPackResources { primary, stack }
    }
}

impl PackResources for OverlayedPackResources {
    fn location(&self) -> &PackLocationInfo {
        self.primary.location()
    }

    fn get_root_resource(&self, path: &[&str]) -> Option<IoSupplier> {
        self.primary.get_root_resource(path)
    }

    fn get_resource(&self, ty: PackType, location: &Identifier) -> Option<IoSupplier> {
        self.stack.iter().find_map(|p| p.get_resource(ty, location))
    }

    fn list_resources(&self, ty: PackType, namespace: &str, directory: &str, output: &mut dyn FnMut(Identifier, IoSupplier)) {
        let mut result: HashMap<Identifier, IoSupplier> = HashMap::new();
        for pack in &self.stack {
            pack.list_resources(ty, namespace, directory, &mut |id, s| {
                result.entry(id).or_insert(s);
            });
        }
        for (id, s) in result {
            output(id, s);
        }
    }

    fn get_namespaces(&self, ty: PackType) -> HashSet<String> {
        self.stack.iter().flat_map(|p| p.get_namespaces(ty)).collect()
    }

    fn metadata(&self) -> Result<Arc<ResourceMetadata>, MetadataError> {
        self.primary.metadata()
    }

    fn close(&self) {
        self.stack.iter().for_each(|p| p.close());
    }
}

// ---- EmbeddedPackResources ---------------------------------------------------------------------------------------

/// A pack served from the embedded vanilla data pack (`mc_data::datapack`): the built-in `vanilla` pack (root `""`,
/// exposing only the `minecraft` namespace) or a feature pack under `data/minecraft/datapacks/<id>/`.
pub struct EmbeddedPackResources {
    location: PackLocationInfo,
    /// Path prefix inside the embedded archive, empty or ending in `/`.
    root: String,
    /// `exposeNamespace` of `VanillaPackResourcesBuilder`; `None` = discover namespaces from the files.
    exposed_namespaces: Option<HashSet<String>>,
    built_in_metadata: Option<Map<String, Value>>,
    meta: Mutex<Option<Arc<ResourceMetadata>>>,
}

impl EmbeddedPackResources {
    pub fn new(location: PackLocationInfo, root: String, exposed_namespaces: Option<HashSet<String>>, built_in_metadata: Option<Map<String, Value>>) -> Self {
        EmbeddedPackResources { location, root, exposed_namespaces, built_in_metadata, meta: Mutex::new(None) }
    }

    fn supplier(path: String) -> IoSupplier {
        Arc::new(move || mc_data::datapack::read(&path).map(|c| c.into_owned()).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone())))
    }
}

impl PackResources for EmbeddedPackResources {
    fn location(&self) -> &PackLocationInfo {
        &self.location
    }

    fn get_root_resource(&self, path: &[&str]) -> Option<IoSupplier> {
        if path.is_empty() || !path.iter().all(|s| is_valid_path_segment(s)) {
            return None;
        }
        let full = format!("{}{}", self.root, path.join("/"));
        mc_data::datapack::exists(&full).then(|| EmbeddedPackResources::supplier(full))
    }

    fn get_resource(&self, ty: PackType, location: &Identifier) -> Option<IoSupplier> {
        if let Err(e) = decompose_path(location.path()) {
            log::error!("Invalid path {location}: {e}");
            return None;
        }
        let full = format!("{}{}/{}/{}", self.root, ty.directory(), location.namespace(), location.path());
        mc_data::datapack::exists(&full).then(|| EmbeddedPackResources::supplier(full))
    }

    fn list_resources(&self, ty: PackType, namespace: &str, directory: &str, output: &mut dyn FnMut(Identifier, IoSupplier)) {
        if let Err(e) = decompose_path(directory) {
            return log::error!("Invalid path {directory}: {e}");
        }
        let ns_root = format!("{}{}/{namespace}/", self.root, ty.directory());
        let prefix = format!("{ns_root}{directory}/");
        for path in mc_data::datapack::paths().filter(|p| p.starts_with(&prefix)) {
            let resource_path = &path[ns_root.len()..];
            match Identifier::try_build(namespace, resource_path) {
                Some(id) => output(id, EmbeddedPackResources::supplier(path.to_owned())),
                None => log::error!("Invalid path in pack: {namespace}:{resource_path}, ignoring"),
            }
        }
    }

    fn get_namespaces(&self, ty: PackType) -> HashSet<String> {
        if let Some(ns) = &self.exposed_namespaces {
            return ns.clone();
        }
        let prefix = format!("{}{}/", self.root, ty.directory());
        mc_data::datapack::paths()
            .filter_map(|p| p.strip_prefix(&prefix))
            .filter_map(|rest| rest.split_once('/').map(|(ns, _)| ns))
            .filter(|ns| namespace_valid(ns))
            .map(str::to_owned)
            .collect()
    }

    fn metadata(&self) -> Result<Arc<ResourceMetadata>, MetadataError> {
        cached_metadata(&self.meta, || load_metadata(self, self.built_in_metadata.as_ref()))
    }
}

/// `ResourcesSupplier` of an embedded pack (`VanillaPackResources.asResourcesSupplier` / feature packs found in the jar).
pub struct EmbeddedResourcesSupplier {
    pub root: String,
    pub exposed_namespaces: Option<HashSet<String>>,
    pub built_in_metadata: Option<Map<String, Value>>,
}

impl EmbeddedResourcesSupplier {
    fn open(&self, location: &PackLocationInfo) -> Arc<dyn PackResources> {
        Arc::new(EmbeddedPackResources::new(location.clone(), self.root.clone(), self.exposed_namespaces.clone(), self.built_in_metadata.clone()))
    }
}

impl ResourcesSupplier for EmbeddedResourcesSupplier {
    fn open_metadata(&self, location: &PackLocationInfo) -> Arc<dyn PackResources> {
        self.open(location)
    }

    fn open_resources(&self, location: &PackLocationInfo, _metadata: &PackMetadata) -> Vec<Arc<dyn PackResources>> {
        vec![self.open(location)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose() {
        assert_eq!(decompose_path("a/b.json").unwrap(), vec!["a", "b.json"]);
        assert_eq!(decompose_path("..").unwrap_err(), "Invalid path '..'");
        assert_eq!(decompose_path("a//b").unwrap_err(), "Invalid segment '' in path 'a//b'");
        assert_eq!(decompose_path("a/B").unwrap_err(), "Invalid segment 'B' in path 'a/B'");
    }

    #[test]
    fn zip_namespace_extraction() {
        assert_eq!(FilePackResources::extract_namespace("data/", "data/foo/x.json"), "foo");
        assert_eq!(FilePackResources::extract_namespace("data/", "data/foo"), "foo");
        assert_eq!(FilePackResources::extract_namespace("data/", "assets/foo/x"), "");
    }
}
