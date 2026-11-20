//! Gives the full chunks of an overworld region folder the `blending_data` an upgraded pre-1.18 chunk carries
//! (`BlendingDataFix`), so the chunks around them are generated blended:
//!
//!   mark_old_chunks REGION_DIR X1,Z1,X2,Z2      (chunk coordinates, inclusive)
//!
//! Used by `scripts/worldgen-diff.sh --base-world` to build the stored neighbourhood both servers start from.
#![allow(clippy::print_stderr, reason = "command-line example")]

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::region::{RegionFileStorage, RegionStorageInfo};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, area] = args.as_slice() else {
        eprintln!("usage: mark_old_chunks REGION_DIR X1,Z1,X2,Z2");
        std::process::exit(2);
    };
    let bounds: Vec<i32> = area
        .split(',')
        .map(|v| v.parse().expect("a chunk coordinate"))
        .collect();
    let [x1, z1, x2, z2] = bounds[..] else {
        eprintln!("the area is X1,Z1,X2,Z2");
        std::process::exit(2);
    };
    let info = RegionStorageInfo::new(
        "world",
        Identifier::parse("minecraft:overworld").expect("identifier"),
        "chunk",
    );
    let mut storage = RegionFileStorage::new(info, dir, false);
    let mut marked = 0;
    for z in z1..=z2 {
        for x in x1..=x2 {
            let pos = ChunkPos::new(x, z);
            let Some(mut chunk) = storage.read(&pos).expect("readable chunk") else {
                continue;
            };
            if chunk.get_string("Status") != Some("minecraft:full") {
                continue;
            }
            let mut blending = CompoundTag::new();
            blending.put_int("min_section", -4);
            blending.put_int("max_section", 20);
            chunk.put("blending_data", blending);
            storage.write(&pos, Some(&chunk)).expect("writable chunk");
            marked += 1;
        }
    }
    storage.close().expect("closed storage");
    eprintln!("marked {marked} chunks");
}
