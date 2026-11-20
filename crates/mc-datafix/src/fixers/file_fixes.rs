//! The `FileFixerUpper.Builder.addFixer` registrations of `DataFixers.addFixers`, in registration order. A file fix
//! shares its version with a data schema; the file fixes are registered after the data fixes (they are independent of
//! them), the fix implementations live next to the files they work on.

use crate::fixer::DataFixerBuilder;

pub fn add(b: &mut DataFixerBuilder) {
    b.add_file_fix("LegacyStructureFileFix", 1493);
    b.add_file_fix("ResourcePackLocationFileFix", 4772);
    b.add_file_fix("DimensionStorageFileFix", 4772);
    b.add_file_fix("PlayerStorageFileFix", 4772);
    b.add_file_fix("LevelDatToSavedDataFileFix", 4772);
    b.add_file_fix("RemoveObsoleteFilesFileFix", 4772);
    b.add_file_fix("GeneratedStructuresRenameFileFix", 4773);
    b.add_file_fix(
        "ReenableSpectatorsGenerateChunksInHardcoreWorldsFileFix",
        4899,
    );
}
