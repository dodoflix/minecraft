//! `RemoveObsoleteFilesFileFix`.

use super::legacy_structure_file_fix::{
    END_LEGACY_STRUCTURES, NETHER_LEGACY_STRUCTURES, OVERWORLD_LEGACY_STRUCTURES,
};
use crate::filefix::{FileFix, FileFixOperation};

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    for file in [
        "data/villages.dat",
        "data/villages_end.dat",
        "data/villages_nether.dat",
        "data/advancements",
        "data/functions",
    ] {
        fix.add_file_fix_operation(FileFixOperation::delete(file));
    }
    let leftover_structures = OVERWORLD_LEGACY_STRUCTURES
        .iter()
        .chain(&["Village"])
        .chain(&NETHER_LEGACY_STRUCTURES)
        .chain(&END_LEGACY_STRUCTURES);
    for leftover_structure in leftover_structures {
        fix.add_file_fix_operation(FileFixOperation::delete(&format!(
            "data/{leftover_structure}.dat"
        )));
        fix.add_file_fix_operation(FileFixOperation::delete(&format!(
            "data/{leftover_structure}_index.dat"
        )));
    }
    fix
}
