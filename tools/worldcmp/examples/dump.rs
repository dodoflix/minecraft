#![expect(clippy::print_stdout, reason = "example program that prints a dump")]

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut s = RegionFileStorage::new(
        RegionStorageInfo::new(
            "x",
            Identifier::parse("minecraft:overworld").unwrap(),
            "chunk",
        ),
        &a[1],
        false,
    );
    let t = s
        .read(&ChunkPos::new(a[2].parse().unwrap(), a[3].parse().unwrap()))
        .unwrap()
        .unwrap();
    for sec in t.get_list_or_empty("sections").iter().take(6) {
        let c = sec.as_compound().unwrap();
        println!("keys {:?}", c.keys().collect::<Vec<_>>());
        println!(
            "{}",
            worldcmp::compare::canon(
                c.get("block_states")
                    .unwrap()
                    .as_compound()
                    .unwrap()
                    .get("palette")
                    .unwrap()
            )
            .chars()
            .take(400)
            .collect::<String>()
        );
    }
    println!("keys {:?}", t.keys().collect::<Vec<_>>());
}
