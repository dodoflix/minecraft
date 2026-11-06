//! `StructureTemplateManager`: caches the templates found in the world's `generated` directory and the data pack
//! stack (`structure/<id>.nbt`), including the misses (`Optional.empty`) like vanilla's `ConcurrentHashMap`.
//!
//! Templates are immutable once loaded (`Arc<StructureTemplate>`); the structure-block editing flow of vanilla
//! (`getOrCreate` + mutate + `save`) is `put` of a freshly filled template followed by [`StructureTemplateManager::save`].
//! The development-only test structure sources (`StructureUtils.testStructures*Dir`) are not supported.

use super::loader::{
    DirectoryTemplateSource, ResourceManagerTemplateSource, TemplatePathFactory, TemplateSource,
};
use super::structure_template::StructureTemplate;
use crate::server::packs::{FileToIdConverter, ResourceManager};
use mc_core::ident::Identifier;
use mc_nbt::CompoundTag;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const STRUCTURE_DIRECTORY_NAME: &str = "structure";
const STRUCTURE_FILE_EXTENSION: &str = ".nbt";

/// `WORLD_STRUCTURE_LISTER` / `RESOURCE_STRUCTURE_LISTER`.
fn structure_lister() -> FileToIdConverter {
    FileToIdConverter::new(STRUCTURE_DIRECTORY_NAME, STRUCTURE_FILE_EXTENSION)
}

type Cell = Arc<OnceLock<Option<Arc<StructureTemplate>>>>;

pub struct StructureTemplateManager {
    /// `structureRepository`: one cell per id so that concurrent loads of different ids do not serialize while a
    /// given id is loaded exactly once (`ConcurrentHashMap.computeIfAbsent`).
    structure_repository: Mutex<HashMap<Identifier, Cell>>,
    resource_manager_source: Arc<ResourceManagerTemplateSource>,
    sources: Vec<Arc<dyn TemplateSource>>,
    world_templates: TemplatePathFactory,
}

impl StructureTemplateManager {
    /// `new StructureTemplateManager(resourceManager, storage, fixerUpper, blockLookup)`; `generated_dir` is the
    /// world's `generated` directory (`LevelResource.GENERATED_DIR`).
    pub fn new(
        resource_manager: Arc<dyn ResourceManager>,
        generated_dir: impl Into<PathBuf>,
    ) -> StructureTemplateManager {
        let generated_dir: PathBuf = generated_dir.into();
        let resource_manager_source = Arc::new(ResourceManagerTemplateSource::new(
            resource_manager,
            structure_lister(),
        ));
        let sources: Vec<Arc<dyn TemplateSource>> = vec![
            Arc::new(DirectoryTemplateSource::new(
                &generated_dir,
                structure_lister(),
            )),
            resource_manager_source.clone(),
        ];
        StructureTemplateManager {
            structure_repository: Mutex::new(HashMap::new()),
            resource_manager_source,
            sources,
            world_templates: TemplatePathFactory::new(generated_dir),
        }
    }

    fn cell(&self, id: &Identifier) -> Cell {
        self.structure_repository
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id.clone())
            .or_default()
            .clone()
    }

    /// `getOrCreate(id)`: the cached template, else a new empty one that is cached.
    pub fn get_or_create(&self, id: &Identifier) -> Arc<StructureTemplate> {
        if let Some(template) = self.get(id) {
            return template;
        }
        let template = Arc::new(StructureTemplate::new());
        self.put(id, template.clone());
        template
    }

    /// `structureRepository.put(id, Optional.of(template))`.
    pub fn put(&self, id: &Identifier, template: Arc<StructureTemplate>) {
        let cell = Cell::default();
        let _ = cell.set(Some(template));
        self.structure_repository
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), cell);
    }

    /// `get(id)`: `computeIfAbsent(id, tryLoad)`.
    pub fn get(&self, id: &Identifier) -> Option<Arc<StructureTemplate>> {
        self.cell(id).get_or_init(|| self.try_load(id)).clone()
    }

    /// `listTemplates()`: the distinct ids of all sources.
    pub fn list_templates(&self) -> Vec<Identifier> {
        let mut seen = std::collections::HashSet::new();
        self.sources
            .iter()
            .flat_map(|s| s.list())
            .filter(|id| seen.insert(id.clone()))
            .collect()
    }

    fn try_load(&self, id: &Identifier) -> Option<Arc<StructureTemplate>> {
        self.sources
            .iter()
            .find_map(|source| source.load(id))
            .map(Arc::new)
    }

    /// `onResourceManagerReload(resourceManager)`.
    pub fn on_resource_manager_reload(&self, resource_manager: Arc<dyn ResourceManager>) {
        self.resource_manager_source
            .set_resource_manager(resource_manager);
        self.structure_repository
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// `save(id)`: writes the cached template to the world's `generated` directory.
    pub fn save(&self, id: &Identifier) -> bool {
        let Some(template) = self
            .structure_repository
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .and_then(|c| c.get().cloned().flatten())
        else {
            return false;
        };
        let file = match self
            .world_templates
            .create_and_validate_path_to_structure_with(id, &structure_lister())
        {
            Ok(file) => file,
            Err(e) => {
                log::warn!("Failed to save structure file {id}: {e}");
                return false;
            }
        };
        match save_to(&file, &template) {
            Ok(saved) => saved,
            Err(e) => {
                log::warn!(
                    "Failed to save structure file {id} to {}: {e}",
                    file.display()
                );
                false
            }
        }
    }

    pub fn world_templates(&self) -> &TemplatePathFactory {
        &self.world_templates
    }

    /// `remove(id)`.
    pub fn remove(&self, id: &Identifier) {
        self.structure_repository
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }
}

/// `StructureTemplateManager.save(file, template, asText = false)`.
pub fn save_to(file: &Path, template: &StructureTemplate) -> Result<bool, String> {
    let Some(parent) = file.parent() else {
        return Ok(false);
    };
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tag = template.save(CompoundTag::new());
    mc_nbt::nbt_io::write_compressed_file(&tag, file).map_err(|e| e.to_string())?;
    Ok(true)
}
