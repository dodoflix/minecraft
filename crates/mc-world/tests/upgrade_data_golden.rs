//! Tests for `mc_world::upgrade_data`: NBT (golden vs the real vanilla class), fixers (golden vs the real
//! `UpgradeData.BlockFixers`), and the `upgrade(chunk)` control flow against a mock level.
//! Generators + fixtures: `fixtures/ticks/SavedTickGolden.java`, `fixtures/upgrade_data/FixerGolden.java`.

use mc_core::direction::{Direction, Direction8};
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::Fluid;
use mc_nbt::snbt;
use mc_nbt::{CompoundTag, Tag};
use mc_world::ticks::TickPriority;
use mc_world::upgrade_data::*;
use std::collections::{BTreeSet, HashMap, HashSet};

const SAVED_TICK_GOLDEN: &str = include_str!("fixtures/ticks/saved_tick_golden.txt");
const FIXER_GOLDEN: &str = include_str!("fixtures/upgrade_data/fixer_golden.txt");

fn compound(s: &str) -> CompoundTag {
    snbt::parse_compound(s).unwrap_or_else(|e| panic!("bad snbt {s}: {}", e.message()))
}

#[test]
fn upgrade_data_nbt_matches_vanilla() {
    let mut checked = 0;
    for line in SAVED_TICK_GOLDEN.lines().filter(|l| l.starts_with("UPGRADE\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let (min_y, height): (i32, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap());
        let data = UpgradeData::parse(&compound(f[3]), min_y >> 4, (min_y + height - 1) >> 4);
        assert_eq!(data.write(), compound(f[4]), "write: {line}");
        assert_eq!(data.is_empty(), f[5] == "true", "isEmpty: {line}");
        assert_eq!(data.copy().write(), compound(f[6]), "copy: {line}");
        // what we write must read back to the same data
        let again = UpgradeData::parse(&data.write(), min_y >> 4, (min_y + height - 1) >> 4);
        assert_eq!(again.write(), data.write(), "reparse: {line}");
        checked += 1;
    }
    assert_eq!(checked, 3 * 22);
}

#[test]
fn empty_upgrade_data() {
    let e = UpgradeData::empty();
    assert!(e.is_empty());
    assert_eq!(e.write(), CompoundTag::new().with("Sides", Tag::Byte(0)));
    assert_eq!(UpgradeData::EMPTY.copy(), e);
}

#[test]
fn block_fixer_map_matches_vanilla() {
    let mut n = 0;
    for line in FIXER_GOLDEN.lines().filter(|l| l.starts_with("FIXMAP\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        let block = Block::by_name(f[1]).unwrap();
        let name = match BlockFixer::for_block(block) {
            BlockFixer::Blacklist => "BLACKLIST",
            BlockFixer::Default => "DEFAULT",
            BlockFixer::Chest => "CHEST",
            BlockFixer::Leaves => "LEAVES",
            BlockFixer::StemBlock => "STEM_BLOCK",
        };
        assert_eq!(name, f[2], "{}", f[1]);
        n += 1;
    }
    assert_eq!(n, Block::iter().len());
}

/// Records what a fixer asks of the level; `update_shape` is the identity (stone-like behavior).
#[derive(Default)]
struct Mock {
    states: HashMap<BlockPos, BlockState>,
    default_state: Option<BlockState>,
    fluids: HashMap<BlockPos, Fluid>,
    events: Vec<String>,
    shape_calls: Vec<(BlockPos, Direction)>,
    /// `(state block, direction)` -> replacement for the DEFAULT fixer's `updateShape`
    replace: Option<fn(BlockState, Direction) -> BlockState>,
    min_y: i32,
    max_y: i32,
    chunk: ChunkPos,
    section_states: HashMap<(usize, i32), BlockState>,
    block_ticks: Vec<(BlockPos, Block, i32, TickPriority)>,
    fluid_ticks: Vec<(BlockPos, Fluid, i32, TickPriority)>,
}

impl UpgradeChunk for Mock {
    fn get_pos(&self) -> ChunkPos {
        self.chunk
    }
    fn get_min_y(&self) -> i32 {
        self.min_y
    }
    fn get_max_y(&self) -> i32 {
        self.max_y
    }
    fn get_section_state(&self, section_index: usize, index: i32) -> Option<BlockState> {
        if !(0..4096).contains(&index) {
            return None;
        }
        self.section_states.get(&(section_index, index)).copied()
    }
    fn get_block_state(&self, pos: &BlockPos) -> BlockState {
        self.states.get(pos).copied().or(self.default_state).unwrap_or(Block::AIR.default_state())
    }
    fn get_fluid_type(&self, pos: &BlockPos) -> Fluid {
        self.fluids.get(pos).copied().unwrap_or(Fluid::EMPTY)
    }
    fn set_block(&mut self, pos: &BlockPos, state: BlockState, flags: i32, update_limit: i32) -> bool {
        assert_eq!(update_limit, 512);
        self.events.push(format!("setBlock({},{},{},{state},{flags})", pos.x(), pos.y(), pos.z()));
        self.states.insert(*pos, state);
        true
    }
    fn destroy_block(&mut self, pos: &BlockPos, drop_resources: bool, update_limit: i32) -> bool {
        assert_eq!(update_limit, 512);
        self.events.push(format!("destroyBlock({},{},{},{drop_resources})", pos.x(), pos.y(), pos.z()));
        self.states.insert(*pos, Block::AIR.default_state());
        true
    }
    fn update_shape(&mut self, state: BlockState, direction: Direction, pos: &BlockPos, _n: &BlockPos, _ns: BlockState) -> BlockState {
        self.shape_calls.push((*pos, direction));
        self.replace.map_or(state, |f| f(state, direction))
    }
    fn swap_chest_contents(&mut self, a: &BlockPos, b: &BlockPos) {
        self.events.push(format!("getBE({},{},{});getBE({},{},{})", a.x(), a.y(), a.z(), b.x(), b.y(), b.z()));
    }
    fn schedule_block_tick(&mut self, pos: &BlockPos, block: Block, delay: i32, priority: TickPriority) {
        self.block_ticks.push((*pos, block, delay, priority));
    }
    fn schedule_fluid_tick(&mut self, pos: &BlockPos, fluid: Fluid, delay: i32, priority: TickPriority) {
        self.fluid_ticks.push((*pos, fluid, delay, priority));
    }
}

#[test]
fn chest_and_stem_fixers_match_vanilla() {
    // Vanilla lists only the (state, direction, neighbour) combinations where the result differs, events were
    // recorded or an exception was thrown; every other combination must be a no-op with no events.
    let mut expected: HashMap<(String, String, String), (String, String)> = HashMap::new();
    for line in FIXER_GOLDEN.lines().filter(|l| l.starts_with("CHEST\t") || l.starts_with("STEM\t")) {
        let f: Vec<&str> = line.split('\t').collect();
        expected.insert((f[1].into(), f[2].into(), f[3].into()), (f[4].into(), f[5].into()));
    }
    let pos = BlockPos::new(5, 64, 5);
    let others: Vec<BlockState> =
        [Block::STONE, Block::PUMPKIN, Block::MELON, Block::CARVED_PUMPKIN, Block::ATTACHED_PUMPKIN_STEM, Block::AIR].map(|b| b.default_state()).to_vec();
    let mut seen = 0;
    for (which, blocks) in [("CHEST", [Block::CHEST, Block::TRAPPED_CHEST]), ("STEM", [Block::PUMPKIN_STEM, Block::MELON_STEM])] {
        let states: Vec<BlockState> = blocks.iter().flat_map(|b| b.states()).collect();
        let mut neighbours: Vec<BlockState> = if which == "CHEST" {
            states.iter().copied().filter(|s| !s.to_string().contains("waterlogged=true")).collect()
        } else {
            vec![]
        };
        neighbours.extend(others.iter().copied());
        if which == "STEM" {
            neighbours.extend(states.iter().copied());
        }
        for &state in &states {
            for &nb in &neighbours {
                for d in Direction::ALL {
                    let mut mock = Mock::default();
                    let npos = pos.relative(d);
                    let mut leaves = LeavesQueue::default();
                    let res = BlockFixer::for_block(state.block()).update_shape(state, d, nb, &mut mock, &pos, &npos, &mut leaves);
                    let key = (state.to_string(), d.get_name().to_string(), nb.to_string());
                    let actual = match &res {
                        Ok(s) => (s.to_string(), mock.events.join(";")),
                        Err(_) => ("EXC:java.lang.IllegalArgumentException".to_string(), String::new()),
                    };
                    match expected.get(&key) {
                        Some(e) => {
                            assert_eq!(&actual, e, "{key:?}");
                            seen += 1;
                        }
                        None => assert_eq!(actual, (state.to_string(), String::new()), "unlisted combination must be a no-op: {key:?}"),
                    }
                }
            }
        }
    }
    assert_eq!(seen, expected.len());
}

fn stone() -> BlockState {
    Block::STONE.default_state()
}

fn mock(chunk: ChunkPos) -> Mock {
    Mock { default_state: Some(stone()), min_y: 0, max_y: 15, chunk, ..Default::default() }
}

fn side_data(sides: &[Direction8]) -> UpgradeData {
    let bits: i32 = sides.iter().map(|d| 1 << *d as i32).sum();
    UpgradeData::parse(&CompoundTag::new().with("Sides", Tag::Int(bits)), 0, 0)
}

#[test]
fn upgrade_sides_visit_vanilla_regions() {
    // chunk (2, -1): block x 32..=47, z -16..=-1; Direction8 order N, NE, E, SE, S, SW, W, NW
    let cases: [(Direction8, (i32, i32), (i32, i32)); 8] = [
        (Direction8::North, (33, 46), (-16, -16)),
        (Direction8::NorthEast, (47, 47), (-16, -16)),
        (Direction8::East, (47, 47), (-15, -2)),
        (Direction8::SouthEast, (47, 47), (-1, -1)),
        (Direction8::South, (33, 46), (-1, -1)),
        (Direction8::SouthWest, (32, 32), (-1, -1)),
        (Direction8::West, (32, 32), (-15, -2)),
        (Direction8::NorthWest, (32, 32), (-16, -16)),
    ];
    for (d, (x0, x1), (z0, z1)) in cases {
        let mut m = mock(ChunkPos::new(2, -1));
        let mut data = side_data(&[d]);
        assert_eq!(data.sides(), vec![d]);
        data.upgrade(&mut m).unwrap();
        assert!(data.is_empty(), "{d:?}: side consumed");
        let visited: BTreeSet<(i32, i32, i32)> = m.shape_calls.iter().map(|(p, _)| (p.x(), p.y(), p.z())).collect();
        let want: BTreeSet<(i32, i32, i32)> =
            (x0..=x1).flat_map(|x| (0..=15).flat_map(move |y| (z0..=z1).map(move |z| (x, y, z)))).collect();
        assert_eq!(visited, want, "{d:?}");
        assert_eq!(m.shape_calls.len(), want.len() * 6, "{d:?}: every position is updated from all 6 directions");
        // order: x fastest, then y, then z (BlockPos.betweenClosed); directions in Direction.values() order
        let first: Vec<Direction> = m.shape_calls[..6].iter().map(|(_, d)| *d).collect();
        assert_eq!(first, Direction::ALL);
    }
}

#[test]
fn upgrade_inside_decodes_indices_and_updates_or_destroys() {
    // index (y << 8 | z << 4 | x) in section 1 (block y 16..32) of chunk (-1, 3)
    let tag = compound("{Indices:{\"1\":[I;0,4095,291]}}");
    let mut data = UpgradeData::parse(&tag, 0, 1);
    assert!(!data.is_empty());
    let mut m = mock(ChunkPos::new(-1, 3));
    m.max_y = 31;
    for c in [0, 4095, 291] {
        m.section_states.insert((1, c), stone());
    }
    // the middle block becomes air (destroyed with drops), the others dirt (setBlock 18)
    m.replace = Some(|s, d| if d == Direction::Down { Block::DIRT.default_state() } else { s });
    data.upgrade(&mut m).unwrap();
    assert!(data.is_empty());
    let mut centres: Vec<(i32, i32, i32)> = vec![];
    for (p, d) in &m.shape_calls {
        if *d == Direction::Down {
            centres.push((p.x(), p.y(), p.z()));
        }
    }
    // 4095: x=15,y=15,z=15; 291 = 0x123: y=1, z=2, x=3
    assert_eq!(centres, vec![(-16, 16, 48), (-1, 31, 63), (-13, 17, 50)]);
    assert_eq!(m.events.len(), 3);
    assert_eq!(m.events[0], format!("setBlock(-16,16,48,{},18)", Block::DIRT.default_state()));

    // new state is air: destroyBlock(pos, true, null, 512)
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;5]}}"), 0, 0);
    let mut m = mock(ChunkPos::new(0, 0));
    m.section_states.insert((0, 5), stone());
    m.replace = Some(|_, _| Block::AIR.default_state());
    data.upgrade(&mut m).unwrap();
    assert_eq!(m.events, vec!["destroyBlock(5,0,0,true)"]);
}

#[test]
fn upgrade_inside_out_of_range_index_is_an_error() {
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;4096]}}"), 0, 0);
    let mut m = mock(ChunkPos::new(0, 0));
    assert!(data.upgrade(&mut m).is_err());
}

#[test]
fn upgrade_schedules_neighbor_ticks_resolving_air_and_empty() {
    let tag = compound(
        "{neighbor_block_ticks:[{i:\"minecraft:air\",x:1,y:2,z:3,t:4,p:-1},{i:\"minecraft:dirt\",x:5,y:6,z:7,t:8,p:9}],\
         neighbor_fluid_ticks:[{i:\"minecraft:empty\",x:1,y:2,z:3,t:4,p:1},{i:\"minecraft:lava\",x:5,y:6,z:7,t:8,p:0}]}",
    );
    let mut data = UpgradeData::parse(&tag, 0, 0);
    assert!(data.is_empty(), "ticks alone do not make UpgradeData non-empty");
    let mut m = mock(ChunkPos::new(0, 0));
    m.states.insert(BlockPos::new(1, 2, 3), Block::SAND.default_state());
    m.fluids.insert(BlockPos::new(1, 2, 3), Fluid::WATER);
    data.upgrade(&mut m).unwrap();
    assert_eq!(
        m.block_ticks,
        vec![
            (BlockPos::new(1, 2, 3), Block::SAND, 4, TickPriority::High),
            (BlockPos::new(5, 6, 7), Block::DIRT, 8, TickPriority::ExtremelyLow)
        ]
    );
    assert_eq!(
        m.fluid_ticks,
        vec![(BlockPos::new(1, 2, 3), Fluid::WATER, 4, TickPriority::Low), (BlockPos::new(5, 6, 7), Fluid::LAVA, 8, TickPriority::Normal)]
    );
    // upgrade does not consume the ticks
    assert_eq!(data.neighbor_block_ticks().len(), 2);
}

#[test]
fn blacklisted_blocks_are_not_updated_and_default_fixer_calls_update_shape() {
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;0]}}"), 0, 0);
    let mut m = mock(ChunkPos::new(0, 0));
    m.default_state = Some(Block::SAND.default_state());
    m.section_states.insert((0, 0), Block::SAND.default_state());
    data.upgrade(&mut m).unwrap();
    assert!(m.shape_calls.is_empty());
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;0]}}"), 0, 0);
    m.section_states.insert((0, 0), stone());
    data.upgrade(&mut m).unwrap();
    assert_eq!(m.shape_calls.iter().map(|(_, d)| *d).collect::<Vec<_>>(), Direction::ALL);
}

#[test]
fn indices_outside_chunk_columns_never_update() {
    // `upgradeInside` only updates positions inside the chunk's column range (always true for 0..16 offsets)
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;0,17,4095]}}"), 0, 0);
    let mut m = mock(ChunkPos::new(0, 0));
    for c in [0, 17, 4095] {
        m.section_states.insert((0, c), stone());
    }
    data.upgrade(&mut m).unwrap();
    let centres: HashSet<BlockPos> = m.shape_calls.iter().map(|(p, _)| *p).collect();
    assert_eq!(centres.len(), 3);
}

#[test]
fn leaves_fixer_queues_changed_shapes_and_propagates_distance() {
    // Vanilla's leaves `updateShape` always returns the state, so this queue is dormant there; the port keeps the
    // logic: a changed state's distance picks the queue and `processChunk` lowers distances outward from it.
    let leaves = Block::OAK_LEAVES.default_state(); // distance=7
    let mut data = UpgradeData::parse(&compound("{Indices:{\"0\":[I;0]}}"), 0, 0);
    let mut m = mock(ChunkPos::new(0, 0));
    m.default_state = Some(leaves);
    m.section_states.insert((0, 0), leaves);
    m.replace = Some(|s, d| if d == Direction::Down { s.set(mc_data::props::DISTANCE, 3) } else { s });
    data.upgrade(&mut m).unwrap();
    // `updateShape` returned the unchanged state: nothing set; the queued position (distance 3) is then lowered to 3
    assert_eq!(m.events[0], format!("setBlock(0,0,0,{},18)", leaves.set(mc_data::props::DISTANCE, 3)));
    assert!(m.events.iter().any(|e| e.contains("distance=5")), "{:?}", m.events);
    assert!(m.events.iter().all(|e| e.ends_with(",18)")));
}
