//! Golden test: `Double.toString` / `Float.toString` (JDK 19+ shortest-decimal rules) and `Vec3` / `AABB` `toString`
//! against the vanilla oracle `fixtures/java_format/JavaFormat.java` (output: `java_format.txt`).
//!
//! Fixture lines: `d <bits16> <string>`, `f <bits8> <string>`, `v3 <3 x bits16> <string>`, `aabb <6 x bits16> <string>`.
//! (`Vec2` has no vanilla `toString` override, so there is nothing to compare.)

use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_core::phys::{AABB, Vec3};
use std::sync::LazyLock;

static FIXTURE: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/java_format/java_format.txt.gz"));

fn dbl(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).unwrap())
}

#[test]
fn java_format_matches_oracle() {
    let (mut nd, mut nf, mut nv, mut nb, mut bad) = (0, 0, 0, 0, Vec::new());
    for line in FIXTURE.lines() {
        let mut it = line.split(' ');
        let kind = it.next().unwrap();
        let (got, want) = match kind {
            "d" => {
                nd += 1;
                let h = it.next().unwrap();
                (
                    java_double_to_string(dbl(h)),
                    it.next().unwrap().to_string(),
                )
            }
            "f" => {
                nf += 1;
                let h = it.next().unwrap();
                (
                    java_float_to_string(f32::from_bits(u32::from_str_radix(h, 16).unwrap())),
                    it.next().unwrap().to_string(),
                )
            }
            "v3" => {
                nv += 1;
                let [x, y, z] = [(); 3].map(|_| dbl(it.next().unwrap()));
                (
                    Vec3::new(x, y, z).to_string(),
                    it.collect::<Vec<_>>().join(" "),
                )
            }
            "aabb" => {
                nb += 1;
                let [a, b, c, d, e, f] = [(); 6].map(|_| dbl(it.next().unwrap()));
                (
                    AABB::new(a, b, c, d, e, f).to_string(),
                    it.collect::<Vec<_>>().join(" "),
                )
            }
            k => panic!("unknown fixture kind {k}"),
        };
        if got != want {
            bad.push(format!("{line} | got {got}"));
        }
    }
    assert!(
        nd > 20000 && nf > 20000 && nv == 5 && nb == 4,
        "fixture counts {nd} {nf} {nv} {nb}"
    );
    assert!(
        bad.is_empty(),
        "{} mismatches, first: {:#?}",
        bad.len(),
        &bad[..bad.len().min(15)]
    );
}
