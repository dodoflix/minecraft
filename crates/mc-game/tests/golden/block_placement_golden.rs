//! Golden test of `Block.getStateForPlacement` (plus the replace-clicked decision of `BlockPlaceContext`) against the vanilla
//! block classes on a real level: for every block, 4 neighbourhoods of 16 placement contexts (clicked face, hit location, player yaw/pitch,
//! sneaking, the held item, the states of the clicked cell and of the cell beside it, a pseudo-random neighbourhood), see
//! `tests/oracle/BlockPlacementOracle.java`, which mirrors the neighbourhood function, the context derivation and the hashing.
//! One 64-bit hash per block is compared with `tests/fixtures/block_placement.txt`.
//! Debugging a mismatch: `BLOCK_PLACEMENT_VERBOSE=<block|*> cargo test ... -- --nocapture` prints the per-context detail in the
//! format of `scripts/golden-block-placement.sh verbose <block>`.
use crate::common::block_scenario::parse_state;
use mc_core::direction::Direction;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game::server::level::ServerLevel;
use mc_game::world::item::{
    BlockPlaceContext, ContextPlayer, InteractionHand, ItemStack, UseOnContext,
};
use mc_game::world::level::block::BlockStateExt;
use mc_game::world::level::block::helpers::get_state_for_placement;
use mc_game::world::level::flags::{UPDATE_LIMIT, UPDATE_SKIP_ALL_SIDEEFFECTS};
use mc_game::world::level::{BlockGetter, LevelAccessor};
use std::collections::HashMap;
use std::fmt::Write;
use std::time::{Duration, Instant};

const WORLDS: u64 = 4;
const PER_WORLD: u64 = 16;
/// Blocks some placements need next to them, added to the pool of the block behaviour golden.
const EXTRA: [&str; 26] = [
    "jungle_log",
    "pointed_dripstone",
    "redstone_wire",
    "redstone_block",
    "rail",
    "powered_rail",
    "lever",
    "stone",
    "oak_fence",
    "oak_stairs",
    "scaffolding",
    "sea_pickle",
    "snow",
    "vine",
    "sculk_vein",
    "white_bed",
    "torch",
    "wall_torch",
    "oak_door",
    "oak_trapdoor",
    "oak_sign",
    "piston",
    "sticky_piston",
    "observer",
    "tripwire_hook",
    "tripwire",
];
/// Blocks only one block needs next to it (cocoa: a jungle log), added to that block's own neighbour pool: in the shared pool
/// they would be too rare to ever turn up.
const NEEDS: [(&str, [&str; 2]); 1] = [("cocoa", ["jungle_log", "jungle_wood"])];
const POOL_CAP: usize = 24;
const CLASS_CAP: usize = 200;
const BASE: BlockPos = BlockPos::new(4, 80, 4);
const SIZE: i32 = 8;
const FACES: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];
const FRACTION: [f64; 8] = [0.0, 0.0625, 0.25, 0.4375, 0.5, 0.5625, 0.75, 1.0];
/// 0 keep, 1 air, 2 water, 3 flowing water, 4 own state, 5 snow, 6 short grass, 7 lava.
const SEL: [usize; 12] = [0, 0, 1, 1, 2, 3, 4, 4, 4, 5, 6, 7];

fn mix(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xCBF29CE484222325u64, |h, c| {
        (h ^ u64::from(c)).wrapping_mul(0x100000001B3)
    })
}

/// The neighbourhood function of the oracle (the pools of `block_behaviour_golden`).
struct Pools {
    world_seed: u64,
    global: Vec<BlockState>,
    own: Vec<BlockState>,
    mates: Vec<BlockState>,
}

impl Pools {
    fn cell(&self, p: BlockPos) -> BlockState {
        let h = mix(self
            .world_seed
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(
                (i64::from(p.x()).wrapping_mul(73856093)
                    ^ i64::from(p.y()).wrapping_mul(19349663)
                    ^ i64::from(p.z()).wrapping_mul(83492791)) as u64,
            ));
        let r = h % 100;
        let h2 = mix(h);
        let pool = if r < 25 {
            return Block::AIR.default_state();
        } else if r < 55 {
            &self.global
        } else if r < 80 {
            &self.own
        } else {
            &self.mates
        };
        pool[(h2 % pool.len() as u64) as usize]
    }

    fn special(
        &self,
        sel: usize,
        kept: BlockState,
        own_index: u64,
        fixed: &[BlockState; 5],
    ) -> BlockState {
        match sel {
            1 => Block::AIR.default_state(),
            2 => fixed[0],
            3 => fixed[1],
            4 => self.own[(own_index % self.own.len() as u64) as usize],
            5 => fixed[2],
            6 => fixed[3],
            7 => fixed[4],
            _ => kept,
        }
    }
}

fn put(level: &mut ServerLevel, p: BlockPos, s: BlockState) {
    if level.get_block_state(p) != s {
        level.set_block(p, s, UPDATE_SKIP_ALL_SIDEEFFECTS, UPDATE_LIMIT);
    }
}

fn level() -> ServerLevel {
    let mut level = ServerLevel::detached(
        mc_core::ident::Identifier::parse("minecraft:overworld").expect("dimension id"),
    );
    // the neighbourhood lives in chunk (0, 0): force it and tick the chunk system until it is entity ticking
    level
        .chunk_source_mut()
        .update_chunk_forced(ChunkPos::new(0, 0), true);
    let start = Instant::now();
    loop {
        level.tick_chunk_source(&|| true, false, true);
        while level.poll_chunk_task() {}
        level.tick_entity_management();
        if level.is_position_ticking_with_entities_loaded(ChunkPos::pack_xz(0, 0)) {
            return level;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "chunk did not reach entity ticking"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn capped(block: Block, cap: usize) -> Vec<BlockState> {
    block.states().take(cap).collect()
}

fn describe(s: Option<BlockState>) -> String {
    s.map_or_else(
        || "none".to_string(),
        crate::common::block_scenario::state_string,
    )
}

#[test]
fn block_placement_matches_vanilla() {
    let fixture = include_str!("../fixtures/block_placement.txt");
    let expected: HashMap<&str, &str> = fixture.lines().filter_map(|l| l.split_once(' ')).collect();
    let mut global = Vec::new();
    for name in include_str!("../fixtures/block_behaviour_pool.txt")
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        global.extend(capped(
            Block::by_name(&format!("minecraft:{name}"))
                .unwrap_or_else(|| panic!("pool block {name}")),
            POOL_CAP,
        ));
    }
    for name in EXTRA {
        global.extend(capped(
            Block::by_name(&format!("minecraft:{name}"))
                .unwrap_or_else(|| panic!("extra block {name}")),
            POOL_CAP,
        ));
    }
    let fixed = [
        parse_state("minecraft:water"),
        parse_state("minecraft:water[level=3]"),
        parse_state("minecraft:snow"),
        parse_state("minecraft:short_grass"),
        parse_state("minecraft:lava"),
    ];
    let only = std::env::var("BLOCK_PLACEMENT_VERBOSE").ok();
    let mut level = level();
    let mut pools = Pools {
        world_seed: 0,
        global,
        own: Vec::new(),
        mates: Vec::new(),
    };
    let mut mismatches = Vec::new();
    for b in Block::iter() {
        let name = b.data().name.trim_start_matches("minecraft:");
        if only.as_deref().is_some_and(|o| o != "*" && o != name) {
            continue;
        }
        pools.own = b.states().collect();
        let mut mates = Vec::new();
        for o in Block::iter() {
            if o.data().class == b.data().class && mates.len() < CLASS_CAP {
                let room = CLASS_CAP - mates.len();
                mates.extend(capped(o, room));
            }
        }
        for need in NEEDS
            .iter()
            .filter(|(block, _)| *block == name)
            .flat_map(|(_, needs)| needs.iter())
        {
            mates.extend(capped(
                Block::by_name(&format!("minecraft:{need}"))
                    .unwrap_or_else(|| panic!("needed block {need}")),
                POOL_CAP,
            ));
        }
        pools.mates = mates;
        let name_hash = fnv(name);
        let mut verbose = only.as_ref().map(|_| String::new());
        let mut h = 0u64;
        for k in 0..WORLDS * PER_WORLD {
            if k % PER_WORLD == 0 {
                pools.world_seed = mix(name_hash ^ (k / PER_WORLD + 1));
                for x in 0..SIZE {
                    for y in 0..SIZE {
                        for z in 0..SIZE {
                            let p = BASE.offset(x, y, z);
                            put(&mut level, p, pools.cell(p));
                        }
                    }
                }
            }
            let r0 = mix(name_hash.wrapping_add(k));
            let r1 = mix(r0);
            let r2 = mix(r1);
            let clicked = BASE.offset(
                3 + (r0 & 1) as i32,
                3 + ((r0 >> 1) & 1) as i32,
                3 + ((r0 >> 2) & 1) as i32,
            );
            let face = FACES[((r0 >> 3) % 6) as usize];
            let yaw = -180.0f32 + 22.5f32 * ((r0 >> 12) & 15) as f32;
            let pitch = -90.0f32 + 22.5f32 * ((r0 >> 16) % 9) as f32;
            let sneak = (r0 >> 40) & 1 != 0;
            let inside = (r0 >> 41) & 1 != 0;
            let (fx, fy, fz) = (
                FRACTION[(r1 & 7) as usize],
                FRACTION[((r1 >> 3) & 7) as usize],
                FRACTION[((r1 >> 6) & 7) as usize],
            );
            let c_sel = SEL[((r1 >> 9) % 12) as usize];
            let p_sel = SEL[((r1 >> 20) % 12) as usize];
            let beside = clicked.relative(face);
            let c_state = pools.special(c_sel, pools.cell(clicked), r2 & 0xFFFF_FFFF, &fixed);
            let p_state = pools.special(p_sel, pools.cell(beside), r2 >> 32, &fixed);
            put(&mut level, clicked, c_state);
            put(&mut level, beside, p_state);
            let axis = |step: i32, base: i32, f: f64| {
                f64::from(base)
                    + match step {
                        0 => f,
                        s if s > 0 => 1.0,
                        _ => 0.0,
                    }
            };
            let (hx, hy, hz) = (
                axis(face.get_step_x(), clicked.x(), fx),
                axis(face.get_step_y(), clicked.y(), fy),
                axis(face.get_step_z(), clicked.z(), fz),
            );
            let stack = b.item().map_or_else(ItemStack::empty, ItemStack::new);
            let hit = BlockHitResult {
                location: Vec3::new(hx, hy, hz),
                direction: face,
                block_pos: clicked,
                miss: false,
                inside,
                world_border_hit: false,
            };
            let player = ContextPlayer {
                entity_id: 0,
                y_rot: yaw,
                x_rot: pitch,
                secondary_use_active: sneak,
                can_use_game_master_blocks: false,
            };
            let replaceable = |level: &ServerLevel, c: &BlockPlaceContext| {
                level
                    .get_block_state(c.get_clicked_pos())
                    .can_be_replaced_by(c)
            };
            let ctx = BlockPlaceContext::new(
                UseOnContext::new(Some(player), InteractionHand::MainHand, stack, hit),
                |pos| level.get_block_state(pos),
            );
            level.tick_state.random.set_seed(mix(r2) as i64);
            let result = get_state_for_placement(b, &mut level, &ctx);
            let replace_clicked = ctx.replacing_clicked_on_block(false);
            let can_place = ctx.can_place(|c| replaceable(&level, c));
            h = mix(h ^ result.map_or(0, |s| u64::from(s.id()) + 1));
            h = mix(h ^ if replace_clicked { 1 } else { 2 });
            h = mix(h ^ if can_place { 1 } else { 2 });
            if let Some(v) = &mut verbose {
                write!(
                    v,
                    "{k} face={} yaw={yaw:?} pitch={pitch:?} sneak={} click={hx:?},{hy:?},{hz:?} inside={} clicked={} beside={} -> {} rc={} cp={}",
                    face.get_name(),
                    u8::from(sneak),
                    u8::from(inside),
                    describe(Some(c_state)),
                    describe(Some(p_state)),
                    describe(result),
                    u8::from(replace_clicked),
                    u8::from(can_place),
                )
                .expect("write to string");
                let q = ctx.get_clicked_pos();
                for d in FACES {
                    if d.get_axis().is_horizontal() {
                        write!(
                            v,
                            " {}={}",
                            d.get_name(),
                            describe(Some(level.get_block_state(q.relative(d))))
                        )
                        .expect("write to string");
                    }
                }
                v.push('\n');
            }
            put(&mut level, clicked, pools.cell(clicked));
            put(&mut level, beside, pools.cell(beside));
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
