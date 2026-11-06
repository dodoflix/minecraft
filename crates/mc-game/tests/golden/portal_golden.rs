//! Golden test of `PortalShape`, `PortalForcer`, `BlockUtil` and `BaseFireBlock.onPlace` against vanilla.
//!
//! The fixtures (`tests/fixtures/portal/*.txt`, producer `tests/oracle/PortalOracle.java`, run with
//! `scripts/entity-oracle.sh crates/mc-game/tests/oracle/PortalOracle.java shapes|forcer|misc [N] > fixture`) are scripts with the
//! vanilla results: `> command` lines are replayed against void levels (overworld + nether sized like the real ones), `< result`
//! lines are what vanilla printed after the command. The command list is in the header of the Java file.

use crate::common::feature_harness::{parse_state, state_string};
use mc_core::direction::{Axis, Direction};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game::registry::typed::dimension_type::DimensionType;
use mc_game::server::level::ServerLevel;
use mc_game::util::block_util;
use mc_game::world::entity::EntityDimensions;
use mc_game::world::level::flags::{UPDATE_CLIENTS, UPDATE_KNOWN_SHAPE, UPDATE_SKIP_ON_PLACE};
use mc_game::world::level::portal::PortalShape;
use mc_game::world::level::{BlockGetter, LevelAccessor, LevelHeight};
use std::collections::HashMap;
use std::sync::Arc;

const QUIET: u32 = UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE | UPDATE_SKIP_ON_PLACE;

fn nether_type() -> DimensionType {
    use mc_game::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
    struct L;
    impl RegistryLookup for L {
        fn contains(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
        fn tag_exists(&self, _: &str, _: &Identifier) -> Lookup {
            Lookup::Yes
        }
    }
    let json = serde_json::json!({
        "has_skylight": false, "has_ceiling": true, "has_ender_dragon_fight": false, "coordinate_scale": 8.0,
        "min_y": 0, "height": 256, "logical_height": 128, "infiniburn": "#minecraft:infiniburn_nether",
        "ambient_light": 0.1, "monster_spawn_light_level": 7, "monster_spawn_block_light_limit": 15,
    });
    let l = L;
    DimensionType::from_json(&json, &RegistryDecodeContext::new(&l)).expect("nether dimension type")
}

struct Replay {
    overworld: ServerLevel,
    nether: ServerLevel,
    in_nether: bool,
}

impl Replay {
    fn new() -> Self {
        let overworld = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        let mut nether = ServerLevel::detached_with_height(
            Identifier::parse("minecraft:the_nether").unwrap(),
            LevelHeight {
                min_y: 0,
                height: 256,
            },
            32,
        );
        nether.dimension_type = Some(Arc::new(nether_type()));
        Replay {
            overworld,
            nether,
            in_nether: false,
        }
    }

    fn level(&mut self) -> &mut ServerLevel {
        if self.in_nether {
            &mut self.nether
        } else {
            &mut self.overworld
        }
    }
}

fn pos(t: &[&str], i: usize) -> BlockPos {
    BlockPos::new(
        t[i].parse().unwrap(),
        t[i + 1].parse().unwrap(),
        t[i + 2].parse().unwrap(),
    )
}

fn axis(s: &str) -> Axis {
    match s {
        "x" => Axis::X,
        "z" => Axis::Z,
        _ => panic!("axis {s}"),
    }
}

fn shape_line(s: &PortalShape) -> String {
    format!(
        "axis={} num={} w={} h={} bl={},{},{}",
        s.axis.get_name(),
        s.num_portal_blocks,
        s.width,
        s.height,
        s.bottom_left.x(),
        s.bottom_left.y(),
        s.bottom_left.z()
    )
}

fn f64_text(v: f64) -> String {
    mc_core::math::java::java_double_to_string(v)
}

/// Executes one command and returns the result lines the Java oracle prints for it.
fn exec(r: &mut Replay, line: &str) -> Vec<String> {
    let t: Vec<&str> = line.split(' ').collect();
    let mut out = Vec::new();
    match t[0] {
        "dim" => r.in_nether = t[1] == "nether",
        "border" => {
            let b = r.level().get_world_border();
            if t[1] == "reset" {
                b.set_center(0.0, 0.0);
                b.set_size(5.999997E7);
            } else {
                b.set_center(t[1].parse().unwrap(), t[2].parse().unwrap());
                b.set_size(t[3].parse().unwrap());
            }
        }
        "fill" => {
            let c: Vec<i32> = (1..7).map(|i| t[i].parse().unwrap()).collect();
            let st = parse_state(t[7]);
            let level = r.level();
            for x in c[0].min(c[3])..=c[0].max(c[3]) {
                for y in c[1].min(c[4])..=c[1].max(c[4]) {
                    for z in c[2].min(c[5])..=c[2].max(c[5]) {
                        level.set_block(BlockPos::new(x, y, z), st, QUIET, 512);
                    }
                }
            }
        }
        "set2" => {
            r.level()
                .set_block(pos(&t, 1), parse_state(t[4]), QUIET, 512);
        }
        "set" => {
            r.level()
                .set_block_and_update(pos(&t, 1), parse_state(t[4]));
        }
        "any" => {
            let s = PortalShape::find_any_shape(&*r.level(), pos(&t, 1), axis(t[4]));
            out.push(format!(
                "A {} valid={} complete={}",
                shape_line(&s),
                s.is_valid(),
                s.is_complete()
            ));
        }
        "empty" | "emptyplace" => {
            let level = r.level();
            let s = PortalShape::find_empty_portal_shape(&*level, pos(&t, 1), axis(t[4]));
            out.push(format!(
                "E {}",
                s.as_ref().map_or("none".to_owned(), shape_line)
            ));
            if t[0] == "emptyplace" {
                if let Some(s) = s {
                    s.create_portal_blocks(level);
                }
            }
        }
        "digest" => {
            let c: Vec<i32> = (1..7).map(|i| t[i].parse().unwrap()).collect();
            let level = r.level();
            let mut hash: u64 = 0xcbf29ce484222325;
            let mut n = 0;
            let mut special = Vec::new();
            for x in c[0]..=c[3] {
                for y in c[1]..=c[4] {
                    for z in c[2]..=c[5] {
                        let st = level.get_block_state(BlockPos::new(x, y, z));
                        if st.is_air() {
                            continue;
                        }
                        n += 1;
                        let s = format!("{x} {y} {z} {}", state_string(st));
                        for b in s.bytes() {
                            hash ^= u64::from(b);
                            hash = hash.wrapping_mul(0x100000001b3);
                        }
                        let name = st.block().data().name.trim_start_matches("minecraft:");
                        if matches!(
                            name,
                            "obsidian" | "nether_portal" | "fire" | "soul_fire" | "crying_obsidian"
                        ) {
                            special.push(format!("O {s}"));
                        }
                    }
                }
            }
            out.push(format!("D n={n} hash={hash:x}"));
            out.extend(special);
        }
        "find" => {
            let found = r
                .level()
                .get_portal_forcer()
                .find_closest_portal_position(pos(&t, 1), t[4] == "true");
            out.push(match found {
                Some(p) => format!("F {} {} {}", p.x(), p.y(), p.z()),
                None => "F none".to_owned(),
            });
        }
        "create" => {
            let created = r
                .level()
                .get_portal_forcer()
                .create_portal(pos(&t, 1), axis(t[4]));
            out.push(match created {
                Some(f) => format!(
                    "C {} {} {} {} {}",
                    f.min_corner.x(),
                    f.min_corner.y(),
                    f.min_corner.z(),
                    f.axis1_size,
                    f.axis2_size
                ),
                None => "C none".to_owned(),
            });
        }
        "rect" => {
            let level = r.level();
            let p = pos(&t, 1);
            let st = level.get_block_state(p);
            let f = block_util::get_largest_rectangle_around(
                p,
                axis(t[4]),
                21,
                Axis::Y,
                21,
                &mut |q| level.get_block_state(q) == st,
            );
            out.push(format!(
                "R {} {} {} {} {}",
                f.min_corner.x(),
                f.min_corner.y(),
                f.min_corner.z(),
                f.axis1_size,
                f.axis2_size
            ));
        }
        "maxrect" => {
            let cols: Vec<i32> = t[1..].iter().map(|s| s.parse().unwrap()).collect();
            let (b, h) = block_util::get_max_rectangle_location(&cols);
            out.push(format!("M {} {} {}", b.min, b.max, h));
        }
        "relpos" => {
            let i = |k: usize| t[k].parse::<i32>().unwrap();
            let f = block_util::FoundRectangle {
                min_corner: BlockPos::new(i(1), i(2), i(3)),
                axis1_size: i(4),
                axis2_size: i(5),
            };
            let d = |k: usize| t[k].parse::<f64>().unwrap();
            let dims = EntityDimensions::scalable(t[10].parse().unwrap(), t[11].parse().unwrap());
            let v = PortalShape::get_relative_position(
                &f,
                axis(t[6]),
                Vec3::new(d(7), d(8), d(9)),
                &dims,
            );
            out.push(format!(
                "V {} {} {}",
                f64_text(v.x),
                f64_text(v.y),
                f64_text(v.z)
            ));
        }
        other => panic!("unknown command {other}"),
    }
    out
}

/// Numeric tokens compare by value (the Java side prints `Double.toString`), everything else textually.
fn same(a: &str, b: &str) -> bool {
    a == b
        || (a.split(' ').count() == b.split(' ').count()
            && a.split(' ').zip(b.split(' ')).all(|(x, y)| x == y || matches!((x.parse::<f64>(), y.parse::<f64>()), (Ok(p), Ok(q)) if p.to_bits() == q.to_bits())))
}

fn replay(fixture: &str) {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/portal/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("{fixture}: {e}"));
    let lines: Vec<&str> = text.lines().collect();
    let mut r = Replay::new();
    let mut failures = Vec::new();
    let mut i = 0;
    let mut commands = 0;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        if let Some(cmd) = l.strip_prefix("> ") {
            let mut expected = Vec::new();
            while i < lines.len() && lines[i].starts_with("< ") {
                expected.push(lines[i].strip_prefix("< ").unwrap());
                i += 1;
            }
            let actual = exec(&mut r, cmd);
            commands += 1;
            let ok = actual.len() == expected.len()
                && actual.iter().zip(&expected).all(|(a, e)| same(a, e));
            if !ok {
                failures.push(format!(
                    "line {}: `{cmd}`\n  vanilla: {expected:?}\n  ours:    {actual:?}",
                    i
                ));
                if failures.len() >= 8 {
                    break;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{fixture}: {} mismatching commands (of {commands} run so far):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(commands > 0);
    let _ = (Block::AIR, Direction::Up);
}

#[test]
fn portal_shapes() {
    replay("shapes.txt");
}

#[test]
fn portal_misc() {
    replay("misc.txt");
}

#[test]
fn portal_forcer() {
    replay("forcer.txt");
}
