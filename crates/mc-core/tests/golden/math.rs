//! Golden tests for `mc_core::math` against vanilla `Mth` (fixtures/math/MthGolden.java -> mth.txt).
use mc_core::math::java;
use mc_core::math::mth::*;
use mc_core::phys::{AABB, Vec3};

const FIXTURE: &str = include_str!("../fixtures/math/mth.txt");

fn d(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
fn f(s: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(s, 16).unwrap())
}
fn i(s: &str) -> i32 {
    s.parse().unwrap()
}
fn l(s: &str) -> i64 {
    s.parse().unwrap()
}
fn b(s: &str) -> bool {
    s.parse().unwrap()
}

/// Result compared by representation; NaNs compare equal.
enum R {
    D(f64),
    F(f32),
    I(i64),
    B(bool),
    L(Vec<i32>),
    S(String),
}

impl R {
    fn expect(&self, tok: &str) -> Result<(), String> {
        let ok = match self {
            R::D(x) => {
                let e = d(tok);
                (x.is_nan() && e.is_nan()) || x.to_bits() == e.to_bits()
            }
            R::F(x) => {
                let e = f(tok);
                (x.is_nan() && e.is_nan()) || x.to_bits() == e.to_bits()
            }
            R::I(x) => *x == l(tok),
            R::B(x) => *x == b(tok),
            R::L(x) => {
                let e: Vec<i32> = tok
                    .trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(i)
                    .collect();
                *x == e
            }
            R::S(x) => x == tok,
        };
        if ok {
            Ok(())
        } else {
            Err(match self {
                R::D(x) => format!("got {:016x} ({x})", x.to_bits()),
                R::F(x) => format!("got {:08x} ({x})", x.to_bits()),
                R::I(x) => format!("got {x}"),
                R::B(x) => format!("got {x}"),
                R::L(x) => format!("got {x:?}"),
                R::S(x) => format!("got {x}"),
            })
        }
    }
}

/// Runs `func` over every fixture line named `name`; returns the number of lines checked.
fn run(name: &str, func: impl Fn(&[&str]) -> R) {
    let mut n = 0;
    for line in FIXTURE.lines() {
        let Some((lhs, rhs)) = line.split_once(" = ") else {
            continue;
        };
        let mut toks = lhs.split(' ');
        if toks.next() != Some(name) {
            continue;
        }
        let args: Vec<&str> = toks.collect();
        if let Err(e) = func(&args).expect(rhs) {
            panic!("{name}({}) expected {rhs}: {e}", args.join(" "));
        }
        n += 1;
    }
    assert!(n > 0, "no fixture lines for {name}");
}

fn hash_line(name: &str) -> Vec<i64> {
    let line = FIXTURE.lines().find(|l| l.starts_with(name)).unwrap();
    line.split(' ').skip(1).map(l).collect()
}

#[test]
fn constants() {
    let line = FIXTURE.lines().find(|l| l.starts_with("consts")).unwrap();
    let t: Vec<f32> = line
        .split(" = ")
        .next()
        .unwrap()
        .split(' ')
        .skip(1)
        .map(f)
        .collect();
    let ours = [
        PI,
        HALF_PI,
        TWO_PI,
        DEG_TO_RAD,
        RAD_TO_DEG,
        EPSILON,
        SQRT_OF_TWO,
    ];
    for (a, e) in ours.iter().zip(&t) {
        assert_eq!(a.to_bits(), e.to_bits());
    }
}

#[test]
fn sin_cos_table_and_lookups() {
    // Whole 65536-entry table via the public lookups: x = (n + 0.25) / scale lands on index n (sin) / n + 16384 (cos).
    let (mut hs, mut hc) = (0i64, 0i64);
    for n in 0..65536 {
        let x = (f64::from(n) + 0.25) / 10430.378350470453;
        hs = hs
            .wrapping_mul(1000003)
            .wrapping_add(i64::from(sin(x).to_bits() as i32));
        hc = hc
            .wrapping_mul(1000003)
            .wrapping_add(i64::from(cos(x).to_bits() as i32));
    }
    assert_eq!([hs, hc], hash_line("sintable_hash")[..]);
    run("sin", |a| R::F(sin(d(a[0]))));
    run("cos", |a| R::F(cos(d(a[0]))));
}

#[test]
fn atan2_and_inv_sqrt() {
    run("atan2", |a| R::D(atan2(d(a[0]), d(a[1]))));
    run("fastInvSqrt", |a| R::D(fast_inv_sqrt(d(a[0]))));
    run("fastInvCubeRoot", |a| R::F(fast_inv_cube_root(f(a[0]))));
    run("invSqrt_d", |a| R::D(inv_sqrt_f64(d(a[0]))));
    run("invSqrt_f", |a| R::F(inv_sqrt_f32(f(a[0]))));
    run("sqrt", |a| R::F(sqrt(f(a[0]))));
}

#[test]
fn floor_ceil_frac_sign() {
    run("lfloor", |a| R::I(lfloor(d(a[0]))));
    run("floor_d", |a| R::I(floor_f64(d(a[0])).into()));
    run("floor_f", |a| R::I(floor_f32(f(a[0])).into()));
    run("ceil_d", |a| R::I(ceil_f64(d(a[0])).into()));
    run("ceil_f", |a| R::I(ceil_f32(f(a[0])).into()));
    run("ceilLong", |a| R::I(ceil_long(d(a[0]))));
    run("frac_d", |a| R::D(frac_f64(d(a[0]))));
    run("frac_f", |a| R::F(frac_f32(f(a[0]))));
    run("sign", |a| R::I(sign(d(a[0])).into()));
    run("quantize", |a| R::I(quantize(d(a[0]), i(a[1])).into()));
}

#[test]
fn degrees() {
    run("wrapDegrees_d", |a| R::D(wrap_degrees_f64(d(a[0]))));
    run("wrapDegrees_f", |a| R::F(wrap_degrees_f32(f(a[0]))));
    run("wrapDegrees_i", |a| R::I(wrap_degrees_i32(i(a[0])).into()));
    run("wrapDegrees_l", |a| R::F(wrap_degrees_i64(l(a[0]))));
    run("wrapDegrees90", |a| R::F(wrap_degrees90(f(a[0]))));
    run("packDegrees", |a| R::I(pack_degrees(f(a[0])).into()));
    run("unpackDegrees", |a| R::F(unpack_degrees(i(a[0]) as i8)));
    run("rotLerp_d", |a| {
        R::D(rot_lerp_f64(d(a[0]), d(a[1]), d(a[2])))
    });
    run("rotLerp_f", |a| {
        R::F(rot_lerp_f32(f(a[0]), f(a[1]), f(a[2])))
    });
    run("rotLerpRad", |a| {
        R::F(rot_lerp_rad(f(a[0]), f(a[1]), f(a[2])))
    });
    run("approach", |a| R::F(approach(f(a[0]), f(a[1]), f(a[2]))));
    run("approachDegrees", |a| {
        R::F(approach_degrees(f(a[0]), f(a[1]), f(a[2])))
    });
    run("degreesDifference", |a| {
        R::F(degrees_difference(f(a[0]), f(a[1])))
    });
    run("degreesDifferenceAbs", |a| {
        R::F(degrees_difference_abs(f(a[0]), f(a[1])))
    });
    run("rotateIfNecessary", |a| {
        R::F(rotate_if_necessary(f(a[0]), f(a[1]), f(a[2])))
    });
    run("triangleWave", |a| R::F(triangle_wave(f(a[0]), f(a[1]))));
}

#[test]
fn integer_helpers() {
    run("murmur", |a| R::I(murmur_hash3_mixer(i(a[0])).into()));
    run("ceillog2", |a| R::I(ceillog2(i(a[0])).into()));
    run("log2", |a| R::I(log2(i(a[0])).into()));
    run("smallestEncompassingPowerOfTwo", |a| {
        R::I(smallest_encompassing_power_of_two(i(a[0])).into())
    });
    run("smallestSquareSide", |a| {
        R::I(smallest_square_side(i(a[0])).into())
    });
    run("isPowerOfTwo_i", |a| R::B(is_power_of_two_i32(i(a[0]))));
    run("isPowerOfTwo_l", |a| R::B(is_power_of_two_i64(l(a[0]))));
    run("abs_i", |a| R::I(abs_i32(i(a[0])).into()));
    run("square_i", |a| R::I(square_i32(i(a[0])).into()));
    run("square_l", |a| R::I(square_i64(l(a[0]))));
    run("positiveModulo_i", |a| {
        R::I(positive_modulo_i32(i(a[0]), i(a[1])).into())
    });
    run("floorDiv", |a| R::I(floor_div(i(a[0]), i(a[1])).into()));
    run("isMultipleOf", |a| R::B(is_multiple_of(i(a[0]), i(a[1]))));
    run("positiveCeilDiv_i", |a| {
        R::I(positive_ceil_div_i32(i(a[0]), i(a[1])).into())
    });
    run("roundToward_i", |a| {
        R::I(round_toward_i32(i(a[0]), i(a[1])).into())
    });
    run("positiveCeilDiv_l", |a| {
        R::I(positive_ceil_div_i64(l(a[0]), l(a[1])))
    });
    run("roundToward_l", |a| {
        R::I(round_toward_i64(l(a[0]), l(a[1])))
    });
    run("absMax_i", |a| R::I(abs_max_i32(i(a[0]), i(a[1])).into()));
    run("clamp_i", |a| {
        R::I(clamp_i32(i(a[0]), i(a[1]), i(a[2])).into())
    });
    run("clamp_l", |a| R::I(clamp_i64(l(a[0]), l(a[1]), l(a[2]))));
    run("chessboardDistance", |a| {
        R::I(chessboard_distance(i(a[0]), i(a[1]), i(a[2]), i(a[3])).into())
    });
    run("getSeed", |a| R::I(get_seed_xyz(i(a[0]), i(a[1]), i(a[2]))));
}

#[test]
fn float_helpers() {
    run("clamp_d", |a| R::D(clamp_f64(d(a[0]), d(a[1]), d(a[2]))));
    run("clamp_f", |a| R::F(clamp_f32(f(a[0]), f(a[1]), f(a[2]))));
    run("clampedLerp_d", |a| {
        R::D(clamped_lerp_f64(d(a[0]), d(a[1]), d(a[2])))
    });
    run("clampedLerp_f", |a| {
        R::F(clamped_lerp_f32(f(a[0]), f(a[1]), f(a[2])))
    });
    run("lerp_d", |a| R::D(lerp_f64(d(a[0]), d(a[1]), d(a[2]))));
    run("lerp_f", |a| R::F(lerp_f32(f(a[0]), f(a[1]), f(a[2]))));
    run("inverseLerp_d", |a| {
        R::D(inverse_lerp_f64(d(a[0]), d(a[1]), d(a[2])))
    });
    run("inverseLerp_f", |a| {
        R::F(inverse_lerp_f32(f(a[0]), f(a[1]), f(a[2])))
    });
    run("absMax_d", |a| R::D(abs_max_f64(d(a[0]), d(a[1]))));
    run("absMax_f", |a| R::F(abs_max_f32(f(a[0]), f(a[1]))));
    run("abs_f", |a| R::F(abs_f32(f(a[0]))));
    run("equal_d", |a| R::B(equal_f64(d(a[0]), d(a[1]))));
    run("equal_f", |a| R::B(equal_f32(f(a[0]), f(a[1]))));
    run("positiveModulo_d", |a| {
        R::D(positive_modulo_f64(d(a[0]), d(a[1])))
    });
    run("positiveModulo_f", |a| {
        R::F(positive_modulo_f32(f(a[0]), f(a[1])))
    });
    run("map_d", |a| {
        R::D(map_f64(d(a[0]), d(a[1]), d(a[2]), d(a[3]), d(a[4])))
    });
    run("map_f", |a| {
        R::F(map_f32(f(a[0]), f(a[1]), f(a[2]), f(a[3]), f(a[4])))
    });
    run("clampedMap_d", |a| {
        R::D(clamped_map_f64(d(a[0]), d(a[1]), d(a[2]), d(a[3]), d(a[4])))
    });
    run("clampedMap_f", |a| {
        R::F(clamped_map_f32(f(a[0]), f(a[1]), f(a[2]), f(a[3]), f(a[4])))
    });
    run("catmullrom", |a| {
        R::F(catmullrom(f(a[0]), f(a[1]), f(a[2]), f(a[3]), f(a[4])))
    });
    run("smoothstep", |a| R::F(smoothstep(f(a[0]))));
    run("smoothstepDerivative", |a| {
        R::F(smoothstep_derivative(f(a[0])))
    });
    run("square_f", |a| R::F(square_f32(f(a[0]))));
    run("cube", |a| R::F(cube(f(a[0]))));
    run("lerp2_d", |a| {
        R::D(lerp2_f64(
            d(a[0]),
            d(a[1]),
            d(a[2]),
            d(a[3]),
            d(a[4]),
            d(a[5]),
        ))
    });
    run("lerp2_f", |a| {
        R::F(lerp2_f32(
            f(a[0]),
            f(a[1]),
            f(a[2]),
            f(a[3]),
            f(a[4]),
            f(a[5]),
        ))
    });
    run("lerp3_d", |a| {
        let v: Vec<f64> = a.iter().map(|s| d(s)).collect();
        R::D(lerp3_f64(
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10],
        ))
    });
    run("lerp3_f", |a| {
        let v: Vec<f32> = a.iter().map(|s| f(s)).collect();
        R::F(lerp3_f32(
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10],
        ))
    });
    run("lengthSquared2_d", |a| {
        R::D(length_squared_f64(d(a[0]), d(a[1])))
    });
    run("lengthSquared2_f", |a| {
        R::F(length_squared_f32(f(a[0]), f(a[1])))
    });
    run("length2_d", |a| R::D(length_f64(d(a[0]), d(a[1]))));
    run("length2_f", |a| R::F(length_f32(f(a[0]), f(a[1]))));
    run("lengthSquared3_d", |a| {
        R::D(length_squared_xyz_f64(d(a[0]), d(a[1]), d(a[2])))
    });
    run("lengthSquared3_f", |a| {
        R::F(length_squared_xyz_f32(f(a[0]), f(a[1]), f(a[2])))
    });
    run("length3_d", |a| {
        R::D(length_xyz_f64(d(a[0]), d(a[1]), d(a[2])))
    });
    run("length3_f", |a| {
        R::F(length_xyz_f32(f(a[0]), f(a[1]), f(a[2])))
    });
}

#[test]
fn random_helpers() {
    run("rnd", |a| {
        let mut rs = mc_core::random::create(l(a[0]));
        let (lo, hi, flo, fhi, dlo, dhi) = (i(a[1]), i(a[2]), f(a[3]), f(a[4]), d(a[5]), d(a[6]));
        let h = |x: f32| format!("{:08x}", x.to_bits());
        let n = next_int(&mut rs, lo, hi);
        let nf = next_float(&mut rs, flo, fhi);
        let nd = next_double(&mut rs, dlo, dhi);
        let rb = random_between_inclusive(&mut rs, lo, hi + 1);
        let rf = random_between(&mut rs, flo, fhi + 1.0);
        let no = normal(&mut rs, flo, fhi);
        let uuid = create_insecure_uuid(&mut rs);
        R::S(format!(
            "{n} {} {:016x} {rb} {} {} {uuid}",
            h(nf),
            nd.to_bits(),
            h(rf),
            h(no)
        ))
    });
    run("wobble", |a| R::D(wobble(d(a[0]))));
}

#[test]
fn colors_and_discrete_lerp() {
    run("hsvToRgb", |a| {
        R::I(hsv_to_rgb(f(a[0]), f(a[1]), f(a[2])).into())
    });
    run("hsvToArgb", |a| {
        R::I(hsv_to_argb(f(a[0]), f(a[1]), f(a[2]), i(a[3])).into())
    });
    run("lerpInt", |a| {
        R::I(lerp_int(f(a[0]), i(a[1]), i(a[2])).into())
    });
    run("lerpDiscrete", |a| {
        R::I(lerp_discrete(f(a[0]), i(a[1]), i(a[2])).into())
    });
    let msg = FIXTURE
        .lines()
        .find(|l| l.starts_with("hsv_err"))
        .unwrap()
        .split_once(" = ")
        .unwrap()
        .1;
    let err = std::panic::catch_unwind(|| hsv_to_rgb(-0.5, 0.5, 0.5)).unwrap_err();
    assert_eq!(err.downcast_ref::<String>().unwrap(), msg);
}

#[test]
fn searches_and_streams() {
    run("binarySearch", |a| {
        let target = i64::from(i(a[2]));
        R::I(binary_search(i(a[0]), i(a[1]), |n| i64::from(n) * i64::from(n) >= target).into())
    });
    run("outFromOrigin", |a| {
        R::L(out_from_origin(i(a[0]), i(a[1]), i(a[2])))
    });
    run("outFromOrigin_step", |a| {
        R::L(out_from_origin_step(i(a[0]), i(a[1]), i(a[2]), i(a[3])))
    });
}

#[test]
fn geometry() {
    run("ray", |a| {
        let v: Vec<f64> = a.iter().map(|s| d(s)).collect();
        R::B(ray_intersects_aabb(
            &Vec3::new(v[0], v[1], v[2]),
            &Vec3::new(v[3], v[4], v[5]),
            &AABB::new(v[6], v[7], v[8], v[9], v[10], v[11]),
        ))
    });
    let mut n = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("lerp_vec3 ")) {
        let (lhs, rhs) = line.split_once(" = ").unwrap();
        let a: Vec<f64> = lhs.split(' ').skip(1).map(d).collect();
        let e: Vec<f64> = rhs.split(' ').map(d).collect();
        let r = lerp_vec3(
            a[0],
            &Vec3::new(a[1], a[2], a[3]),
            &Vec3::new(a[4], a[5], a[6]),
        );
        assert_eq!(
            [r.x.to_bits(), r.y.to_bits(), r.z.to_bits()],
            [e[0].to_bits(), e[1].to_bits(), e[2].to_bits()]
        );
        n += 1;
    }
    assert!(n > 0);
}

#[test]
fn java_helpers() {
    // Values asserted from Java semantics: Math.round, floorMod, String.hashCode of a non-BMP string.
    assert_eq!(java::round_f64(-0.5), 0);
    assert_eq!(java::round_f64(0.49999999999999994), 0);
    assert_eq!(java::round_f64(f64::NAN), 0);
    assert_eq!(java::round_f64(1e300), i64::MAX);
    assert_eq!(java::floor_mod_i32(i32::MIN, -1), 0);
    assert_eq!(java::floor_mod_i64(-5, 3), 1);
    // "😀" is the surrogate pair D83D DE00: 0xD83D * 31 + 0xDE00
    assert_eq!(java::java_string_hash("😀"), 0xD83D * 31 + 0xDE00);
    assert!(java::min_f64(0.0, -0.0).is_sign_negative());
    assert!(java::max_f32(f32::NAN, 1.0).is_nan());
}
