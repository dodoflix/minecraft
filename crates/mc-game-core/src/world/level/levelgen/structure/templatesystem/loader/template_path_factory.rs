//! `TemplatePathFactory`: file locations of the templates a world saves.

use crate::server::packs::FileToIdConverter;
use mc_core::ident::Identifier;
use std::path::{Path, PathBuf};

pub struct TemplatePathFactory {
    source_dir: PathBuf,
}

impl TemplatePathFactory {
    pub fn new(source_dir: impl Into<PathBuf>) -> TemplatePathFactory {
        TemplatePathFactory {
            source_dir: source_dir.into(),
        }
    }

    pub fn source_dir(&self) -> &Path {
        &self.source_dir
    }

    /// `createAndValidatePathToStructure(id, converter)`.
    pub fn create_and_validate_path_to_structure_with(
        &self,
        id: &Identifier,
        converter: &FileToIdConverter,
    ) -> Result<PathBuf, String> {
        self.create_and_validate_path_to_structure(&converter.id_to_file(id))
    }

    /// `createAndValidatePathToStructure(resourceLocation)`: `<dir>/<namespace>/<path>`; `.`/`..`/empty path segments
    /// are rejected (`FileUtil.decomposePath`) as are non portable names.
    pub fn create_and_validate_path_to_structure(
        &self,
        location: &Identifier,
    ) -> Result<PathBuf, String> {
        let mut path = self.source_dir.join(location.namespace());
        for part in location.path().split('/') {
            if !is_path_part_portable(part) {
                return Err(format!("Resource path '{location}' is not portable"));
            }
            path.push(part);
        }
        Ok(path)
    }
}

/// `FileUtil.isPathPartPortable`: no traversal, no Windows reserved device names, no trailing dots.
fn is_path_part_portable(part: &str) -> bool {
    if part.is_empty() || part == "." || part == ".." || part.ends_with('.') {
        return false;
    }
    let stem = part.split('.').next().unwrap_or(part).to_ascii_lowercase();
    let reserved = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    !reserved
}
