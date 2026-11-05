//! Golden tests for `mc_core::phys::shapes` against vanilla.
//!
//! `fixtures/shapes/shapes.script` is interpreted by both `ShapesOracle.java` (real vanilla classes, output
//! committed as `shapes.out`) and the interpreter below (the Rust port). The output lines must be identical.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use mc_core::direction::{Axis, Direction};
use mc_core::phys::shapes::{BitSetDiscreteVoxelShape, BooleanOp, Coords, DiscreteVoxelShape, Shapes, VoxelShape};
use mc_core::phys::{Vec3, AABB};
use mc_core::pos::BlockPos;

const SCRIPT: &str = include_str!("../fixtures/shapes/shapes.script");
const EXPECTED: &str = include_str!("../fixtures/shapes/shapes.out");
// The generator is committed next to its output; referencing it here keeps the pair honest.
const _ORACLE: &str = include_str!("../fixtures/shapes/ShapesOracle.java");

fn hx(d: f64) -> String {
    format!("{:x}", d.to_bits())
}

struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Hasher(0xcbf29ce484222325)
    }
    fn add(&mut self, v: u64) {
        self.0 = (self.0 ^ v).wrapping_mul(0x100000001b3);
    }
    fn add_f(&mut self, d: f64) {
        self.add(d.to_bits());
    }
    fn add_i(&mut self, i: i32) {
        self.add(i as i64 as u64);
    }
    fn hex(&self) -> String {
        format!("{:x}", self.0)
    }
}

fn op(n: &str) -> BooleanOp {
    match n {
        "FALSE" => BooleanOp::FALSE,
        "NOT_OR" => BooleanOp::NOT_OR,
        "ONLY_SECOND" => BooleanOp::ONLY_SECOND,
        "NOT_FIRST" => BooleanOp::NOT_FIRST,
        "ONLY_FIRST" => BooleanOp::ONLY_FIRST,
        "NOT_SECOND" => BooleanOp::NOT_SECOND,
        "NOT_SAME" => BooleanOp::NOT_SAME,
        "NOT_AND" => BooleanOp::NOT_AND,
        "AND" => BooleanOp::AND,
        "SAME" => BooleanOp::SAME,
        "SECOND" => BooleanOp::SECOND,
        "CAUSES" => BooleanOp::CAUSES,
        "FIRST" => BooleanOp::FIRST,
        "CAUSED_BY" => BooleanOp::CAUSED_BY,
        "OR" => BooleanOp::OR,
        "TRUE" => BooleanOp::TRUE,
        _ => panic!("bad op {n}"),
    }
}

fn dir(n: &str) -> Direction {
    match n {
        "DOWN" => Direction::Down,
        "UP" => Direction::Up,
        "NORTH" => Direction::North,
        "SOUTH" => Direction::South,
        "WEST" => Direction::West,
        "EAST" => Direction::East,
        _ => panic!("bad dir {n}"),
    }
}

fn axis(n: &str) -> Axis {
    match n {
        "X" => Axis::X,
        "Y" => Axis::Y,
        "Z" => Axis::Z,
        _ => panic!("bad axis {n}"),
    }
}

fn reference(r: &VoxelShape, ops: &[&VoxelShape]) -> String {
    if r.ptr_eq(&Shapes::block()) {
        return "block".into();
    }
    if r.ptr_eq(&Shapes::empty()) {
        return "empty".into();
    }
    for (i, o) in ops.iter().enumerate() {
        if r.ptr_eq(o) {
            return format!("op{i}");
        }
    }
    "new".into()
}

fn join_hex(it: impl Iterator<Item = f64>) -> String {
    it.map(hx).collect::<Vec<_>>().join(",")
}

fn dump(name: &str, s: &VoxelShape) -> String {
    let d = s.shape();
    let mut b = format!("dump {name} kind={}", if s.is_cube_voxel_shape() { "cube" } else { "array" });
    let (disc, words) = match &**d {
        DiscreteVoxelShape::BitSet(bs) => {
            let w = bs.bits();
            ("bitset", if w.is_empty() { "-".to_string() } else { w.iter().map(|w| format!("{w:x}")).collect::<Vec<_>>().join(",") })
        }
        DiscreteVoxelShape::Sub(_) => ("sub", "-".to_string()),
    };
    b += &format!(" disc={disc} size={},{},{}", d.get_x_size(), d.get_y_size(), d.get_z_size());
    b += &format!(" first={},{},{}", d.first_full(Axis::X), d.first_full(Axis::Y), d.first_full(Axis::Z));
    b += &format!(" last={},{},{}", d.last_full(Axis::X), d.last_full(Axis::Y), d.last_full(Axis::Z));
    b += &format!(" words={words} cells=");
    for x in 0..d.get_x_size() {
        for y in 0..d.get_y_size() {
            for z in 0..d.get_z_size() {
                b.push(if d.is_full(x, y, z) { '1' } else { '0' });
            }
        }
    }
    b += " ckind=";
    for a in Axis::VALUES {
        b.push(if s.get_coords(a).is_cube_point_range() { 'c' } else { 'a' });
    }
    for (n, a) in [("cx", Axis::X), ("cy", Axis::Y), ("cz", Axis::Z)] {
        b += &format!(" {n}={}", join_hex(s.get_coords(a).to_vec().into_iter()));
    }
    b
}

const VALS: [f64; 6] = [-0.3, 0.0, 0.2, 0.5, 0.8, 1.1];
const SIZES: [f64; 3] = [0.25, 0.6, 1.0];
const DELTAS: [f64; 9] = [-1.5, -0.5, -0.2, -0.05, 0.0, 0.05, 0.2, 0.5, 1.5];

fn run(script: &str) -> (Vec<String>, HashMap<String, VoxelShape>) {
    static HOOK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut shapes: HashMap<String, VoxelShape> = HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let snapshot = shapes.clone();
        let mut lines: Vec<String> = Vec::new();
        let mut defined: Option<(String, VoxelShape)> = None;
        let res = catch_unwind(AssertUnwindSafe(|| command(&t, &snapshot, &mut lines, &mut defined)));
        match res {
            Ok(()) => {
                out.extend(lines);
                if let Some((n, s)) = defined {
                    shapes.insert(n, s);
                }
            }
            Err(_) => out.push(format!("ERR {line}")),
        }
    }
    std::panic::set_hook(prev);
    (out, shapes)
}

fn get<'a>(shapes: &'a HashMap<String, VoxelShape>, n: &str) -> Option<&'a VoxelShape> {
    shapes.get(n)
}

fn f(s: &str) -> f64 {
    s.parse().unwrap()
}

fn command(t: &[&str], sh: &HashMap<String, VoxelShape>, out: &mut Vec<String>, defined: &mut Option<(String, VoxelShape)>) {
    let name = t.get(1).copied().unwrap_or("");
    macro_rules! shape {
        ($n:expr) => {
            match get(sh, $n) {
                Some(s) => s,
                None => {
                    out.push(format!("UNDEF {}", $n));
                    return;
                }
            }
        };
    }
    match t[0] {
        "empty" => *defined = Some((name.into(), Shapes::empty())),
        "block" => *defined = Some((name.into(), Shapes::block())),
        "infinity" => *defined = Some((name.into(), Shapes::infinity())),
        "box" | "create" => {
            let v: Vec<f64> = (0..6).map(|i| f(t[2 + i])).collect();
            let s = if t[0] == "box" {
                Shapes::box_(v[0], v[1], v[2], v[3], v[4], v[5])
            } else {
                Shapes::create_bounds(v[0], v[1], v[2], v[3], v[4], v[5])
            };
            *defined = Some((name.into(), s));
        }
        "or" => {
            let first = &sh[t[2]];
            let tail: Vec<VoxelShape> = t[3..].iter().map(|n| sh[*n].clone()).collect();
            *defined = Some((name.into(), Shapes::or_many(first, &tail)));
        }
        "join" | "joinu" => {
            let (a, b) = (&sh[t[2]], &sh[t[3]]);
            let r = if t[0] == "join" { Shapes::join(a, b, op(t[4])) } else { Shapes::join_unoptimized(a, b, op(t[4])) };
            out.push(format!("ref {name} {}", reference(&r, &[a, b])));
            *defined = Some((name.into(), r));
        }
        "move" => {
            let a = &sh[t[2]];
            let r = a.move_(f(t[3]), f(t[4]), f(t[5]));
            out.push(format!("ref {name} {}", reference(&r, &[a])));
            *defined = Some((name.into(), r));
        }
        "opt" | "single" => {
            let a = &sh[t[2]];
            let r = if t[0] == "opt" { a.optimize() } else { a.single_encompassing() };
            out.push(format!("ref {name} {}", reference(&r, &[a])));
            *defined = Some((name.into(), r));
        }
        "face" => {
            let a = &sh[t[2]];
            let r = a.get_face_shape(dir(t[3]));
            let r2 = a.get_face_shape(dir(t[3]));
            out.push(format!("ref {name} {} cached={}", reference(&r, &[a]), r.ptr_eq(&r2)));
            *defined = Some((name.into(), r));
        }
        "slice" => {
            *defined = Some((name.into(), sh[t[2]].slice(axis(t[3]), t[4].parse().unwrap())));
        }
        "findbits" => out.push(format!("findbits {} {} {}", t[1], t[2], Shapes::find_bits(f(t[1]), f(t[2])))),
        "dump" => {
            let s = shape!(name);
            out.push(dump(name, s));
        }
        "aabbs" => {
            let s = shape!(name);
            let mut b = format!("aabbs {name} ");
            for a in s.to_aabbs() {
                b += &format!("{},{},{},{},{},{};", hx(a.min_x), hx(a.min_y), hx(a.min_z), hx(a.max_x), hx(a.max_y), hx(a.max_z));
            }
            out.push(b);
        }
        "edges" => {
            let s = shape!(name);
            let mut h = Hasher::new();
            let mut n = 0;
            s.for_all_edges(|x1, y1, z1, x2, y2, z2| {
                n += 1;
                for v in [x1, y1, z1, x2, y2, z2] {
                    h.add_f(v);
                }
            });
            out.push(format!("edges {name} {n} {}", h.hex()));
        }
        "grid" => {
            let s = shape!(name);
            for (ax, an) in [(Axis::X, "x"), (Axis::Y, "y"), (Axis::Z, "z")] {
                let mut h = Hasher::new();
                for x in VALS {
                    for y in VALS {
                        for z in VALS {
                            for sz in SIZES {
                                let bx = AABB::new(x, y, z, x + sz, y + sz, z + sz);
                                for d in DELTAS {
                                    h.add_f(s.collide(ax, &bx, d));
                                }
                            }
                        }
                    }
                }
                out.push(format!("grid {name} {an} {}", h.hex()));
            }
            let mut h = Hasher::new();
            let list = [s.clone(), sh["slab"].clone(), sh["fence"].clone()];
            for x in VALS {
                for y in VALS {
                    for d in DELTAS {
                        h.add_f(Shapes::collide(Axis::Y, &AABB::new(x, y, 0.3, x + 0.4, y + 0.4, 0.7), list.iter(), d));
                    }
                }
            }
            out.push(format!("gridmulti {name} {}", h.hex()));
        }
        "faces" => {
            let s = shape!(name);
            let d = s.shape();
            let mut h = Hasher::new();
            let mut n = 0;
            d.for_all_faces(&mut |dir, x, y, z| {
                n += 1;
                h.add_i(dir.ordinal() as i32);
                h.add_i(x);
                h.add_i(y);
                h.add_i(z);
            });
            let mut hi = Hasher::new();
            if let DiscreteVoxelShape::BitSet(bs) = &**d {
                for x in 0..d.get_x_size() {
                    for y in 0..d.get_y_size() {
                        for z in 0..d.get_z_size() {
                            hi.add_i(bs.is_interior(x, y, z) as i32);
                        }
                    }
                }
            }
            out.push(format!("faces {name} {n} {} {}", h.hex(), hi.hex()));
        }
        "bounds" => {
            let s = shape!(name);
            if s.is_empty() {
                out.push(format!("bounds {name} ERR"));
            } else {
                let a = s.bounds();
                out.push(format!(
                    "bounds {name} {}",
                    join_hex([a.min_x, a.min_y, a.min_z, a.max_x, a.max_y, a.max_z].into_iter())
                ));
            }
        }
        "point" => {
            let s = shape!(name);
            let mut h = Hasher::new();
            let coords = [-0.3, 0.0, 1e-7, 0.2, 0.5, 0.8, 0.9999999, 1.0, 1.1];
            for ax in Axis::VALUES {
                for b in coords {
                    for c in coords {
                        h.add_f(s.min_bc(ax, b, c));
                        h.add_f(s.max_bc(ax, b, c));
                    }
                }
                for c in coords {
                    h.add_i(s.find_index(ax, c));
                }
                h.add_f(s.min(ax));
                h.add_f(s.max(ax));
            }
            out.push(format!("point {name} {}", h.hex()));
        }
        "closest" => {
            let s = shape!(name);
            let r = s.closest_point_to(Vec3::new(f(t[2]), f(t[3]), f(t[4])));
            let r = r.map_or("none".to_string(), |v| join_hex([v.x, v.y, v.z].into_iter()));
            out.push(format!("closest {name} {} {} {} {r}", t[2], t[3], t[4]));
        }
        "joinne" => out.push(format!("joinne {} {} {} {}", t[1], t[2], t[3], Shapes::join_is_not_empty(&sh[t[1]], &sh[t[2]], op(t[3])))),
        "equal" => out.push(format!("equal {} {} {}", t[1], t[2], Shapes::equal(&sh[t[1]], &sh[t[2]]))),
        "faceocc" => out.push(format!("faceocc {} {} {}", t[1], t[2], Shapes::face_shape_occludes(&sh[t[1]], &sh[t[2]]))),
        "occl" => {
            let d = dir(t[3]);
            out.push(format!(
                "occl {} {} {} {} {}",
                t[1],
                t[2],
                t[3],
                Shapes::block_occludes(&sh[t[1]], &sh[t[2]], d),
                Shapes::merged_face_occludes(&sh[t[1]], &sh[t[2]], d)
            ));
        }
        "clip" => {
            let s = &sh[t[1]];
            let v: Vec<f64> = (0..9).map(|i| f(t[2 + i])).collect();
            let r = s.clip(Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5]), BlockPos::new(v[6] as i32, v[7] as i32, v[8] as i32));
            let mut b = format!("clip {}", t[1..].join(" "));
            match r {
                None => b += " null",
                Some(r) => {
                    b += &format!(
                        " {} {} {},{},{} {} {}",
                        join_hex([r.location.x, r.location.y, r.location.z].into_iter()),
                        r.direction.get_name(),
                        r.block_pos.x(),
                        r.block_pos.y(),
                        r.block_pos.z(),
                        r.inside,
                        r.miss
                    );
                }
            }
            out.push(b);
        }
        other => panic!("unknown command {other}"),
    }
}

#[test]
fn matches_vanilla_oracle() {
    let (actual, _) = run(SCRIPT);
    let expected: Vec<&str> = EXPECTED.lines().collect();
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(a, e, "first mismatch at output line {}", i + 1);
    }
    assert_eq!(actual.len(), expected.len(), "line count");
}

fn kv<'a>(line: &'a str, key: &str) -> &'a str {
    let p = format!(" {key}=");
    let i = line.find(&p).unwrap() + p.len();
    let rest = &line[i..];
    &rest[..rest.find(' ').unwrap_or(rest.len())]
}

fn ints(s: &str) -> Vec<i32> {
    s.split(',').map(|v| v.parse().unwrap()).collect()
}

fn hexes(s: &str) -> Vec<f64> {
    s.split(',').map(|v| f64::from_bits(u64::from_str_radix(v, 16).unwrap())).collect()
}

/// The public "exact vanilla internals" constructors rebuild every dumped bitset-backed shape bit-for-bit.
#[test]
fn constructors_rebuild_dumped_shapes() {
    let (_, shapes) = run(SCRIPT);
    let mut rebuilt = 0;
    for line in EXPECTED.lines().filter(|l| l.starts_with("dump ") && l.contains("disc=bitset")) {
        let name = line.split(' ').nth(1).unwrap();
        let size = ints(kv(line, "size"));
        let (first, last) = (ints(kv(line, "first")), ints(kv(line, "last")));
        let words_s = kv(line, "words");
        let words: Vec<u64> = if words_s == "-" { vec![] } else { words_s.split(',').map(|w| u64::from_str_radix(w, 16).unwrap()).collect() };
        let discrete = BitSetDiscreteVoxelShape::from_raw(size[0], size[1], size[2], &words, first[0], first[1], first[2], last[0], last[1], last[2]);
        let shape = if kv(line, "kind") == "cube" {
            VoxelShape::cube(DiscreteVoxelShape::BitSet(discrete))
        } else {
            let ckind = kv(line, "ckind").as_bytes();
            let mk = |i: usize, key: &str| {
                let v = hexes(kv(line, key));
                if ckind[i] == b'c' { Coords::cube(v.len() as i32 - 1) } else { Coords::array(v) }
            };
            VoxelShape::array(DiscreteVoxelShape::BitSet(discrete), mk(0, "cx"), mk(1, "cy"), mk(2, "cz"))
        };
        assert_eq!(&shape, &shapes[name], "{name}");
        rebuilt += 1;
    }
    assert!(rebuilt > 50);
}

#[test]
fn partial_eq_is_structural_and_ptr_eq_is_identity() {
    let a = Shapes::box_(0.0, 0.0, 0.0, 1.0, 0.5, 1.0);
    let b = Shapes::box_(0.0, 0.0, 0.0, 1.0, 0.5, 1.0);
    assert_eq!(a, b);
    assert!(!a.ptr_eq(&b));
    assert!(a.ptr_eq(&a.clone()));
    assert_ne!(a, Shapes::box_(0.0, 0.0, 0.0, 1.0, 0.25, 1.0));
    assert_ne!(a.move_(0.0, 0.0, 0.0), a, "moved shapes use OffsetDoubleList-style array coords");
}

#[test]
fn shapes_are_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<VoxelShape>();
}
