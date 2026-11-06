//! `ResourceManagerTemplateSource`: templates from the data pack stack (`data/<ns>/structure/<path>.nbt`).

use super::template_source::{TemplateSource, load_from_bytes};
use crate::server::packs::{FileToIdConverter, ResourceManager};
use crate::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate;
use mc_core::ident::Identifier;
use std::sync::{Arc, RwLock};

pub struct ResourceManagerTemplateSource {
    resource_manager: RwLock<Arc<dyn ResourceManager>>,
    file_to_id_converter: FileToIdConverter,
}

impl ResourceManagerTemplateSource {
    pub fn new(
        resource_manager: Arc<dyn ResourceManager>,
        file_to_id_converter: FileToIdConverter,
    ) -> Self {
        ResourceManagerTemplateSource {
            resource_manager: RwLock::new(resource_manager),
            file_to_id_converter,
        }
    }

    /// `setResourceManager(resourceManager)`.
    pub fn set_resource_manager(&self, resource_manager: Arc<dyn ResourceManager>) {
        *self
            .resource_manager
            .write()
            .unwrap_or_else(|e| e.into_inner()) = resource_manager;
    }

    fn manager(&self) -> Arc<dyn ResourceManager> {
        self.resource_manager
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl TemplateSource for ResourceManagerTemplateSource {
    fn load(&self, id: &Identifier) -> Option<StructureTemplate> {
        let file = self.file_to_id_converter.id_to_file(id);
        // `FileNotFoundException` (no such resource) is a silent miss
        let bytes = match self.manager().get_resource(&file)?.open() {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::error!("Couldn't load structure {id}: {e}");
                return None;
            }
        };
        load_from_bytes(&bytes, |e| log::error!("Couldn't load structure {id}: {e}"))
    }

    fn list(&self) -> Vec<Identifier> {
        self.file_to_id_converter
            .list_matching_resources(&*self.manager())
            .keys()
            .map(|file| self.file_to_id_converter.file_to_id(file))
            .collect()
    }
}
