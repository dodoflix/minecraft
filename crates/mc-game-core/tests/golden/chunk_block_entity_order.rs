//! Block entities of a chunk read from disk: `postLoad` instantiates them in NBT list order into the chunk's
//! fastutil map (`keepPacked` ones stay pending NBT), and a save lists `getBlockEntitiesPos()` order. The map and
//! set orders themselves are checked against the real classes by `mc-core`'s `java_collections` golden.

use mc_core::java_collections::{FastOpenHashMap, block_pos_hash, hash_set_union_order};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_game::world::level::chunk::LevelChunk;
use mc_nbt::CompoundTag;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::upgrade_data::UpgradeData;

fn tag(pos: BlockPos, keep_packed: bool) -> CompoundTag {
    let mut t = CompoundTag::new();
    t.put_string("id", "minecraft:chest");
    t.put_int("x", pos.x());
    t.put_int("y", pos.y());
    t.put_int("z", pos.z());
    t.put_boolean("keepPacked", keep_packed);
    t
}

#[test]
fn post_load_fills_the_map_in_list_order_and_saves_in_set_order() {
    let factory =
        PalettedContainerFactory::create(BiomeRegistry::new(vec!["minecraft:plains".into()]))
            .unwrap();
    let height = LevelHeight {
        min_y: -64,
        height: 384,
    };
    let mut proto = ProtoChunk::new(
        ChunkPos::new(2, -1),
        UpgradeData::empty(),
        height,
        &factory,
        None,
    );
    // 30 chests (the map rehashes past 24), every 7th kept packed; y repeats at 31 steps so some hashes collide
    let positions: Vec<BlockPos> = (0..30)
        .map(|i| {
            BlockPos::new(
                32 + (i * 5) % 16,
                -60 + (i % 3) * 31 + (i / 3) % 7,
                -16 + (i * 7) % 16,
            )
        })
        .collect();
    assert_eq!(
        positions
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        30
    );
    for pos in &positions {
        proto.set_block_state(*pos, Block::CHEST.default_state(), None);
    }
    let tags: Vec<CompoundTag> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| tag(*p, i % 7 == 6))
        .collect();
    let data = mc_world::chunk::LevelChunk::from_proto(proto, &factory);
    let mut chunk = LevelChunk::from_data(data, tags.clone());
    // saved before its post-load, a disk chunk keeps its block entity list as read
    assert_eq!(chunk.block_entity_nbt_for_saving(), tags);
    chunk.run_post_load();

    let mut expected = FastOpenHashMap::new(16, block_pos_hash);
    let mut packed = Vec::new();
    for (i, p) in positions.iter().enumerate() {
        if i % 7 == 6 {
            packed.push(*p);
        } else {
            expected.put(*p, ());
        }
    }
    let live: Vec<BlockPos> = chunk.block_entities.keys().copied().collect();
    assert_eq!(live, expected.iter().map(|e| *e.0).collect::<Vec<_>>());
    assert_eq!(live.len(), 30 - packed.len());
    let pending: Vec<BlockPos> = chunk.data.access.block_entities.keys().copied().collect();
    assert_eq!(pending.len(), packed.len());
    assert!(packed.iter().all(|p| pending.contains(p)));

    let saved: Vec<BlockPos> = chunk
        .block_entity_nbt_for_saving()
        .iter()
        .map(|t| {
            BlockPos::new(
                t.get_int("x").unwrap(),
                t.get_int("y").unwrap(),
                t.get_int("z").unwrap(),
            )
        })
        .collect();
    assert_eq!(
        saved,
        hash_set_union_order(pending.into_iter(), live.into_iter(), block_pos_hash)
    );
}
