//! Golden tests for `mc_world::ticks` against the real vanilla classes (generators + fixtures in `fixtures/ticks/`).

use mc_core::bounding_box::BoundingBox;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::vec3i::Vec3i;
use mc_data::block::Block;
use mc_nbt::snbt;
use mc_nbt::{CompoundTag, Tag};
use mc_world::ticks::*;
use std::collections::HashMap;

const SAVED_TICK_GOLDEN: &str = include_str!("fixtures/ticks/saved_tick_golden.txt");
const TICKS_GOLDEN: &str = include_str!("fixtures/ticks/ticks_golden.txt");

fn block(name: &str) -> Block {
    Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"))
}

fn st(t: &ScheduledTick<Block>) -> String {
    format!(
        "{},{},{},{},{},{},{}",
        t.ty.name().trim_start_matches("minecraft:"),
        t.pos.x(),
        t.pos.y(),
        t.pos.z(),
        t.trigger_tick,
        t.priority.get_value(),
        t.sub_tick_order
    )
}

fn st_opt(t: Option<ScheduledTick<Block>>) -> String {
    t.map_or("null".to_string(), |t| st(&t))
}

fn sv(t: &SavedTick<Block>) -> String {
    format!("{},{},{},{},{},{}", t.ty.name().trim_start_matches("minecraft:"), t.pos.x(), t.pos.y(), t.pos.z(), t.delay, t.priority.get_value())
}

fn join<I: IntoIterator<Item = String>>(it: I) -> String {
    it.into_iter().collect::<Vec<_>>().join(";")
}

fn parse_saved(list: &str) -> Vec<SavedTick<Block>> {
    if list.is_empty() {
        return vec![];
    }
    list.split(';')
        .map(|e| {
            let f: Vec<&str> = e.split(',').collect();
            SavedTick {
                ty: block(f[0]),
                pos: BlockPos::new(f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap()),
                delay: f[4].parse().unwrap(),
                priority: TickPriority::by_value(f[5].parse().unwrap()),
            }
        })
        .collect()
}

fn n<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn all(c: &LevelChunkTicks<Block>) -> String {
    format!("{}|{}", c.count(), join(c.get_all().map(st)))
}

enum State {
    Chunk(LevelChunkTicks<Block>),
    Proto(ProtoChunkTicks<Block>),
    Level(LevelTicks<Block>),
}

#[test]
fn tick_containers_match_vanilla() {
    let mut states: HashMap<String, State> = HashMap::new();
    let mut checked = 0usize;
    for (ln, line) in TICKS_GOLDEN.lines().enumerate() {
        let mut parts = line.splitn(3, '\t');
        let (scen, op, expected) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap_or(""));
        let f: Vec<&str> = op.split(' ').collect();
        let actual = match (f[0], states.get_mut(scen)) {
            ("NEW", None) if scen.starts_with("CHUNK") => {
                states.insert(scen.to_string(), State::Chunk(LevelChunkTicks::new()));
                "0".to_string()
            }
            ("NEW", None) => {
                states.insert(scen.to_string(), State::Level(LevelTicks::new()));
                "0".to_string()
            }
            ("NEWPENDING", None) => {
                let c = LevelChunkTicks::from_pending(parse_saved(f.get(1).copied().unwrap_or("")));
                let r = c.count().to_string();
                states.insert(scen.to_string(), State::Chunk(c));
                r
            }
            ("LOAD", None) => {
                let p = ProtoChunkTicks::load(&parse_saved(f.get(1).copied().unwrap_or("")));
                let r = format!("{}|{}", p.count(), join(p.scheduled_ticks().iter().map(sv)));
                states.insert(scen.to_string(), State::Proto(p));
                r
            }
            (_, Some(State::Chunk(c))) => match f[0] {
                "S" => {
                    c.schedule(ScheduledTick {
                        ty: block(f[1]),
                        pos: BlockPos::new(n(f[2]), n(f[3]), n(f[4])),
                        trigger_tick: n(f[5]),
                        priority: TickPriority::by_value(n(f[6])),
                        sub_tick_order: n(f[7]),
                    });
                    c.count().to_string()
                }
                "H" => c.has_scheduled_tick(&BlockPos::new(n(f[2]), n(f[3]), n(f[4])), block(f[1])).to_string(),
                "K" => st_opt(c.peek().copied()),
                "P" => st_opt(c.poll()),
                "R" => {
                    let (m, rem): (i32, i32) = (n(f[1]), n(f[2]));
                    c.remove_if(|t| (t.pos.x() + t.pos.y() + t.pos.z()).rem_euclid(m) == rem);
                    all(c)
                }
                "U" => {
                    c.unpack(n(f[1]));
                    all(c)
                }
                "A" => join(c.pack(n(f[1])).iter().map(sv)),
                "G" => all(c),
                o => panic!("op {o}"),
            },
            (_, Some(State::Proto(p))) => match f[0] {
                "S" => {
                    p.schedule(ScheduledTick {
                        ty: block(f[1]),
                        pos: BlockPos::new(n(f[2]), n(f[3]), n(f[4])),
                        trigger_tick: n(f[5]),
                        priority: TickPriority::by_value(n(f[6])),
                        sub_tick_order: 0,
                    });
                    "ok".to_string()
                }
                "H" => p.has_scheduled_tick(&BlockPos::new(n(f[2]), n(f[3]), n(f[4])), block(f[1])).to_string(),
                "A" => format!("{}|{}", p.count(), join(p.pack(n(f[1])).iter().map(sv))),
                o => panic!("op {o}"),
            },
            (_, Some(State::Level(lt))) => match f[0] {
                "ADD" => {
                    let mut c = LevelChunkTicks::from_pending(parse_saved(f.get(4).copied().unwrap_or("")));
                    c.unpack(n(f[3]));
                    lt.add_container(ChunkPos::new(n(f[1]), n(f[2])), c);
                    lt.count().to_string()
                }
                "REM" => {
                    lt.remove_container(ChunkPos::new(n(f[1]), n(f[2])));
                    lt.count().to_string()
                }
                "S" => {
                    lt.schedule(ScheduledTick {
                        ty: block(f[1]),
                        pos: BlockPos::new(n(f[2]), n(f[3]), n(f[4])),
                        trigger_tick: n(f[5]),
                        priority: TickPriority::by_value(n(f[6])),
                        sub_tick_order: n(f[7]),
                    });
                    lt.count().to_string()
                }
                "H" => lt.has_scheduled_tick(&BlockPos::new(n(f[2]), n(f[3]), n(f[4])), block(f[1])).to_string(),
                "T" => {
                    let (cur, max, modulus): (i64, usize, i32) = (n(f[1]), n(f[2]), n(f[3]));
                    let mut ran = vec![];
                    lt.tick(
                        cur,
                        max,
                        |p| modulus == 0 || (ChunkPos::get_x(p).wrapping_add(ChunkPos::get_z(p))).rem_euclid(modulus) != 0,
                        |pos, ty| ran.push(format!("{},{},{},{}", ty.name().trim_start_matches("minecraft:"), pos.x(), pos.y(), pos.z())),
                    );
                    format!("{}|{}", ran.join(";"), lt.count())
                }
                "CL" => {
                    lt.clear_area(&BoundingBox::new(n(f[1]), 0, n(f[2]), n(f[3]), 2, n(f[4])));
                    lt.count().to_string()
                }
                "CP" => {
                    lt.copy_area(&BoundingBox::new(n(f[1]), 0, n(f[2]), n(f[3]), 2, n(f[4])), &Vec3i::new(n(f[5]), 0, n(f[6])));
                    lt.count().to_string()
                }
                o => panic!("op {o}"),
            },
            (o, _) => panic!("line {ln}: op {o} without state"),
        };
        assert_eq!(actual, expected, "line {}: {scen} {op}", ln + 1);
        checked += 1;
    }
    assert!(checked > 20_000, "fixture unexpectedly small: {checked}");
}

#[test]
fn tick_priority_by_value() {
    assert_eq!(TickPriority::ALL.map(TickPriority::get_value), [-3, -2, -1, 0, 1, 2, 3]);
    for v in -10..=10 {
        let p = TickPriority::by_value(v);
        assert_eq!(p.get_value(), v.clamp(-3, 3));
    }
    assert_eq!(TickPriority::by_value(i32::MIN), TickPriority::ExtremelyHigh);
    assert_eq!(TickPriority::by_value(i32::MAX), TickPriority::ExtremelyLow);
}

fn compound(s: &str) -> CompoundTag {
    snbt::parse_compound(s).unwrap_or_else(|e| panic!("bad snbt {s}: {}", e.message()))
}

#[test]
fn saved_tick_codec_matches_vanilla() {
    let mut n_checked = 0;
    for line in SAVED_TICK_GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "BLOCK" | "FLUID" => {
                let key = if f[0] == "BLOCK" { "block_ticks" } else { "fluid_ticks" };
                let input = compound(&format!("{{{key}:{}}}", f[1]));
                let expected = snbt::parse(f[2]).unwrap();
                let actual = if f[0] == "BLOCK" {
                    SavedTick::<Block>::write_list(&SavedTick::<Block>::read_list(&input, key))
                } else {
                    SavedTick::<mc_data::fluid::Fluid>::write_list(&SavedTick::<mc_data::fluid::Fluid>::read_list(&input, key))
                };
                assert_eq!(Tag::List(actual), expected, "{line}");
                n_checked += 1;
            }
            _ => {}
        }
    }
    assert!(n_checked > 35);
}

#[test]
fn filter_tick_list_for_chunk() {
    let t = |x, z| SavedTick::probe(mc_data::block::Block::STONE, BlockPos::new(x, 5, z));
    let list = [t(0, 0), t(15, 15), t(16, 0), t(-1, 0), t(-17, 31), t(3, -1)];
    let got = SavedTick::filter_tick_list_for_chunk(&list, ChunkPos::new(0, 0));
    assert_eq!(got, vec![t(0, 0), t(15, 15)]);
    let got = SavedTick::filter_tick_list_for_chunk(&list, ChunkPos::new(-2, 1));
    assert_eq!(got, vec![t(-17, 31)]);
}

#[test]
fn will_tick_this_tick_and_phases() {
    // `willTickThisTick` is only observable between collect and cleanup.
    let mut lt = LevelTicks::<Block>::new();
    let mut c = LevelChunkTicks::new();
    c.schedule(ScheduledTick::new(Block::STONE, BlockPos::new(1, 2, 3), 5, 0));
    c.schedule(ScheduledTick::new(Block::DIRT, BlockPos::new(1, 2, 3), 5, 1));
    lt.add_container(ChunkPos::new(0, 0), c);
    lt.collect_ticks(5, 10, |_| true);
    assert!(lt.will_tick_this_tick(&BlockPos::new(1, 2, 3), Block::STONE));
    assert!(!lt.will_tick_this_tick(&BlockPos::new(1, 2, 4), Block::STONE));
    assert_eq!(lt.poll_collected_tick(), Some((BlockPos::new(1, 2, 3), Block::STONE)));
    assert!(!lt.will_tick_this_tick(&BlockPos::new(1, 2, 3), Block::STONE));
    assert!(lt.will_tick_this_tick(&BlockPos::new(1, 2, 3), Block::DIRT));
    lt.cleanup_after_tick();
    assert!(!lt.will_tick_this_tick(&BlockPos::new(1, 2, 3), Block::DIRT));
}
