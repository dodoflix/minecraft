//! Golden test of the block behaviour classes (`can_survive`, `update_shape`, `rotate`, `mirror`,
//! `get_post_process_pos`, `get_visual_shape`, `get_shape`, `get_collision_shape`, `get_block_support_shape`,
//! `get_interaction_shape`) against the vanilla block classes: for every state of every block, 16 pseudo-random
//! worlds over a stub `LevelReader` (see `tests/oracle/BlockBehaviourOracle.java`, which mirrors the world function,
//! the pools and the hashing). One 64-bit hash per block is compared with `tests/fixtures/block_behaviour.txt`.
//! Debugging a mismatch: `BLOCK_BEHAVIOUR_VERBOSE=<block> cargo test ... -- --nocapture` prints the per-state detail in
//! the format of `scripts/golden-block-behaviour.sh verbose <block>`.
use crate::common::block_pool::{Pools, mix};
use mc_core::mirror::Mirror;
use mc_core::phys::shapes::VoxelShape;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_game::world::level::block::{BlockStateExt, UPDATE_SHAPE_ORDER};
use mc_game::world::level::collision::CollisionContext;
use mc_game::world::level::{BlockGetter, LevelAccessor, LevelHeight, LevelReader, LightLayer};
use mc_world::heightmap::HeightmapType;
use mc_world::ticks::TickPriority;
use std::collections::HashMap;

const WORLDS: i64 = 16;
const ORIGIN: BlockPos = BlockPos::new(100, 70, -37);

fn brightness(x: i32, y: i32, z: i32, layer: i64) -> i32 {
    (i64::from(x) * 3 + i64::from(y) * 5 + i64::from(z) * 7 + layer * 11).rem_euclid(16) as i32
}

struct Stub {
    pools: Pools,
    center: BlockState,
    ticks: Vec<[i64; 7]>,
    random: LegacyRandomSource,
}

impl Stub {
    fn cell(&self, x: i32, y: i32, z: i32) -> BlockState {
        if x == ORIGIN.x() && y == ORIGIN.y() && z == ORIGIN.z() {
            return self.center;
        }
        self.pools.cell_at(x, y, z)
    }
}

impl BlockGetter for Stub {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        self.cell(pos.x(), pos.y(), pos.z())
    }
    fn get_fluid_state(&self, pos: BlockPos) -> mc_data::fluid::FluidState {
        self.get_block_state(pos).get_fluid_state()
    }
    fn get_block_entity(
        &self,
        _pos: BlockPos,
    ) -> Option<&dyn mc_game::world::block_entity::BlockEntity> {
        None
    }
    fn level_height(&self) -> LevelHeight {
        LevelHeight::new(-64, 384)
    }
}

impl LevelReader for Stub {
    fn has_chunk(&self, _x: i32, _z: i32) -> bool {
        true
    }
    fn get_height_at(&self, _ty: HeightmapType, x: i32, z: i32) -> i32 {
        60 + (x * 3 + z * 5).rem_euclid(8)
    }
    fn get_sea_level(&self) -> i32 {
        63
    }
    fn get_brightness(&self, layer: LightLayer, pos: BlockPos) -> i32 {
        brightness(
            pos.x(),
            pos.y(),
            pos.z(),
            if layer == LightLayer::Sky { 0 } else { 1 },
        )
    }
    fn get_sky_darken(&self) -> i32 {
        0
    }
    fn get_raw_brightness(&self, pos: BlockPos, _sky_darkening: i32) -> i32 {
        brightness(pos.x(), pos.y(), pos.z(), 5)
    }
    fn can_see_sky(&self, pos: BlockPos) -> bool {
        brightness(pos.x(), pos.y(), pos.z(), 9) < 8
    }
}

impl LevelAccessor for Stub {
    fn set_block(&mut self, _pos: BlockPos, _state: BlockState, _flags: u32, _limit: i32) -> bool {
        unreachable!("set_block")
    }
    fn remove_block(&mut self, _pos: BlockPos, _moved: bool) -> bool {
        unreachable!("remove_block")
    }
    fn destroy_block(
        &mut self,
        _pos: BlockPos,
        _drop: bool,
        _breaker: Option<i32>,
        _limit: i32,
    ) -> bool {
        unreachable!("destroy_block")
    }
    fn get_game_time(&self) -> i64 {
        0
    }
    fn next_sub_tick_count(&mut self) -> i64 {
        0
    }
    fn random(&mut self) -> &mut dyn RandomSource {
        &mut self.random
    }
    fn schedule_block_tick(
        &mut self,
        pos: BlockPos,
        block: Block,
        delay: i32,
        priority: TickPriority,
    ) {
        self.ticks.push([
            0,
            i64::from(block.id()),
            i64::from(pos.x() - ORIGIN.x()),
            i64::from(pos.y() - ORIGIN.y()),
            i64::from(pos.z() - ORIGIN.z()),
            i64::from(delay),
            priority as i64,
        ]);
    }
    fn schedule_fluid_tick(
        &mut self,
        pos: BlockPos,
        fluid: Fluid,
        delay: i32,
        priority: TickPriority,
    ) {
        self.ticks.push([
            1,
            i64::from(fluid.id()),
            i64::from(pos.x() - ORIGIN.x()),
            i64::from(pos.y() - ORIGIN.y()),
            i64::from(pos.z() - ORIGIN.z()),
            i64::from(delay),
            priority as i64,
        ]);
    }
    fn has_scheduled_block_tick(&self, _pos: BlockPos, _block: Block) -> bool {
        false
    }
    fn has_scheduled_fluid_tick(&self, _pos: BlockPos, _fluid: Fluid) -> bool {
        false
    }
    fn update_neighbors_at(&mut self, _pos: BlockPos, _source: Block) {}
}

fn hash_shape(mut h: u64, shape: &VoxelShape) -> u64 {
    let boxes = shape.to_aabbs();
    h = mix(h ^ boxes.len() as u64);
    for b in &boxes {
        for c in [b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z] {
            h = mix(h ^ c.to_bits());
        }
    }
    h
}

fn eval_state(stub: &mut Stub, s: BlockState, verbose: &mut Option<String>) -> u64 {
    use std::fmt::Write;
    let mut h = 0x1234567u64;
    let sid = i64::from(s.id());
    for r in Rotation::ALL {
        h = mix(h ^ u64::from(s.rotate(r).id()));
    }
    for m in Mirror::ALL {
        h = mix(h ^ u64::from(s.mirror(m).id()));
    }
    stub.center = s;
    for w in 0..WORLDS {
        stub.pools.world_seed = (w + 1) as u64;
        let surv = s.can_survive(&*stub, ORIGIN);
        h = mix(h ^ if surv { 1 } else { 2 });
        let pp = s.get_post_process_pos(&*stub, ORIGIN);
        h = mix(h ^ match pp {
            None => 7,
            Some(p) => {
                (i64::from(p.x() - ORIGIN.x()) * 31
                    + i64::from(p.y() - ORIGIN.y()) * 5
                    + i64::from(p.z() - ORIGIN.z())) as u64
            }
        });
        h = hash_shape(
            h,
            &s.get_visual_shape(&*stub, ORIGIN, &CollisionContext::empty()),
        );
        h = hash_shape(h, &s.get_shape(&*stub, ORIGIN));
        h = hash_shape(h, &s.get_collision_shape(&*stub, ORIGIN));
        h = hash_shape(h, &s.get_block_support_shape(&*stub, ORIGIN));
        h = hash_shape(h, &s.get_interaction_shape(&*stub, ORIGIN));
        if let Some(v) = verbose {
            let _ = write!(
                v,
                "S{sid} w{w} surv={surv} pp={}",
                pp.map_or("null".to_string(), |p| p.to_short_string())
            );
        }
        for (di, d) in UPDATE_SHAPE_ORDER.into_iter().enumerate() {
            let np = ORIGIN.relative(d);
            let ns = stub.get_block_state(np);
            stub.ticks.clear();
            stub.random = LegacyRandomSource::new(sid * 31 + w * 7 + di as i64);
            let out = s.update_shape(stub, ORIGIN, d, np, ns);
            h = mix(h ^ u64::from(out.id()));
            let r = stub.random.next_long();
            h = mix(h ^ r as u64);
            for t in &stub.ticks {
                for v in t {
                    h = mix(h ^ *v as u64);
                }
            }
            h = mix(h ^ stub.ticks.len() as u64);
            if let Some(v) = verbose {
                let _ = write!(v, " |{}->{}", d.get_name().to_ascii_uppercase(), out.id());
                for t in &stub.ticks {
                    let _ = write!(v, " t{t:?}");
                }
            }
        }
        if let Some(v) = verbose {
            v.push('\n');
        }
    }
    h
}

#[test]
fn block_behaviour_matches_vanilla() {
    let fixture = include_str!("../fixtures/block_behaviour.txt");
    let expected: HashMap<&str, &str> = fixture
        .lines()
        .filter(|l| !l.starts_with('S'))
        .filter_map(|l| l.split_once(' '))
        .collect();
    let only = std::env::var("BLOCK_BEHAVIOUR_VERBOSE").ok();
    let mut stub = Stub {
        pools: Pools::new(&[]),
        center: Block::AIR.default_state(),
        ticks: Vec::new(),
        random: LegacyRandomSource::new(0),
    };
    let mut mismatches = Vec::new();
    for b in Block::iter() {
        let name = b.data().name.trim_start_matches("minecraft:");
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        stub.pools.select(b);
        let mut verbose = only.as_ref().map(|_| String::new());
        let mut h = 0u64;
        for s in b.states() {
            h = mix(h ^ eval_state(&mut stub, s, &mut verbose));
        }
        if let Some(v) = &verbose {
            println!("{name} {h:x}\n{v}");
        }
        let want = expected.get(name).copied().unwrap_or("?");
        if want != format!("{h:x}") {
            mismatches.push(format!(
                "{name} ({}): expected {want}, got {h:x}",
                b.data().class
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} blocks differ from vanilla:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
