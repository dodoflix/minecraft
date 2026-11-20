//! `PlayerStorageFileFix`.

use crate::filefix::{FileFix, FileFixOperation, Move};

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    for (from, to) in [
        ("advancements", "players/advancements"),
        ("playerdata", "players/data"),
        ("stats", "players/stats"),
    ] {
        fix.add_file_fix_operation(FileFixOperation::Move(Move::new(from, to)));
    }
    fix
}
