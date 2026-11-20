//! `DimensionStorageFileFix`: the data and region folders of the three dimensions move below `dimensions/`.

use crate::filefix::{FileFix, FileFixOperation, FileRelation, Move, RegexMove};

fn group_move(from_to: &[(&str, &str)], moves: Vec<Move>) -> FileFixOperation {
    FileFixOperation::GroupMove {
        from_to: from_to
            .iter()
            .map(|&(from, to)| (from.to_owned(), to.to_owned()))
            .collect(),
        moves,
    }
}

pub fn new(version: i32) -> FileFix {
    let mut fix = FileFix::new(version);
    fix.add_file_fix_operation(FileFixOperation::ApplyInFolders {
        folders: FileRelation::dimensions_data(),
        operations: vec![
            FileFixOperation::Move(Move::new("chunks.dat", "minecraft/chunk_tickets.dat")),
            FileFixOperation::Move(Move::new("raids.dat", "minecraft/raids.dat")),
            FileFixOperation::Move(Move::new("world_border.dat", "minecraft/world_border.dat")),
        ],
    });
    fix.add_file_fix_operation(group_move(
        &[
            ("data", "dimensions/minecraft/overworld/data/minecraft"),
            (
                "DIM-1/data",
                "dimensions/minecraft/the_nether/data/minecraft",
            ),
            ("DIM1/data", "dimensions/minecraft/the_end/data/minecraft"),
        ],
        vec![
            Move::new("chunks.dat", "chunk_tickets.dat"),
            Move::simple("raids.dat"),
            Move::new("raids_end.dat", "raids.dat"),
            Move::simple("world_border.dat"),
        ],
    ));
    fix.add_file_fix_operation(FileFixOperation::delete("DIM1/data"));
    fix.add_file_fix_operation(FileFixOperation::delete("DIM-1/data"));
    fix.add_file_fix_operation(FileFixOperation::ApplyInFolders {
        folders: FileRelation::data(),
        operations: vec![
            FileFixOperation::Move(Move::new("scoreboard.dat", "minecraft/scoreboard.dat")),
            FileFixOperation::Move(Move::new("stopwatches.dat", "minecraft/stopwatches.dat")),
            FileFixOperation::RegexMove(RegexMove::new(
                r"command_storage_([a-z0-9_.-]+)\.dat",
                r"$1/command_storage\.dat",
            )),
            FileFixOperation::Move(Move::new("idcounts.dat", "minecraft/maps/last_id.dat")),
            FileFixOperation::RegexMove(RegexMove::new(
                r"map_(\d+)\.dat",
                r"minecraft/maps/$1\.dat",
            )),
            FileFixOperation::Move(Move::new(
                "random_sequences.dat",
                "minecraft/random_sequences.dat",
            )),
        ],
    });
    fix.add_file_fix_operation(group_move(
        &[
            ("", "dimensions/minecraft/overworld"),
            ("DIM-1", "dimensions/minecraft/the_nether"),
            ("DIM1", "dimensions/minecraft/the_end"),
        ],
        vec![
            Move::simple("region"),
            Move::simple("entities"),
            Move::simple("poi"),
        ],
    ));
    fix.add_file_fix_operation(FileFixOperation::delete("DIM-1"));
    fix.add_file_fix_operation(FileFixOperation::delete("DIM1"));
    fix
}
