//! `DirectoryTemplateSource`: templates from a directory laid out as `<dir>/<namespace>/structure/<path>.nbt` (the
//! world's `generated` directory).

use super::template_path_factory::TemplatePathFactory;
use super::template_source::{TemplateSource, load_from_bytes};
use crate::server::packs::FileToIdConverter;
use crate::world::level::levelgen::structure::templatesystem::structure_template::StructureTemplate;
use mc_core::ident::Identifier;
use std::path::{Path, PathBuf};

pub struct DirectoryTemplateSource {
    source_dir: PathBuf,
    file_to_id_converter: FileToIdConverter,
}

impl DirectoryTemplateSource {
    pub fn new(source_dir: impl Into<PathBuf>, file_to_id_converter: FileToIdConverter) -> Self {
        DirectoryTemplateSource {
            source_dir: source_dir.into(),
            file_to_id_converter,
        }
    }
}

impl TemplateSource for DirectoryTemplateSource {
    fn load(&self, id: &Identifier) -> Option<StructureTemplate> {
        if !self.source_dir.is_dir() {
            return None;
        }
        let path = TemplatePathFactory::new(&self.source_dir)
            .create_and_validate_path_to_structure(&self.file_to_id_converter.id_to_file(id))
            .ok()?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::error!(
                    "Couldn't load structure from {}:{id}: {e}",
                    self.source_dir.display()
                );
                return None;
            }
        };
        load_from_bytes(&bytes, |e| {
            log::error!(
                "Couldn't load structure from {}:{id}: {e}",
                self.source_dir.display()
            )
        })
    }

    fn list(&self) -> Vec<Identifier> {
        let mut result = Vec::new();
        let Ok(namespaces) = std::fs::read_dir(&self.source_dir) else {
            return result;
        };
        let mut namespaces: Vec<_> = namespaces
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .collect();
        namespaces.sort_by_key(|e| e.file_name());
        for namespace in namespaces {
            let namespace_name = namespace.file_name().to_string_lossy().into_owned();
            let root = namespace.path().join(self.file_to_id_converter.prefix());
            collect(
                &root,
                &root,
                &namespace_name,
                &self.file_to_id_converter,
                &mut result,
            );
        }
        result
    }
}

fn collect(
    root: &Path,
    dir: &Path,
    namespace: &str,
    converter: &FileToIdConverter,
    out: &mut Vec<Identifier>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, namespace, converter, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            let relative = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            // `PathPackResources.listResources` skips names that are not valid resource paths
            if let Some(file) =
                Identifier::try_build(namespace, &format!("{}/{relative}", converter.prefix()))
                && converter.extension_matches(&file)
            {
                out.push(converter.file_to_id(&file));
            }
        }
    }
}
