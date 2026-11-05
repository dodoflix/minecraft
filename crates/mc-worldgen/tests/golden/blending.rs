//! Differential test of `blending::*` against the vanilla server: hand-built old-world scenarios (legacy `BlendingData`
//! around a target chunk) through the real `Blender`, `BlendingData.calculateData` and `BelowZeroRetrogen`.
//! The fixture is produced by `tests/oracle/BlendingOracle.java` (scenario parameters + FNV-1a digests of every value stream;
//! regenerate with `scripts/vanilla-java.sh crates/mc-worldgen/tests/oracle/BlendingOracle.java <out>` from the repo root).

use crate::common;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_nbt::{CompoundTag, Tag};
use mc_world::below_zero_retrogen::BelowZeroRetrogen;
use mc_world::blending_data::{BlendingData, BlendingDataPacked};
use mc_world::chunk::{LevelChunk, LevelHeight, ProtoChunk};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::{BiomeId, BiomeRegistry, PalettedContainerFactory};
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;
use mc_worldgen::blending::{self, BlendingChunk, BlendingRegion, Blender};
use mc_worldgen::densityfunction::{DensityVolume, SamplerContext};

// ---- shared deterministic helpers (mirrors of the oracle) ----

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn h(seed: i64, a: i64, b: i64, c: i64) -> u64 {
    mix64(
        (seed as u64).wrapping_mul(0x9E3779B97F4A7C15)
            ^ mix64(
                (a as u64).wrapping_mul(0xD6E8FEB86659FD93)
                    ^ mix64((b as u64).wrapping_mul(0xCA5A826395121157) ^ mix64((c as u64).wrapping_add(0x1234567))),
            ),
    )
}

fn hmod(v: u64, m: i32) -> i32 {
    ((v >> 1) % m as u64) as i32
}

struct Digest {
    v: u64,
    count: u64,
}

impl Digest {
    fn new() -> Digest {
        Digest { v: 0xcbf29ce484222325, count: 0 }
    }
    fn b(&mut self, x: i32) {
        self.v ^= (x & 0xFF) as u64;
        self.v = self.v.wrapping_mul(0x100000001b3);
    }
    fn i(&mut self, x: i32) {
        self.b(x);
        self.b(x >> 8);
        self.b(x >> 16);
        self.b(x >> 24);
        self.count += 1;
    }
    fn f(&mut self, x: f32) {
        self.i(x.to_bits() as i32);
    }
    fn line(&self) -> String {
        format!("{:x} {}", self.v, self.count)
    }
}

fn factory() -> PalettedContainerFactory {
    PalettedContainerFactory::create(BiomeRegistry::new(BIOME_NAMES.iter().map(|n| format!("minecraft:{n}")).collect())).unwrap()
}

thread_local! {
    /// Registry ids of the 8 oracle biomes (identity in the synthetic-registry tests, real ids in the terrain test).
    static BIOME_TABLE: Cell<[u32; 8]> = const { Cell::new([0, 1, 2, 3, 4, 5, 6, 7]) };
}

fn biome_fn(seed: i64, qx: i32, qy: i32, qz: i32) -> BiomeId {
    BiomeId(BIOME_TABLE.with(|t| t.get())[hmod(h(seed, qx as i64, qy as i64, qz as i64), 8) as usize])
}

fn surf() -> [Block; 10] {
    [
        Block::GRASS_BLOCK,
        Block::SAND,
        Block::RED_SAND,
        Block::SNOW_BLOCK,
        Block::MYCELIUM,
        Block::PODZOL,
        Block::TERRACOTTA,
        Block::COARSE_DIRT,
        Block::STONE,
        Block::GRAVEL,
    ]
}

fn terrain(seed: i64, wx: i32, y: i32, wz: i32) -> BlockState {
    let hc = h(seed, wx as i64, wz as i64, 77);
    let top = 50 + hmod(hc >> 3, 40);
    let kind = hmod(hc >> 11, 16);
    let surfaces = surf();
    let surface = if kind == 1 { Block::DEEPSLATE } else { surfaces[hmod(hc >> 19, surfaces.len() as i32) as usize] };
    let r = h(seed, wx as i64, y as i64, wz as i64);
    let block = if y < top - 4 {
        if hmod(r, 13) == 0 {
            Block::AIR
        } else if hmod(r >> 7, 7) == 0 {
            Block::DEEPSLATE
        } else {
            Block::STONE
        }
    } else if y < top {
        if y == top - 1 {
            surface
        } else if kind == 1 {
            Block::DEEPSLATE
        } else {
            Block::DIRT
        }
    } else if kind == 0 && y <= 62 {
        Block::WATER
    } else if y < top + 6 {
        match hmod(r, 14) {
            0 => Block::OAK_LEAVES,
            1 => Block::OAK_LOG,
            2 => Block::BROWN_MUSHROOM_BLOCK,
            3 => Block::TORCH,
            4 => Block::COBWEB,
            5 => Block::RED_MUSHROOM_BLOCK,
            6 => Block::SPRUCE_LEAVES,
            7 => Block::WATER,
            8 => Block::LAVA,
            9 => Block::GLASS,
            10 => Block::OAK_FENCE,
            11 => Block::SNOW,
            _ => Block::AIR,
        }
    } else {
        Block::AIR
    };
    block.default_state()
}

const SPARSE: [i32; 5] = [0, 4, 8, 12, 15];

struct OldChunk {
    x: i32,
    z: i32,
    min_sec: i32,
    max_sec: i32,
    status: ChunkStatus,
    primed: bool,
    full: bool,
    heights: Option<Vec<f32>>,
}

struct Scenario {
    id: i32,
    seed: i64,
    min_y: i32,
    height: i32,
    cx: i32,
    cz: i32,
    olds: Vec<OldChunk>,
}

#[allow(clippy::too_many_arguments)]
fn make_chunk(
    f: &PalettedContainerFactory,
    cx: i32,
    cz: i32,
    seed: i64,
    min_y: i32,
    height: i32,
    old: Option<&OldChunk>,
    fill_blocks: bool,
) -> ProtoChunk {
    let blending = old.and_then(|o| {
        BlendingData::unpack(Some(&BlendingDataPacked { min_section: o.min_sec, max_section: o.max_sec, heights: o.heights.clone() }))
    });
    let mut chunk = ProtoChunk::new(ChunkPos::new(cx, cz), UpgradeData::empty(), LevelHeight::new(min_y, height), f, blending);
    if let Some(o) = old {
        for x in 0..16 {
            for z in 0..16 {
                if !fill_blocks || !o.full && !(SPARSE.contains(&x) && SPARSE.contains(&z)) {
                    continue;
                }
                for y in min_y..min_y + height {
                    let state = terrain(seed, cx * 16 + x, y, cz * 16 + z);
                    if !state.is_air() {
                        chunk.set_block_state(BlockPos::new(x, y, z), state, None);
                    }
                }
            }
        }
        let accessor = chunk.get_height_accessor_for_generation();
        chunk.access.fill_biomes_from_noise(accessor, |qx, qy, qz| biome_fn(seed ^ 0xB10, qx, qy, qz));
        if o.primed {
            chunk.access.prime_heightmaps(true, &[HeightmapType::WorldSurfaceWg]);
        }
        chunk.set_persisted_status(o.status);
    }
    chunk
}

struct Region<'f> {
    f: &'f PalettedContainerFactory,
    seed: i64,
    min_y: i32,
    height: i32,
    center: ChunkPos,
    chunks: RefCell<HashMap<i64, ProtoChunk>>,
}

impl BlendingRegion for Region<'_> {
    fn get_center(&self) -> ChunkPos {
        self.center
    }

    fn is_old_chunk_around(&self, pos: ChunkPos, range: i32) -> bool {
        self.chunks.borrow().values().any(|c| {
            c.access.blending_data.is_some() && (c.pos().x - pos.x).abs().max((c.pos().z - pos.z).abs()) <= range
        })
    }

    fn with_chunk(&self, chunk_x: i32, chunk_z: i32, f: &mut dyn FnMut(&mut dyn BlendingChunk)) {
        let mut chunks = self.chunks.borrow_mut();
        let chunk = chunks
            .entry(ChunkPos::pack_xz(chunk_x, chunk_z))
            .or_insert_with(|| make_chunk(self.f, chunk_x, chunk_z, self.seed, self.min_y, self.height, None, true));
        f(chunk);
    }
}

type Golden = HashMap<(i32, String), String>;

fn parse(text: &str) -> (Vec<Scenario>, Vec<Retro>, Golden) {
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut retros = Vec::new();
    let mut golden = Golden::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        match t[0] {
            "SCENARIO" => scenarios.push(Scenario {
                id: t[1].parse().unwrap(),
                seed: t[2].parse().unwrap(),
                min_y: t[3].parse().unwrap(),
                height: t[4].parse().unwrap(),
                cx: t[5].parse().unwrap(),
                cz: t[6].parse().unwrap(),
                olds: Vec::new(),
            }),
            "OLD" => scenarios.last_mut().unwrap().olds.push(OldChunk {
                x: t[1].parse().unwrap(),
                z: t[2].parse().unwrap(),
                min_sec: t[3].parse().unwrap(),
                max_sec: t[4].parse().unwrap(),
                status: ChunkStatus::decode_str(t[5]).unwrap(),
                primed: t[6] == "1",
                full: t[7] == "1",
                heights: (t[8] != "-")
                    .then(|| t[8].split(',').map(|b| f32::from_bits(u32::from_str_radix(b, 16).unwrap())).collect()),
            }),
            "RETRO" => retros.push(Retro {
                id: t[1].parse().unwrap(),
                seed: t[2].parse().unwrap(),
                cx: t[3].parse().unwrap(),
                cz: t[4].parse().unwrap(),
                target: t[5].to_string(),
                bits: (t[6] != "-").then(|| {
                    t[6].trim_matches(|c| c == '[' || c == ']').split(',').map(|v| v.parse::<i64>().unwrap()).collect()
                }),
            }),
            "S" => {
                golden.insert((t[1].parse().unwrap(), t[2].to_string()), t[3..].join(" "));
            }
            _ => {}
        }
    }
    (scenarios, retros, golden)
}

struct Retro {
    id: i32,
    seed: i64,
    cx: i32,
    cz: i32,
    target: String,
    bits: Option<Vec<i64>>,
}

fn check(golden: &Golden, id: i32, what: &str, digest: &Digest) {
    let expected = golden.get(&(id, what.to_string())).unwrap_or_else(|| panic!("scenario {id}: no golden {what}"));
    assert_eq!(&digest.line(), expected, "scenario {id}: {what}");
}

#[test]
fn blending_matches_vanilla() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blending/golden.txt");
    let (scenarios, retros, golden) = parse(&std::fs::read_to_string(path).unwrap());
    assert!(scenarios.len() >= 16);
    let f = factory();
    let mut empties = 0;
    for sc in &scenarios {
        let id = sc.id;
        let (seed, cx, cz) = (sc.seed, sc.cx, sc.cz);
        let region = Region {
            f: &f,
            seed,
            min_y: sc.min_y,
            height: sc.height,
            center: ChunkPos::new(cx, cz),
            chunks: RefCell::new(HashMap::new()),
        };
        for o in &sc.olds {
            let (ox, oz) = (o.x, o.z);
            region.chunks.borrow_mut().insert(ChunkPos::pack_xz(ox, oz), make_chunk(&f, ox, oz, seed, sc.min_y, sc.height, Some(o), true));
        }
        let blender = Blender::of(Some(&region));
        assert_eq!(golden[&(id, "empty".to_string())], format!("{} 0", blender.is_empty() as i32), "scenario {id}: empty");
        empties += blender.is_empty() as i32;

        let (mut dh, mut dd, mut db) = (Digest::new(), Digest::new(), Digest::new());
        for o in &sc.olds {
            let chunks = region.chunks.borrow();
            let data = chunks[&ChunkPos::pack_xz(o.x, o.z)].access.blending_data.as_ref().unwrap();
            let p = data.pack();
            dh.i(p.min_section);
            dh.i(p.max_section);
            match &p.heights {
                Some(hs) => {
                    dh.i(1);
                    hs.iter().for_each(|&x| dh.f(x));
                }
                None => dh.i(0),
            }
            for col in &data.densities {
                match col {
                    None => dd.i(-1),
                    Some(col) => {
                        dd.i(col.len() as i32);
                        col.iter().for_each(|&x| dd.f(x));
                    }
                }
            }
            for col in &data.biomes {
                match col {
                    None => db.i(-1),
                    Some(col) => {
                        db.i(col.len() as i32);
                        col.iter().for_each(|b| db.i(b.0 as i32));
                    }
                }
            }
        }
        check(&golden, id, "heights", &dh);
        check(&golden, id, "densities", &dd);
        check(&golden, id, "biomes", &db);

        let mut dof = Digest::new();
        for a in 0..48 {
            for b in 0..48 {
                let out = blender.blend_offset_and_factor_at(cx * 16 - 64 + a * 3, cz * 16 - 64 + b * 3);
                dof.f(out.alpha);
                dof.f(out.blending_offset);
            }
        }
        check(&golden, id, "offsets", &dof);

        let volumes = [
            DensityVolume::new(5, 3, 5, cx * 16, -64, cz * 16, 4, 8, 4),
            DensityVolume::new(17, 2, 17, cx * 16 - 8, 0, cz * 16 - 8, 1, 1, 1),
            DensityVolume::new(9, 2, 9, cx * 16 - 37, 10, cz * 16 + 11, 2, 3, 2),
            DensityVolume::new(3, 1, 4, cx * 16 + 5, 70, cz * 16 - 3, 7, 1, 5),
        ];
        let mut dv = Digest::new();
        let ctx = SamplerContext::empty_uncached();
        for v in &volumes {
            let ob = blender.blend_offset_and_factor(v);
            dv.i(ob.min_cell_x);
            dv.i(ob.min_cell_z);
            dv.i(ob.cell_count_x);
            dv.i(ob.cell_count_z);
            ob.alphas.iter().for_each(|&x| dv.f(x));
            ob.offsets.iter().for_each(|&x| dv.f(x));
            let (alpha_sampler, offset_sampler) = (ob.create_alpha_sampler(), ob.create_offset_sampler());
            let mut bx = v.min_block_x - 24;
            while bx <= v.max_block_x() + 24 {
                let mut bz = v.min_block_z - 24;
                while bz <= v.max_block_z() + 24 {
                    dv.f(ob.get_alpha(bx, bz));
                    dv.f(ob.get_offset(bx, bz));
                    dv.f(alpha_sampler.sample_value(&ctx, bx, 5, bz));
                    dv.f(offset_sampler.sample_value(&ctx, bx, 5, bz));
                    bz += 5;
                }
                bx += 5;
            }
        }
        check(&golden, id, "volumes", &dv);

        let mut dbd = Digest::new();
        for i in 0..6000i64 {
            let mut x = cx * 16 - 48 + hmod(h(seed, i, 2, 0), 128);
            let mut y = -70 + hmod(h(seed, i, 2, 1), 400);
            let mut z = cz * 16 - 48 + hmod(h(seed, i, 2, 2), 128);
            if i % 3 == 0 {
                x &= !3;
                z &= !3;
                y &= !7;
            }
            let noise = (hmod(h(seed, i, 2, 3), 4000) - 2000) as f32 / 1000.0f32;
            dbd.f(blender.blend_density(x, y, z, noise));
        }
        check(&golden, id, "density", &dbd);

        let mut dbr = Digest::new();
        let resolver = blender.get_biome_resolver(Box::new(move |qx, qy, qz| biome_fn(seed ^ 5, qx, qy, qz)));
        for qx in cx * 4 - 16..cx * 4 + 20 {
            for qz in cz * 4 - 16..cz * 4 + 20 {
                for qy in -16..80 {
                    dbr.i(resolver(qx, qy, qz).0 as i32);
                }
            }
        }
        check(&golden, id, "biome_resolver", &dbr);

        let mut dc = Digest::new();
        let mut trues = 0;
        match blender.get_carving_filter() {
            None => dc.i(-1),
            Some(filter) => {
                for i in 0..20000i64 {
                    let x = hmod(h(seed, i, 3, 0), 64) - 24;
                    let y = sc.min_y - 8 + hmod(h(seed, i, 3, 1), sc.height + 16);
                    let z = hmod(h(seed, i, 3, 2), 64) - 24;
                    let t = filter(x, y, z);
                    trues += t as i32;
                    dc.i(t as i32);
                }
            }
        }
        check(&golden, id, "carving", &dc);
        assert_eq!(golden[&(id, "carving_true".to_string())], format!("{trues} 0"), "scenario {id}: carving_true");

        let mut dbt = Digest::new();
        blending::generate_border_ticks(&region, ChunkPos::new(cx, cz));
        let chunks = region.chunks.borrow();
        let centre = &chunks[&ChunkPos::pack_xz(cx, cz)];
        for list in &centre.access.post_processing {
            match list {
                None => dbt.i(-1),
                Some(l) => {
                    dbt.i(l.len() as i32);
                    l.iter().for_each(|&s| dbt.i(s as i32));
                }
            }
        }
        check(&golden, id, "border", &dbt);
    }
    assert!(empties > 0 && (empties as usize) < scenarios.len(), "scenarios must cover empty and non-empty blenders");

    for r in &retros {
        retro(&f, r, &golden);
    }
}

fn emit_blocks(chunk: &ProtoChunk) -> Digest {
    let mut d = Digest::new();
    for y in -64..16 {
        for z in 0..16 {
            for x in 0..16 {
                d.i(chunk.get_block_state(BlockPos::new(x, y, z)).id() as i32);
            }
        }
    }
    d
}

fn retro(f: &PalettedContainerFactory, r: &Retro, golden: &Golden) {
    let (id, seed, cx, cz) = (r.id, r.seed, r.cx, r.cz);
    let mut chunk = ProtoChunk::new(ChunkPos::new(cx, cz), UpgradeData::empty(), LevelHeight::new(-64, 384), f, None);
    let pool = [Block::BEDROCK, Block::BEDROCK, Block::BEDROCK, Block::STONE, Block::DEEPSLATE, Block::AIR, Block::WATER, Block::DIRT];
    for x in 0..16 {
        for z in 0..16 {
            for y in -64..12 {
                let b = pool[hmod(h(seed, (x + cx * 16) as i64, y as i64, (z + cz * 16) as i64), pool.len() as i32) as usize];
                if b != Block::AIR {
                    chunk.set_block_state(BlockPos::new(x, y, z), b.default_state(), None);
                }
            }
        }
    }
    let accessor = chunk.get_height_accessor_for_generation();
    chunk.access.fill_biomes_from_noise(accessor, |qx, qy, qz| biome_fn(seed ^ 11, qx, qy, qz));
    chunk.access.fill_biomes_from_noise(LevelHeight::new(0, 16), |qx, qy, qz| biome_fn(seed ^ 12, qx, qy, qz));
    let mut tag = CompoundTag::new();
    tag.put_string("target_status", r.target.clone());
    if let Some(bits) = &r.bits {
        tag.put_long_array("missing_bedrock", bits.clone());
    }
    let retrogen = BelowZeroRetrogen::parse(&Tag::Compound(tag)).expect("retrogen parses");
    chunk.set_below_zero_retrogen(Some(retrogen.clone()));

    blending::replace_old_bedrock(&mut chunk);
    check(golden, id, "retro_replace", &emit_blocks(&chunk));
    if retrogen.has_bedrock_holes() {
        blending::apply_bedrock_mask(&retrogen, &mut chunk);
    }
    assert_eq!(golden[&(id, "retro_holes".to_string())], format!("{} 0", retrogen.has_bedrock_holes() as i32));
    check(golden, id, "retro_mask", &emit_blocks(&chunk));

    let retained = |b: BiomeId| (3..=5).contains(&b.0);
    let mut dr = Digest::new();
    let wrapped = blending::get_below_zero_biome_resolver(
        Box::new(move |qx, qy, qz| biome_fn(seed ^ 13, qx, qy, qz)),
        &chunk,
        Box::new(retained),
    );
    for qx in cx * 4..cx * 4 + 4 {
        for qz in cz * 4..cz * 4 + 4 {
            for qy in -16..0 {
                dr.i(wrapped(qx, qy, qz).0 as i32);
            }
        }
    }
    chunk.set_below_zero_retrogen(None);
    let plain = blending::get_below_zero_biome_resolver(
        Box::new(move |qx, qy, qz| biome_fn(seed ^ 13, qx, qy, qz)),
        &chunk,
        Box::new(retained),
    );
    for qx in cx * 4..cx * 4 + 4 {
        for qz in cz * 4..cz * 4 + 4 {
            for qy in -16..0 {
                dr.i(plain(qx, qy, qz).0 as i32);
            }
        }
    }
    check(golden, id, "retro_resolver", &dr);
}

#[test]
fn level_chunk_shares_the_chunk_view_semantics() {
    // `LevelChunk` implements `BlendingChunk` for fully generated neighbours (vanilla ImposterProtoChunk): old, full status.
    let f = factory();
    let mut chunk = LevelChunk::empty(LevelHeight::new(-64, 384), &f, ChunkPos::new(0, 0));
    assert!(!chunk.is_old_noise_generation());
    assert_eq!(chunk.get_highest_generated_status(), ChunkStatus::Full);
    assert!(chunk.blending_data().is_none());
    chunk.update_blending_data(&mut |_, _| panic!("no blending data"));
}

// ------------------------------------------------------------------------------------------------------------------
// Blended terrain: createBiomes / doFill / buildTerrain with a non-empty Blender (tests/oracle/BlendedTerrainOracle.java)
// ------------------------------------------------------------------------------------------------------------------


use mc_worldgen::biome::BiomeManager;
use mc_worldgen::generator::{BuildTerrainContext, ChunkGenerator, NoiseBasedChunkGenerator};
use mc_worldgen::random_state::RandomState;
use std::sync::Arc;

struct TerrainCase {
    id: i32,
    setting: String,
    seed: i64,
    cx: i32,
    cz: i32,
    kind: i32,
    olds: Vec<OldChunk>,
    target: String,
    bits: Option<Vec<i64>>,
}

type Lines = HashMap<(String, i32), Vec<String>>;

fn parse_terrain(text: &str) -> (Vec<TerrainCase>, Lines) {
    let mut cases: Vec<TerrainCase> = Vec::new();
    let mut lines = Lines::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        match t[0] {
            "CASE" => cases.push(TerrainCase {
                id: t[1].parse().unwrap(),
                setting: t[2].to_owned(),
                seed: t[3].parse().unwrap(),
                cx: t[4].parse().unwrap(),
                cz: t[5].parse().unwrap(),
                kind: t[6].parse().unwrap(),
                olds: Vec::new(),
                target: String::new(),
                bits: None,
            }),
            "OLD" => cases.last_mut().unwrap().olds.push(OldChunk {
                x: t[1].parse().unwrap(),
                z: t[2].parse().unwrap(),
                min_sec: t[3].parse().unwrap(),
                max_sec: t[4].parse().unwrap(),
                status: ChunkStatus::decode_str(t[5]).unwrap(),
                primed: t[6] == "1",
                full: false,
                heights: (t[7] != "-")
                    .then(|| t[7].split(',').map(|b| f32::from_bits(u32::from_str_radix(b, 16).unwrap())).collect()),
            }),
            "RETRO" => {
                let c = cases.last_mut().unwrap();
                c.target = t[2].to_owned();
                c.bits = (t[3] != "-")
                    .then(|| t[3].trim_matches(|ch| ch == '[' || ch == ']').split(',').map(|v| v.parse::<i64>().unwrap()).collect());
            }
            "END" => {}
            tag => {
                lines.entry((tag.to_owned(), t[1].parse().unwrap())).or_default().push(t[2..].join(" "));
            }
        }
    }
    (cases, lines)
}

/// Fresh region with the case's old chunks plus the target chunk (kind 0 new, 1 old, 2 below-zero retrogen).
fn build_region<'f>(f: &'f PalettedContainerFactory, c: &TerrainCase, height: LevelHeight, target_status: ChunkStatus) -> Region<'f> {
    let region = Region {
        f,
        seed: c.seed,
        min_y: height.min_y,
        height: height.height,
        center: ChunkPos::new(c.cx, c.cz),
        chunks: RefCell::new(HashMap::new()),
    };
    for o in &c.olds {
        if (o.x, o.z) == (c.cx, c.cz) {
            continue;
        }
        region.chunks.borrow_mut().insert(ChunkPos::pack_xz(o.x, o.z), make_chunk(f, o.x, o.z, c.seed, height.min_y, height.height, Some(o), true));
    }
    let target = if c.kind == 1 {
        let o = c.olds.iter().find(|o| (o.x, o.z) == (c.cx, c.cz)).unwrap();
        let mut chunk = make_chunk(f, c.cx, c.cz, c.seed, height.min_y, height.height, Some(o), false);
        chunk.set_persisted_status(target_status);
        chunk
    } else {
        let mut chunk = ProtoChunk::new(ChunkPos::new(c.cx, c.cz), UpgradeData::empty(), height, f, None);
        if c.kind == 2 {
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..8 {
                        if hmod(h(c.seed, (c.cx * 16 + x) as i64, y as i64, (c.cz * 16 + z) as i64), 3) != 0 {
                            chunk.set_block_state(BlockPos::new(x, y, z), Block::BEDROCK.default_state(), None);
                        }
                    }
                }
            }
            let seed = c.seed;
            chunk.access.fill_biomes_from_noise(LevelHeight::new(0, 16), |qx, qy, qz| biome_fn(seed ^ 12, qx, qy, qz));
            let mut tag = CompoundTag::new();
            tag.put_string("target_status", c.target.clone());
            if let Some(bits) = &c.bits {
                tag.put_long_array("missing_bedrock", bits.clone());
            }
            chunk.set_below_zero_retrogen(Some(BelowZeroRetrogen::parse(&Tag::Compound(tag)).expect("retrogen")));
        }
        chunk.set_persisted_status(target_status);
        chunk
    };
    region.chunks.borrow_mut().insert(ChunkPos::pack_xz(c.cx, c.cz), target);
    region
}

fn terrain_ctx<'a>(rs: &'a Arc<RandomState>, blender: &'a Blender, manager: &'a BiomeManager<'a>) -> BuildTerrainContext<'a> {
    BuildTerrainContext {
        random_state: rs,
        blender,
        structure_starts: &[],
        biome_manager: manager,
        possible_biomes: None,
        carver_biome_cache: None,
    }
}

fn check_lines(lines: &Lines, tag: &str, id: i32, got: &[String]) {
    let expected = lines.get(&(tag.to_owned(), id)).unwrap_or_else(|| panic!("case {id}: no golden {tag}"));
    assert_eq!(got, expected.as_slice(), "case {id}: {tag}");
}

#[test]
fn blended_terrain_matches_vanilla() {
    use crate::common::{WORLD, biome_hash, generator, heightmap_lines, post_processing_hash, random_state, section_hashes};

    let table: [u32; 8] = std::array::from_fn(|i| WORLD.biomes.id_of_name(BIOME_NAMES[i]).expect("biome").0);
    BIOME_TABLE.with(|t| t.set(table));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blended_terrain_golden.txt");
    let (cases, lines) = parse_terrain(&std::fs::read_to_string(path).unwrap());
    assert!(cases.len() >= 12);
    let f = &WORLD.factory;
    let mut generators: HashMap<String, NoiseBasedChunkGenerator> = HashMap::new();
    for c in &cases {
        let id = c.id;
        let generator = generators.entry(c.setting.clone()).or_insert_with(|| generator(&c.setting));
        let rs = random_state(generator, c.seed);
        let height = common::level_height(&c.setting);
        let resolver = generator.biome_source().create_uncached_resolver(&rs);
        let manager = BiomeManager::new(&*resolver, BiomeManager::obfuscate_seed(c.seed));
        let target_key = ChunkPos::pack_xz(c.cx, c.cz);

        // createBiomes
        let region = build_region(f, c, height, ChunkStatus::Empty);
        let blender = Blender::of(Some(&region));
        let mut chunk = region.chunks.borrow_mut().remove(&target_key).unwrap();
        generator.create_biomes(&rs, &blender, &mut chunk);
        chunk.set_persisted_status(ChunkStatus::Biomes);
        check_lines(&lines, "BE", id, &[format!("{}", blender.is_empty() as i32)]);
        check_lines(&lines, "BB", id, &[format!("{:016x}", biome_hash(&chunk))]);

        // doFill alone
        let region = build_region(f, c, height, ChunkStatus::Biomes);
        let blender = Blender::of(Some(&region));
        let mut chunk = region.chunks.borrow_mut().remove(&target_key).unwrap();
        {
            let ctx = terrain_ctx(&rs, &blender, &manager);
            let ns = generator.settings().noise.clamp_to_height_accessor(chunk.get_height_accessor_for_generation());
            let mut nc = generator.create_noise_chunk(chunk.pos(), &ctx, ns);
            generator.do_fill(&mut nc, &mut chunk);
        }
        check_lines(&lines, "BD", id, &[section_hashes(&chunk).trim_start().to_owned()]);

        // buildTerrain
        let region = build_region(f, c, height, ChunkStatus::Biomes);
        let blender = Blender::of(Some(&region));
        let mut chunk = region.chunks.borrow_mut().remove(&target_key).unwrap();
        generator.build_terrain(&mut chunk, &terrain_ctx(&rs, &blender, &manager));
        check_lines(&lines, "BF", id, &[section_hashes(&chunk).trim_start().to_owned()]);
        let got: Vec<String> = heightmap_lines(&mut chunk);
        check_lines(&lines, "BFH", id, &got);
        check_lines(&lines, "BFP", id, &[format!("{:016x}", post_processing_hash(&chunk))]);
        if let Some(retrogen) = chunk.get_below_zero_retrogen().cloned() {
            blending::replace_old_bedrock(&mut chunk);
            if retrogen.has_bedrock_holes() {
                blending::apply_bedrock_mask(&retrogen, &mut chunk);
            }
            check_lines(&lines, "BR", id, &[section_hashes(&chunk).trim_start().to_owned()]);
        } else {
            assert!(!lines.contains_key(&("BR".to_owned(), id)), "case {id}: vanilla had a retrogen");
        }
    }
    assert!(cases.iter().any(|c| c.kind == 2) && cases.iter().any(|c| c.kind == 1));
}

const BIOME_NAMES: [&str; 8] = ["plains", "forest", "desert", "lush_caves", "dripstone_caves", "deep_dark", "ocean", "river"];
