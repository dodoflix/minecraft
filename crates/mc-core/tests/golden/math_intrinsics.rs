//! The `java.lang.Math` functions the server sources call, against a Java-generated golden of 2^21 inputs per function
//! (fixtures/math/MathGolden.java): raw bit patterns, ranges, multiples of pi/2, integers and the edge values, hashed in
//! blocks of 1024 outputs. A function is exact when every block digest equals the recorded one.
//!
//! `KNOWN_DIFFERENT` lists the spellings that still differ; each must keep differing (so the list only shrinks), every
//! other spelling must be bit-exact. `scripts/golden-math.sh --rate` prints the exact mismatch count per spelling by running
//! the JDK live over all inputs (`live_mismatch_rates`).
use mc_core::math::{java, mth};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

static FIXTURE: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/math/math_intrinsics.txt.gz"));

const COUNT: usize = 1 << 21;
const BLOCK: usize = 1024;

const EDGES: [f64; 36] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    1.5,
    -1.5,
    2.5,
    -2.5,
    0.499_999_999_999_999_94,
    -0.499_999_999_999_999_94,
    4_503_599_627_370_496.0,
    9_007_199_254_740_992.0,
    9.223_372_036_854_776E18,
    -9.223_372_036_854_776E18,
    2_147_483_648.0,
    -2_147_483_649.0,
    1.0E300,
    -1.0E300,
    f64::from_bits(1),
    f64::MAX,
    f64::MIN_POSITIVE,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    std::f64::consts::PI,
    std::f64::consts::FRAC_PI_2,
    std::f64::consts::TAU,
    10_430.378_350_470_453,
    1.0E-9,
    100.0,
    255.0,
    0.1,
    0.999_999_999_999_999_9,
    1.000_000_000_000_000_2,
];

const INT_EDGES: [i32; 16] = [
    0,
    1,
    -1,
    2,
    -2,
    7,
    -7,
    16,
    255,
    256,
    -256,
    65535,
    i32::MAX,
    i32::MIN,
    i32::MIN + 1,
    i32::MAX - 1,
];

/// SplitMix64, as in MathGolden.java.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * f64::from_bits(0x3ca0_0000_0000_0000)
    }

    fn mix(&mut self) -> f64 {
        let mode = self.next() & 7;
        let v = self.next();
        match mode {
            0 => f64::from_bits(v),
            1 => self.unit() * 2.0 - 1.0,
            2 => self.unit() * 200.0 - 100.0,
            3 => {
                let e = (v >> 52 & 0x7ff) % 121 + 1023 - 60;
                f64::from_bits((v & 0x8000_0000_0000_0000) | e << 52 | (v & 0xf_ffff_ffff_ffff))
            }
            4 => {
                let k = ((v >> 40) & 0xf_ffff) as f64;
                k * std::f64::consts::FRAC_PI_2 + (self.unit() - 0.5) * 2.0E-6
            }
            5 => self.unit() * 2.0E6 - 1.0E6,
            6 => f64::from(((v >> 32) % 129) as i32 - 64),
            _ => f64::from(((v >> 32) % 257) as i32 - 128) * 0.5,
        }
    }
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

fn dbl(v: f64) -> u64 {
    if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits()
    }
}

fn flt(v: f32) -> u64 {
    (if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() }) as i32 as i64 as u64
}

fn d(bits: u64) -> f64 {
    f64::from_bits(bits)
}

fn f(bits: u64) -> f32 {
    f32::from_bits(bits as u32)
}

fn fl(v: f64) -> u64 {
    let v = v as f32;
    u64::from(if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() })
}

/// Inputs `(a, b)` of index `i` for a generator kind (see MathGolden.java).
fn input(gen_kind: char, rng: &mut Rng, i: usize) -> (u64, u64) {
    let n = EDGES.len();
    let m = INT_EDGES.len();
    let edge = |k: usize| EDGES[k];
    match gen_kind {
        'd' => (dbl(if i < n { edge(i) } else { rng.mix() }), 0),
        'D' => {
            if i < n * n {
                return (dbl(edge(i / n)), dbl(edge(i % n)));
            }
            let x = rng.mix();
            let y = rng.mix();
            (dbl(x), dbl(y))
        }
        'p' => {
            if i < n * n {
                return (dbl(edge(i / n)), dbl(edge(i % n)));
            }
            let x = rng.mix();
            let k = ((rng.next() >> 32) % 81) as i64 - 40;
            let half = rng.next() % 3;
            let frac = rng.next() % 4;
            let y = f64::from(k as i32) * if half == 0 { 0.5 } else { 1.0 }
                + if frac == 0 { rng.unit() } else { 0.0 };
            (dbl(x), dbl(y))
        }
        'i' => {
            let x = if i < n { edge(i) } else { rng.mix() };
            let k = ((rng.next() >> 32) % 81) as i64 - 40;
            (dbl(x), k as u64)
        }
        'f' => (fl(if i < n { edge(i) } else { rng.mix() }), 0),
        'F' => {
            if i < n * n {
                return (fl(edge(i / n)), fl(edge(i % n)));
            }
            let x = rng.mix();
            let y = rng.mix();
            (fl(x), fl(y))
        }
        'I' => {
            if i < m * m {
                return (
                    i64::from(INT_EDGES[i / m]) as u64,
                    i64::from(INT_EDGES[i % m]) as u64,
                );
            }
            let a = rng.next();
            let b = rng.next();
            let shift = rng.next() & 31;
            (
                i64::from(a as i32) as u64,
                i64::from((b >> shift) as i32) as u64,
            )
        }
        _ => {
            if i < m * m {
                return (
                    i64::from(INT_EDGES[i / m]) as u64,
                    i64::from(INT_EDGES[i % m]) as u64,
                );
            }
            let a = rng.next();
            let b = rng.next();
            let shift = rng.next() & 63;
            (a, ((b as i64) >> shift) as u64)
        }
    }
}

type Fn2 = fn(u64, u64) -> u64;

/// Every Rust spelling the ports use, as (Java case, spelling, function).
#[expect(
    clippy::disallowed_methods,
    reason = "the std spellings are what the golden measures against the JDK"
)]
fn spellings() -> Vec<(&'static str, &'static str, Fn2)> {
    fn s(
        case: &'static str,
        spelling: &'static str,
        func: Fn2,
    ) -> (&'static str, &'static str, Fn2) {
        (case, spelling, func)
    }
    vec![
        s("sin", "java::sin", |a, _| dbl(java::sin(d(a)))),
        s("cos", "java::cos", |a, _| dbl(java::cos(d(a)))),
        s("asin", "mth::java_asin", |a, _| dbl(mth::java_asin(d(a)))),
        s("acos", "mth::java_acos", |a, _| dbl(mth::java_acos(d(a)))),
        s("log", "java::log", |a, _| dbl(java::log(d(a)))),
        s("log1p", "java::log1p", |a, _| dbl(java::log1p(d(a)))),
        s("sqrt", "f64::sqrt", |a, _| dbl(d(a).sqrt())),
        s("floor", "f64::floor", |a, _| dbl(d(a).floor())),
        s("ceil", "f64::ceil", |a, _| dbl(d(a).ceil())),
        s("rint", "java::rint", |a, _| dbl(java::rint(d(a)))),
        s("signum", "java::signum_f64", |a, _| {
            dbl(java::signum_f64(d(a)))
        }),
        s("nextUp", "f64::next_up", |a, _| dbl(d(a).next_up())),
        s("nextDown", "f64::next_down", |a, _| dbl(d(a).next_down())),
        (
            "toRadians",
            "f64::to_radians",
            |a, _| dbl(d(a).to_radians()),
        ),
        s("abs", "f64::abs", |a, _| dbl(d(a).abs())),
        s("round", "java::round_f64", |a, _| {
            java::round_f64(d(a)) as u64
        }),
        s("atan2", "mth::java_atan2", |a, b| {
            dbl(mth::java_atan2(d(a), d(b)))
        }),
        s("pow", "java::pow", |a, b| dbl(java::pow(d(a), d(b)))),
        s("pow_int", "java::pow", |a, b| {
            dbl(java::pow(d(a), f64::from(b as i32)))
        }),
        s("min", "java::min_f64", |a, b| {
            dbl(java::min_f64(d(a), d(b)))
        }),
        s("min", "f64::min", |a, b| dbl(d(a).min(d(b)))),
        s("max", "java::max_f64", |a, b| {
            dbl(java::max_f64(d(a), d(b)))
        }),
        s("max", "f64::max", |a, b| dbl(d(a).max(d(b)))),
        s("copySign", "f64::copysign", |a, b| dbl(d(a).copysign(d(b)))),
        s("round_f", "java::round_f32", |a, _| {
            i64::from(java::round_f32(f(a))) as u64
        }),
        s("signum_f", "java::signum_f32", |a, _| {
            flt(java::signum_f32(f(a)))
        }),
        s("nextUp_f", "f32::next_up", |a, _| flt(f(a).next_up())),
        s("nextDown_f", "f32::next_down", |a, _| flt(f(a).next_down())),
        s("min_f", "java::min_f32", |a, b| {
            flt(java::min_f32(f(a), f(b)))
        }),
        s("min_f", "f32::min", |a, b| flt(f(a).min(f(b)))),
        s("max_f", "java::max_f32", |a, b| {
            flt(java::max_f32(f(a), f(b)))
        }),
        s("max_f", "f32::max", |a, b| flt(f(a).max(f(b)))),
        s("floorDiv_i", "java::floor_div_i32", |a, b| {
            i64::from(java::floor_div_i32(
                a as i32,
                if b == 0 { 1 } else { b as i32 },
            )) as u64
        }),
        s("floorMod_i", "java::floor_mod_i32", |a, b| {
            i64::from(java::floor_mod_i32(
                a as i32,
                if b == 0 { 1 } else { b as i32 },
            )) as u64
        }),
        s("floorDiv_l", "java::floor_div_i64", |a, b| {
            java::floor_div_i64(a as i64, if b == 0 { 1 } else { b as i64 }) as u64
        }),
        s("floorMod_l", "java::floor_mod_i64", |a, b| {
            java::floor_mod_i64(a as i64, if b == 0 { 1 } else { b as i64 }) as u64
        }),
    ]
}

/// Spellings that still differ from the JDK, with what they affect. Each must keep differing: when one becomes exact,
/// remove its entry.
const KNOWN_DIFFERENT: &[(&str, &str, &str)] = &[
    (
        "cos",
        "java::cos",
        "correctly rounded; the JVM's own routine differs in 0.064% of the inputs (the last bit)",
    ),
    (
        "log",
        "java::log",
        "correctly rounded; the JVM's own routine differs in 3 of 2^21 inputs",
    ),
    (
        "max",
        "f64::max",
        "differs from Math.max only for NaN and the zeros (0.0 against -0.0)",
    ),
    (
        "max_f",
        "f32::max",
        "differs from Math.max only for NaN and the zeros (0.0 against -0.0)",
    ),
    (
        "min",
        "f64::min",
        "differs from Math.min only for NaN and the zeros (0.0 against -0.0)",
    ),
    (
        "min_f",
        "f32::min",
        "differs from Math.min only for NaN and the zeros (0.0 against -0.0)",
    ),
    (
        "pow",
        "java::pow",
        "correctly rounded; the JVM's own routine differs in 0.063% of the inputs",
    ),
    (
        "pow_int",
        "java::pow",
        "correctly rounded; the JVM's own routine differs in 0.079% of the inputs",
    ),
    (
        "sin",
        "java::sin",
        "correctly rounded; the JVM's own routine differs in 0.160% of the inputs (the last bit)",
    ),
];

fn gen_kind(case: &str) -> char {
    match case {
        "atan2" | "min" | "max" | "copySign" => 'D',
        "pow" => 'p',
        "pow_int" => 'i',
        "round_f" | "signum_f" | "nextUp_f" | "nextDown_f" => 'f',
        "min_f" | "max_f" => 'F',
        "floorDiv_i" | "floorMod_i" => 'I',
        "floorDiv_l" | "floorMod_l" => 'L',
        _ => 'd',
    }
}

fn digests(case: &str) -> Vec<&'static str> {
    let mut lines = FIXTURE.lines();
    for line in lines.by_ref() {
        if line.starts_with("case ") && line.split(' ').nth(1) == Some(case) {
            let blocks = COUNT / BLOCK;
            return lines.take(blocks).collect();
        }
    }
    panic!("case {case} not in the golden");
}

fn block_mismatches(case: &str, func: Fn2) -> usize {
    let expected = digests(case);
    let kind = gen_kind(case);
    let mut rng = Rng(fnv(case));
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut bad = 0;
    for i in 0..COUNT {
        let (a, b) = input(kind, &mut rng, i);
        h = (h ^ func(a, b)).wrapping_mul(0x100_0000_01b3);
        if (i + 1) % BLOCK == 0 {
            if format!("{h:016x}") != expected[i / BLOCK] {
                bad += 1;
            }
            h = 0xcbf2_9ce4_8422_2325;
        }
    }
    bad
}

#[test]
fn sin_table_equals_the_jdk() {
    let expected = digests("sin_table");
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for i in 0..65536usize {
        // the quarter step keeps `index = (x * 10430.378...) as i64` at i despite rounding
        let v = mth::sin((i as f64 + 0.25) / 10_430.378_350_470_453);
        h = (h ^ flt(v)).wrapping_mul(0x100_0000_01b3);
        if (i + 1) % 1024 == 0 {
            assert_eq!(
                format!("{h:016x}"),
                expected[i / 1024],
                "SIN block {}",
                i / 1024
            );
            h = 0xcbf2_9ce4_8422_2325;
        }
    }
}

#[test]
fn spellings_match_the_jdk_except_the_known_ones() {
    let mut unexpected = Vec::new();
    let mut fixed = Vec::new();
    let mut report = BTreeMap::new();
    for (case, spelling, func) in spellings() {
        let bad = block_mismatches(case, func);
        report.insert(format!("{case} {spelling}"), bad);
        let known = KNOWN_DIFFERENT
            .iter()
            .any(|(c, s, _)| *c == case && *s == spelling);
        match (bad == 0, known) {
            (false, false) => unexpected.push(format!(
                "{case} {spelling}: {bad} of {} blocks differ",
                COUNT / BLOCK
            )),
            (true, true) => fixed.push(format!("{case} {spelling}")),
            _ => {}
        }
    }
    println!("{report:#?}");
    assert!(
        unexpected.is_empty(),
        "spellings that differ from the JDK:\n{}",
        unexpected.join("\n")
    );
    assert!(
        fixed.is_empty(),
        "now exact, remove from KNOWN_DIFFERENT: {fixed:?}"
    );
}

/// Runs the JDK over every input of one case and counts the inputs where a spelling differs.
#[test]
#[ignore = "permanent: runs the oracle JDK live over 2^21 inputs per case (scripts/golden-math.sh --rate)"]
fn live_mismatch_rates() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let java = root.join("tests/fixtures/math/MathGolden.java");
    for (case, spelling, func) in spellings() {
        let mut child = Command::new("java")
            .arg(&java)
            .args(["full", case])
            .stdout(Stdio::piped())
            .spawn()
            .expect("java");
        let reader = BufReader::new(child.stdout.take().unwrap());
        let edge_count = match gen_kind(case) {
            'D' | 'p' | 'F' => EDGES.len() * EDGES.len(),
            'I' | 'L' => INT_EDGES.len() * INT_EDGES.len(),
            _ => EDGES.len(),
        };
        let (mut bad, mut total, mut edge_bad) = (0usize, 0usize, 0usize);
        let mut first = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap();
            let mut it = line.split(' ').map(|h| u64::from_str_radix(h, 16).unwrap());
            let (a, b, want) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
            total += 1;
            let got = func(a, b);
            if got != want {
                bad += 1;
                // the first inputs are the edge values: a difference there is a special-case bug, not rounding
                edge_bad += usize::from(total <= edge_count);
                if first.len() < 3 {
                    first.push(format!("{a:016x} {b:016x} jdk {want:016x} rust {got:016x}"));
                }
            }
        }
        assert!(child.wait().unwrap().success());
        println!(
            "{case:12} {spelling:22} {bad:8} of {total} differ ({:.4}%), {edge_bad} among the edge inputs",
            100.0 * bad as f64 / total as f64
        );
        for line in first {
            println!("    {line}");
        }
    }
}
