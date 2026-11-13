//! The display transformation math against the real vanilla classes (`oracle/DisplayMathOracle.java`, line format in its
//! header): `Transformation` with JOML's `Matrix4f`/`Quaternionf` float arithmetic and `MatrixUtil.svdDecompose`, the
//! `Transformation`, quaternion, axis-angle and matrix codecs, `Brightness` and `BillboardConstraints`. Every float is
//! compared by its raw bits, and so is every codec error message.

use std::sync::LazyLock;

use mc_core::display::{BillboardConstraints, Brightness};
use mc_core::math::matrix4f::Matrix4f;
use mc_core::math::quaternionf::Quaternionf;
use mc_core::math::transformation::Transformation;
use mc_dfu::extra::{
    BILLBOARD_CONSTRAINTS, BRIGHTNESS, MATRIX4F, QUATERNIONF, TRANSFORMATION,
    TRANSFORMATION_EXTENDED,
};
use mc_dfu::{Codec, DataResult, encode_nbt, parse_nbt};
use mc_nbt::{Tag, snbt};

static GOLDEN: LazyLock<&'static str> =
    LazyLock::new(|| include_str!("../fixtures/display_math.txt"));

fn hex(f: f32) -> String {
    format!("{:08x}", f.to_bits())
}

fn hex_all(v: &[f32]) -> String {
    v.iter().map(|f| hex(*f)).collect::<Vec<_>>().join(" ")
}

fn matrix(m: &Matrix4f) -> String {
    let v: Vec<f32> = (0..16).map(|i| m.get_row_column(i >> 2, i & 3)).collect();
    format!("P={} M={}", m.properties(), hex_all(&v))
}

fn quat(q: &Quaternionf) -> String {
    hex_all(&[q.x, q.y, q.z, q.w])
}

fn floats(tag: &Tag) -> Vec<f32> {
    tag.as_list()
        .expect("list")
        .iter()
        .map(|t| t.as_float().expect("float"))
        .collect()
}

/// The 14 floats of the `Transformation.CODEC` encoding.
fn encoded(t: &Transformation) -> String {
    let Some(Tag::Compound(tag)) = encode_nbt(&TRANSFORMATION_EXTENDED, t).result() else {
        return "ERR".into();
    };
    let all: Vec<f32> = ["translation", "left_rotation", "scale", "right_rotation"]
        .iter()
        .flat_map(|key| floats(tag.get(key).expect("encoded key")))
        .collect();
    hex_all(&all)
}

fn dump(t: &Transformation) -> String {
    format!(
        "OK {} T={} L={} S={} R={} E={}",
        matrix(t.matrix()),
        hex_all(&t.translation()),
        quat(&t.left_rotation()),
        hex_all(&t.scale()),
        quat(&t.right_rotation()),
        encoded(t)
    )
}

fn decode<A: 'static>(codec: &Codec<A>, input: &str) -> DataResult<A> {
    let tag =
        snbt::parse(input).unwrap_or_else(|e| panic!("fixture SNBT {input}: {}", e.message()));
    parse_nbt(codec, &tag)
}

fn show<A: 'static>(r: DataResult<A>, ok: impl FnOnce(&A) -> String) -> String {
    match r.result_ref() {
        Some(v) => ok(v),
        None => format!("ERR {}", r.error_message().expect("an error")),
    }
}

fn extended(input: &str) -> Transformation {
    decode(&TRANSFORMATION_EXTENDED, input)
        .result()
        .unwrap_or_else(|| panic!("fixture input does not decode: {input}"))
}

fn run(op: &str, inputs: &[&str]) -> String {
    match op {
        "DEC" => show(decode(&TRANSFORMATION_EXTENDED, inputs[0]), dump),
        "CODEC" => show(decode(&TRANSFORMATION, inputs[0]), dump),
        "COMPOSE" => dump(&extended(inputs[0]).compose(&extended(inputs[1]))),
        "PARENT" => matrix(&Transformation::compose_with_parent(
            extended(inputs[0]).matrix(),
            Some(&extended(inputs[1])),
        )),
        "INVERSE" => extended(inputs[0])
            .inverse()
            .map_or("NULL".into(), |t| dump(&t)),
        "IDINV" => Transformation::identity()
            .inverse()
            .map_or("NULL".into(), |t| dump(&t)),
        "QUAT" => show(decode(&QUATERNIONF, inputs[0]), |q| {
            format!("OK {}", quat(q))
        }),
        "MAT" => show(decode(&MATRIX4F, inputs[0]), |m| {
            let encoded = encode_nbt(&MATRIX4F, m).result().expect("matrix encodes");
            format!("OK {} E={}", matrix(m), hex_all(&floats(&encoded)))
        }),
        "BR" => show(decode(&BRIGHTNESS, inputs[0]), |b: &Brightness| {
            format!("OK {} {} {}", b.block, b.sky, b.pack())
        }),
        "BILL" => show(decode(&BILLBOARD_CONSTRAINTS, inputs[0]), |b| {
            format!("OK {}", b.id())
        }),
        "BILLID" => BillboardConstraints::by_id(inputs[0].parse().expect("id"))
            .id()
            .to_string(),
        other => panic!("unknown fixture op {other}"),
    }
}

/// Whether a whitespace separated token is the hex of a float that is a NaN.
fn is_nan_lane(token: &str) -> bool {
    let hex = token.rsplit('=').next().unwrap_or(token);
    hex.len() == 8 && u32::from_str_radix(hex, 16).is_ok_and(|bits| f32::from_bits(bits).is_nan())
}

/// Results are equal when every token is: float lanes by their raw bits, except that two NaNs are equal whatever their
/// sign and payload. Which NaN an operation yields when an operand is NaN depends on the order the machine code takes the
/// operands in, which neither IEEE 754 nor the Java language fixes: the vanilla JVM itself prints different NaN bits for the
/// same input between its interpreter, its compilers and two runs of the oracle (every other lane is stable).
fn same(actual: &str, expected: &str) -> bool {
    let (a, e): (Vec<&str>, Vec<&str>) =
        (actual.split(' ').collect(), expected.split(' ').collect());
    a.len() == e.len()
        && a.iter()
            .zip(&e)
            .all(|(a, e)| a == e || (is_nan_lane(a) && is_nan_lane(e)))
}

#[test]
fn display_math_matches_vanilla() {
    let mut cases = 0;
    let mut failures = Vec::new();
    for (number, line) in GOLDEN.lines().enumerate() {
        let parts: Vec<&str> = line.split('\t').collect();
        let arrow = parts.iter().position(|p| *p == "=>").expect("=> separator");
        let actual = run(parts[0], &parts[1..arrow]);
        let expected = parts[arrow + 1..].join("\t");
        cases += 1;
        if !same(&actual, &expected) {
            failures.push(format!(
                "line {}: {}\n  expected {expected}\n  actual   {actual}",
                number + 1,
                parts[..arrow].join(" ")
            ));
        }
    }
    assert!(cases > 1000, "fixture has {cases} cases");
    assert!(
        failures.is_empty(),
        "{} of {cases} cases differ, first:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
}
