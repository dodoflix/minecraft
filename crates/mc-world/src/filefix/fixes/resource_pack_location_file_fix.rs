//! `ResourcePackLocationFileFix`.

use crate::filefix::{FileFix, FileFixOperation, Move};

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_fix_operation(FileFixOperation::Move(Move::new(
        "resources.zip",
        "resourcepacks/resources.zip",
    )));
    fix
}
