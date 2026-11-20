//! `GeneratedStructuresRenameFileFix`.

use crate::filefix::{FileFix, FileFixOperation, FileRelation, Move};

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_fix_operation(FileFixOperation::ApplyInFolders {
        folders: FileRelation::generated_namespaces(),
        operations: vec![FileFixOperation::Move(Move::new("structures", "structure"))],
    });
    fix
}
