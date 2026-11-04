//! Golden tests for `OctahedralGroup`, `SymmetricGroup3`, `FrontAndTop`, `Rotation/Mirror::rotation` and the
//! `Shapes::rotate*` family against vanilla.
//!
//! `fixtures/octahedral/octahedral.script` is interpreted by both `OctahedralOracle.java` (real vanilla classes,
//! output committed as `octahedral.out`) and the interpreter below (the Rust port). The output lines must be
//! identical.

use std::collections::HashMap;

use mc_core::direction::{Axis, Direction, FrontAndTop};
use mc_core::math::octahedral_group::{OctahedralGroup, SymmetricGroup3};
use mc_core::mirror::Mirror;
use mc_core::phys::shapes::{AttachFace, DiscreteVoxelShape, Shapes, VoxelShape};
use mc_core::phys::Vec3;
use mc_core::rotation::Rotation;

const SCRIPT: &str = include_str!("fixtures/octahedral/octahedral.script");
const EXPECTED: &str = include_str!("fixtures/octahedral/octahedral.out");
// The generator is committed next to its output; referencing it here keeps the pair honest.
const _ORACLE: &str = include_str!("fixtures/octahedral/OctahedralOracle.java");

fn hx(d: f64) -> String {
    format!("{:x}", d.to_bits())
}

fn fx(f: f32) -> String {
    format!("{:x}", f.to_bits())
}

fn joined<T: ToString>(it: impl IntoIterator<Item = T>, sep: &str) -> String {
    it.into_iter().map(|v| v.to_string()).collect::<Vec<_>>().join(sep)
}

fn dump(s: &VoxelShape) -> String {
    let d = s.shape();
    let mut b = format!("kind={}", if s.is_cube_voxel_shape() { "cube" } else { "array" });
    let (disc, words) = match &**d {
        DiscreteVoxelShape::BitSet(bs) => {
            let w = bs.bits();
            ("bitset", if w.is_empty() { "-".to_string() } else { joined(w.iter().map(|w| format!("{w:x}")), ",") })
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
        b += &format!(" {n}={}", joined(s.get_coords(a).to_vec().into_iter().map(hx), ","));
    }
    b
}

fn emit_shape(out: &mut Vec<String>, label: &str, r: &VoxelShape, src: &VoxelShape) {
    let rf = if r.ptr_eq(src) { "same" } else { "new" };
    out.push(format!("{label} ref={rf} {}", dump(r)));
}

fn group(constant_name: &str) -> OctahedralGroup {
    OctahedralGroup::by_name(&constant_name.to_lowercase()).unwrap_or_else(|| panic!("bad group {constant_name}"))
}

fn point(t: &[&str], from: usize) -> Option<Vec3> {
    (t.len() >= from + 3).then(|| Vec3::new(t[from].parse().unwrap(), t[from + 1].parse().unwrap(), t[from + 2].parse().unwrap()))
}

fn opt_ord(o: Option<FrontAndTop>) -> String {
    o.map_or("-".to_string(), |f| f.ordinal().to_string())
}

fn tables(out: &mut Vec<String>) {
    let g = OctahedralGroup::ALL;
    for a in g {
        out.push(format!("compose {}: {}", a.ordinal(), joined(g.iter().map(|&c| a.compose(c).ordinal()), " ")));
    }
    out.push(format!("inverse: {}", joined(g.iter().map(|a| a.inverse().ordinal()), " ")));
    for a in g {
        let inv = |ax| a.inverts(ax) as i32;
        out.push(format!(
            "meta {} {a} {} perm={} inv={}{}{} m={}",
            a.ordinal(),
            a.get_serialized_name(),
            a.permutation().ordinal(),
            inv(Axis::X),
            inv(Axis::Y),
            inv(Axis::Z),
            joined(a.transformation().into_iter().map(fx), ",")
        ));
    }
    out.push(format!(
        "alias {}",
        joined(
            [
                OctahedralGroup::BLOCK_ROT_X_270,
                OctahedralGroup::BLOCK_ROT_X_180,
                OctahedralGroup::BLOCK_ROT_X_90,
                OctahedralGroup::BLOCK_ROT_Y_270,
                OctahedralGroup::BLOCK_ROT_Y_180,
                OctahedralGroup::BLOCK_ROT_Y_90,
                OctahedralGroup::BLOCK_ROT_Z_270,
                OctahedralGroup::BLOCK_ROT_Z_180,
                OctahedralGroup::BLOCK_ROT_Z_90,
            ]
            .map(|g| g.ordinal()),
            " "
        )
    ));
    for a in g {
        out.push(format!("rotdir {}: {}", a.ordinal(), joined(Direction::ALL.map(|d| a.rotate(d).ordinal()), " ")));
    }
    let vecs = [[1, 2, 3], [-4, 5, 6], [0, 0, 0], [7, -8, 9], [i32::MIN, 1, -1], [100, -200, 300]];
    for a in g {
        let parts = vecs.map(|v| {
            let r = a.rotate_vec3i(v);
            format!("{},{},{}", r[0], r[1], r[2])
        });
        out.push(format!("rotvec {}: {}", a.ordinal(), parts.join(" ")));
    }
    for a in g {
        out.push(format!(
            "rotfat {}: {}",
            a.ordinal(),
            joined(FrontAndTop::ALL.map(|f| opt_ord(a.rotate_front_and_top(f))), " ")
        ));
    }
    for f in FrontAndTop::ALL {
        out.push(format!("fat {} {} {} {}", f.ordinal(), f.get_serialized_name(), f.front().ordinal(), f.top().ordinal()));
    }
    for f in Direction::ALL {
        out.push(format!(
            "fatlookup {}: {}",
            f.ordinal(),
            joined(Direction::ALL.map(|t| opt_ord(FrontAndTop::from_front_and_top(f, t))), " ")
        ));
    }
    for r in Rotation::ALL {
        out.push(format!("rotation {} {}", r.ordinal(), r.rotation().ordinal()));
    }
    for m in Mirror::ALL {
        out.push(format!("mirror {} {}", m as usize, m.rotation().ordinal()));
    }
    for a in SymmetricGroup3::ALL {
        let vi = a.permute_vector([10, 20, 30]);
        let vf = a.permute_vector_f32([1.5, -2.5, 3.5]);
        out.push(format!(
            "sym {} inv={} compose={} permute={}{}{} axis={} vi={},{},{} vf={},{},{} m={}",
            a.ordinal(),
            a.inverse().ordinal(),
            SymmetricGroup3::ALL.iter().map(|&c| format!("{},", a.compose(c).ordinal())).collect::<String>(),
            a.permute(0),
            a.permute(1),
            a.permute(2),
            Axis::VALUES.iter().map(|&x| a.permute_axis(x).ordinal().to_string()).collect::<String>(),
            vi[0],
            vi[1],
            vi[2],
            vf[0].to_bits() as i32,
            vf[1].to_bits() as i32,
            vf[2].to_bits() as i32,
            joined(a.transformation().into_iter().map(fx), ",")
        ));
    }
}

fn emit_dir_map(out: &mut Vec<String>, label: &str, m: &HashMap<Direction, VoxelShape>, src: &VoxelShape) {
    for d in Direction::ALL {
        if let Some(s) = m.get(&d) {
            emit_shape(out, &format!("{label} {}", d.get_name()), s, src);
        }
    }
    out.push(format!("{label} keys={}", m.len()));
}

fn emit_axis_map(out: &mut Vec<String>, label: &str, m: &HashMap<Axis, VoxelShape>, src: &VoxelShape) {
    for a in Axis::VALUES {
        if let Some(s) = m.get(&a) {
            emit_shape(out, &format!("{label} {}", a.get_name()), s, src);
        }
    }
    out.push(format!("{label} keys={}", m.len()));
}

fn run(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    tables(&mut out);
    let mut sh: HashMap<String, VoxelShape> = HashMap::new();
    let num = |s: &str| -> f64 { s.parse().unwrap() };
    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            "empty" => drop(sh.insert(t[1].into(), Shapes::empty())),
            "block" => drop(sh.insert(t[1].into(), Shapes::block())),
            "box" | "create" => {
                let v: Vec<f64> = (0..6).map(|i| num(t[2 + i])).collect();
                let s = if t[0] == "box" {
                    Shapes::box_(v[0], v[1], v[2], v[3], v[4], v[5])
                } else {
                    Shapes::create_bounds(v[0], v[1], v[2], v[3], v[4], v[5])
                };
                sh.insert(t[1].into(), s);
            }
            "or" => {
                let tail: Vec<VoxelShape> = t[3..].iter().map(|n| sh[*n].clone()).collect();
                let s = Shapes::or_many(&sh[t[2]], &tail);
                sh.insert(t[1].into(), s);
            }
            "move" => {
                let s = sh[t[2]].move_(num(t[3]), num(t[4]), num(t[5]));
                sh.insert(t[1].into(), s);
            }
            "slice" => {
                let a = match t[3] {
                    "X" => Axis::X,
                    "Y" => Axis::Y,
                    _ => Axis::Z,
                };
                let s = sh[t[2]].slice(a, t[4].parse().unwrap());
                sh.insert(t[1].into(), s);
            }
            "rot" => {
                let src = &sh[t[1]];
                let g = group(t[2]);
                let r = match point(&t, 3) {
                    None => Shapes::rotate(src, g),
                    Some(p) => Shapes::rotate_with_point(src, g, &p),
                };
                emit_shape(&mut out, &format!("rot {}", &line[4..]), &r, src);
            }
            "rotall" => {
                let src = &sh[t[1]];
                let p = point(&t, 2);
                for g in OctahedralGroup::ALL {
                    let r = match &p {
                        None => Shapes::rotate(src, g),
                        Some(p) => Shapes::rotate_with_point(src, g, p),
                    };
                    let pt = if p.is_some() { " pt" } else { "" };
                    emit_shape(&mut out, &format!("rotall {} {}{pt}", t[1], g.ordinal()), &r, src);
                }
            }
            "disc" => {
                let src = &sh[t[1]];
                let r = src.shape().rotate(group(t[2]));
                // vanilla wraps both in fresh CubeVoxelShapes, so the reference is always "new"
                out.push(format!("disc {} {} ref=new {}", t[1], t[2], dump(&VoxelShape::cube(r))));
            }
            "horizaxis" => {
                let src = &sh[t[1]];
                let m = match point(&t, 2) {
                    None => Shapes::rotate_horizontal_axis(src),
                    Some(p) => Shapes::rotate_horizontal_axis_with_center(src, &p),
                };
                emit_axis_map(&mut out, line, &m, src);
            }
            "allaxis" => {
                let src = &sh[t[1]];
                let m = match point(&t, 2) {
                    None => Shapes::rotate_all_axis(src),
                    Some(p) => Shapes::rotate_all_axis_with_center(src, &p),
                };
                emit_axis_map(&mut out, line, &m, src);
            }
            "horiz" => {
                let src = &sh[t[1]];
                let g = (t.len() > 2 && t[2] != "-").then(|| group(t[2]));
                let m = match (g, point(&t, 3)) {
                    (None, _) => Shapes::rotate_horizontal(src),
                    (Some(g), None) => Shapes::rotate_horizontal_initial(src, g),
                    (Some(g), Some(p)) => Shapes::rotate_horizontal_with(src, g, &p),
                };
                emit_dir_map(&mut out, line, &m, src);
            }
            "all" => {
                let src = &sh[t[1]];
                let g = (t.len() > 2 && t[2] != "-").then(|| group(t[2]));
                let m = match (g, point(&t, 3)) {
                    (Some(g), p) => Shapes::rotate_all_with(src, g, &p.unwrap_or(Vec3::new(0.5, 0.5, 0.5))),
                    (None, None) => Shapes::rotate_all(src),
                    (None, Some(p)) => Shapes::rotate_all_with_center(src, &p),
                };
                emit_dir_map(&mut out, line, &m, src);
            }
            "attach" => {
                let src = &sh[t[1]];
                let m = if t.len() > 2 {
                    Shapes::rotate_attach_face_initial(src, group(t[2]))
                } else {
                    Shapes::rotate_attach_face(src)
                };
                for f in AttachFace::ALL {
                    emit_dir_map(&mut out, &format!("{line} {}", f.get_serialized_name()), &m[&f], src);
                }
                out.push(format!("{line} faces={}", m.len()));
            }
            other => panic!("unknown command {other}"),
        }
    }
    out
}

#[test]
fn matches_vanilla_oracle() {
    let actual = run(SCRIPT);
    let expected: Vec<&str> = EXPECTED.lines().collect();
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(a, e, "first mismatch at output line {}", i + 1);
    }
    assert_eq!(actual.len(), expected.len(), "line count");
}

#[test]
fn group_laws() {
    use OctahedralGroup as G;
    for a in G::ALL {
        assert_eq!(a.compose(a.inverse()), G::Identity);
        assert_eq!(a.compose(G::Identity), a);
        assert_eq!(G::by_name(a.get_serialized_name()), Some(a));
        for b in G::ALL {
            for c in G::ALL {
                assert_eq!(a.compose(b).compose(c), a.compose(b.compose(c)));
            }
        }
    }
}
