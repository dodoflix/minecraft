//! The file fixes registered in `DataFixers.addFixers`: `crate::filefix::file_fixer()` builds them from the
//! registrations of `mc_datafix::get_file_fixer()`.

pub mod dimension_storage_file_fix;
pub mod generated_structures_rename_file_fix;
pub mod legacy_structure_file_fix;
pub mod level_dat_to_saved_data_file_fix;
pub mod player_storage_file_fix;
pub mod reenable_spectators_generate_chunks_in_hardcore_worlds_file_fix;
pub mod remove_obsolete_files_file_fix;
pub mod resource_pack_location_file_fix;

use std::sync::LazyLock;

use mc_datafix::{data_fixer, get_file_fixer};

use super::{FileFix, FileFixerUpper};

/// `DataFixers.getFileFixer()`.
pub fn file_fixer() -> &'static FileFixerUpper {
    static FILE_FIXER: LazyLock<FileFixerUpper> = LazyLock::new(new_file_fixer);
    &FILE_FIXER
}

/// A file fixer of the registered fixes of its own, for what must not share [`file_fixer`]'s environment
/// ([`FileFixerUpper::with_hooks`]).
pub fn new_file_fixer() -> FileFixerUpper {
    FileFixerUpper::from_registry(data_fixer(), get_file_fixer(), |registration| {
        make_fix(registration.name, registration.version)
    })
}

fn make_fix(name: &str, version: i32) -> FileFix {
    match name {
        "LegacyStructureFileFix" => legacy_structure_file_fix::new(version),
        "ResourcePackLocationFileFix" => resource_pack_location_file_fix::new(version),
        "DimensionStorageFileFix" => dimension_storage_file_fix::new(version),
        "PlayerStorageFileFix" => player_storage_file_fix::new(version),
        "LevelDatToSavedDataFileFix" => level_dat_to_saved_data_file_fix::new(version),
        "RemoveObsoleteFilesFileFix" => remove_obsolete_files_file_fix::new(version),
        "GeneratedStructuresRenameFileFix" => generated_structures_rename_file_fix::new(version),
        "ReenableSpectatorsGenerateChunksInHardcoreWorldsFileFix" => {
            reenable_spectators_generate_chunks_in_hardcore_worlds_file_fix::new(version)
        }
        _ => panic!("no file fix {name}"),
    }
}
