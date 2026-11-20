//! `FileRelation`: the folders (or files) of the world a file fix works on, found from a base path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::cow_tree::CowTree;

type Resolver = dyn Fn(&CowTree, &Path) -> Vec<PathBuf> + Send + Sync;

/// A function from a base path of the [`CowTree`] to the paths it relates to.
#[derive(Clone)]
pub struct FileRelation(Arc<Resolver>);

impl FileRelation {
    pub fn new(
        resolver: impl Fn(&CowTree, &Path) -> Vec<PathBuf> + Send + Sync + 'static,
    ) -> FileRelation {
        FileRelation(Arc::new(resolver))
    }

    /// `getPaths(basePath)`.
    pub fn paths(&self, tree: &CowTree, base_path: &Path) -> Vec<PathBuf> {
        (self.0)(tree, base_path)
    }

    /// `ORIGIN`: the base path itself.
    pub fn origin() -> FileRelation {
        FileRelation::new(|_, base| vec![base.to_path_buf()])
    }

    fn below(relative: &'static str) -> FileRelation {
        FileRelation::new(move |_, base| vec![base.join(relative)])
    }

    /// `REGION`.
    pub fn region() -> FileRelation {
        Self::below("region")
    }

    /// `DATA`.
    pub fn data() -> FileRelation {
        Self::below("data")
    }

    /// `PLAYER_DATA`.
    pub fn player_data() -> FileRelation {
        Self::below("players/data")
    }

    /// `DIMENSIONS`: every `dimensions/<namespace>/<name>` folder, else the three default dimensions.
    pub fn dimensions() -> FileRelation {
        FileRelation::new(discover_dimensions)
    }

    /// `DIMENSIONS_DATA`.
    pub fn dimensions_data() -> FileRelation {
        Self::dimensions().resolve(&Self::data())
    }

    /// `GENERATED_NAMESPACES`: the namespace folders of `generated`.
    pub fn generated_namespaces() -> FileRelation {
        FileRelation::origin().resolve(&FileRelation::new(|tree, base| {
            directories_in_path(tree, &base.join("generated"))
        }))
    }

    /// `OLD_OVERWORLD`.
    pub fn old_overworld() -> FileRelation {
        Self::origin()
    }

    /// `OLD_NETHER`.
    pub fn old_nether() -> FileRelation {
        Self::origin().resolve(&Self::below("DIM-1"))
    }

    /// `OLD_END`.
    pub fn old_end() -> FileRelation {
        Self::origin().resolve(&Self::below("DIM1"))
    }

    /// `forDataFileInDimension(dimension, fileName)`.
    pub fn for_data_file_in_dimension(dimension: &str, file_name: &str) -> FileRelation {
        let relative = format!("dimensions/minecraft/{dimension}/data/{file_name}");
        FileRelation::new(move |_, base| vec![base.join(&relative)])
    }

    /// `forFile(fileName)`: `file_name` below each of this relation's paths.
    pub fn for_file(&self, file_name: &str) -> FileRelation {
        let file_name = file_name.to_owned();
        self.resolve(&FileRelation::new(move |_, base| {
            vec![base.join(&file_name)]
        }))
    }

    /// `resolve(other)`: `other` applied to each of this relation's paths.
    pub fn resolve(&self, other: &FileRelation) -> FileRelation {
        let (first, second) = (self.clone(), other.clone());
        FileRelation::new(move |tree, base| {
            first
                .paths(tree, base)
                .iter()
                .flat_map(|path| second.paths(tree, path))
                .collect()
        })
    }

    /// `join(relations)`: the paths of all of them, each once.
    pub fn join(relations: &[FileRelation]) -> FileRelation {
        let relations = relations.to_vec();
        FileRelation::new(move |tree, base| {
            let mut paths: Vec<PathBuf> = Vec::new();
            for relation in &relations {
                for path in relation.paths(tree, base) {
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
            }
            paths
        })
    }
}

fn default_dimensions(base: &Path) -> Vec<PathBuf> {
    ["overworld", "the_nether", "the_end"]
        .iter()
        .map(|dimension| base.join(format!("dimensions/minecraft/{dimension}")))
        .collect()
}

/// `directoriesInPath`: the subdirectories, none when the path cannot be listed.
fn directories_in_path(tree: &CowTree, path: &Path) -> Vec<PathBuf> {
    tree.list(path)
        .map(|names| {
            names
                .iter()
                .map(|name| path.join(name))
                .filter(|child| tree.is_dir(child))
                .collect()
        })
        .unwrap_or_default()
}

fn discover_dimensions(tree: &CowTree, base: &Path) -> Vec<PathBuf> {
    let root = base.join("dimensions");
    if !tree.exists(&root) {
        return default_dimensions(base);
    }
    match tree.list(&root) {
        Ok(namespaces) => {
            let discovered: Vec<PathBuf> = namespaces
                .iter()
                .map(|name| root.join(name))
                .filter(|path| tree.is_dir(path))
                .flat_map(|path| directories_in_path(tree, &path))
                .collect();
            if discovered.is_empty() {
                default_dimensions(base)
            } else {
                discovered
            }
        }
        Err(e) => {
            log::warn!("Failed to discover dimensions, assuming default: {e}");
            default_dimensions(base)
        }
    }
}
