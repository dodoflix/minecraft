//! `net.minecraft.util.datafix.DataFixers`: the global [`DataFixer`] and the version constants and entry points the
//! load paths use (`mc-world` and the game crates depend on this crate).

use crate::datafix_types::DataFixTypes;
use crate::fixer::{DataFixer, DataFixerBuilder};
use crate::ty::ensure_namespaced;
use mc_nbt::CompoundTag;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// `SharedConstants.getCurrentVersion().dataVersion().version()`: the one definition of the world version
/// (`mc_world::level_storage::WORLD_VERSION` and the chunk/entity/template stamps derive from it).
pub const CURRENT_VERSION: i32 = 5023;

/// `DataFixers.BLENDING_VERSION`.
pub const BLENDING_VERSION: i32 = 4997;

/// `FileFixerUpper.FILE_FIXER_INTRODUCTION_VERSION`: worlds saved before it map to file fixer version 0.
const FILE_FIXER_INTRODUCTION_VERSION: i32 = 4772;

/// `FileFixerUpper.latestFileFixerVersion` of `DataFixers.getFileFixer()`: the highest file-fix schema version.
///
/// shortcut: the recorded maximum (`tests/fixtures/fixes.tsv`, checked by `facade`), not yet derived from the
/// registered file schemas; derive it when the file fixer registers them.
pub const LATEST_FILE_FIXER_VERSION: i32 = 4899;

/// `DataFixers.getDataFixer()`: built on first use from [`crate::fixers::add_fixers`].
pub fn data_fixer() -> &'static DataFixer {
    static DATA_FIXER: LazyLock<DataFixer> = LazyLock::new(|| {
        let mut builder = DataFixerBuilder::new(CURRENT_VERSION);
        crate::fixers::add_fixers(&mut builder);
        builder.build()
    });
    &DATA_FIXER
}

/// `DataFixTypes.updateToCurrentVersion(DataFixers.getDataFixer(), tag, fromVersion)`: a failing update is logged by
/// the fixer and leaves the input unchanged.
pub fn update_to_current(ty: DataFixTypes, tag: &CompoundTag, from_version: i32) -> CompoundTag {
    ty.update_tag_to_current_version(data_fixer(), tag, from_version)
}

/// `FileFixerUpper.worldVersionToFileFixerVersion`.
pub fn world_version_to_file_fixer_version(level_data_version: i32) -> i32 {
    if level_data_version < FILE_FIXER_INTRODUCTION_VERSION {
        0
    } else {
        level_data_version
    }
}

/// `FileFixerUpper.requiresFileFixing`.
pub fn requires_file_fixing(level_data_version: i32) -> bool {
    world_version_to_file_fixer_version(level_data_version) < LATEST_FILE_FIXER_VERSION
}

/// `DataFixers.createRenamerNoNamespace(map)`: exact-id lookup.
pub fn create_renamer_no_namespace(
    map: &[(&str, &str)],
) -> impl Fn(&str) -> String + Clone + Send + Sync + 'static {
    let map = renames(map);
    move |id| map.get(id).cloned().unwrap_or_else(|| id.to_string())
}

/// `DataFixers.createRenamer(map)`: the id is looked up namespaced, an id without a rename is kept as it was written.
pub fn create_renamer(
    map: &[(&str, &str)],
) -> impl Fn(&str) -> String + Clone + Send + Sync + 'static {
    let map = renames(map);
    move |id| {
        map.get(&ensure_namespaced(id))
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }
}

/// `DataFixers.createRenamer(from, to)`.
pub fn create_renamer_pair(
    from: &str,
    to: &str,
) -> impl Fn(&str) -> String + Clone + Send + Sync + 'static {
    let (from, to) = (from.to_string(), to.to_string());
    move |id| {
        if ensure_namespaced(id) == from {
            to.clone()
        } else {
            id.to_string()
        }
    }
}

fn renames(map: &[(&str, &str)]) -> Arc<HashMap<String, String>> {
    Arc::new(
        map.iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect(),
    )
}
