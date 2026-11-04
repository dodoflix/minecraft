//! Golden tests against vanilla: fixtures/random/RandomGen.java -> random.txt.
use mc_core::random::*;

const GOLDEN: &str = include_str!("fixtures/random/random.txt");

const SEEDS: [i64; 9] = [0, 1, -1, 42, 12345, -987654321, 0x123456789ABCDEF, i64::MAX, i64::MIN];
const BOUNDS: [i32; 11] = [1, 2, 3, 7, 16, 100, 1000, 65536, 1 << 30, i32::MAX, 12345678];

fn f(v: f32) -> String {
    format!("{:x}", v.to_bits())
}
fn d(v: f64) -> String {
    format!("{:x}", v.to_bits())
}

struct Out(Vec<String>);
impl Out {
    fn p(&mut self, tag: &str, v: impl std::fmt::Display) {
        self.0.push(format!("{tag} {v}"));
    }
}

fn make_wg(kind: &str, seed: i64) -> WorldgenRandom {
    let wg = |inner: Box<dyn RandomSource>| Box::new(WorldgenRandom::new(inner)) as Box<dyn RandomSource>;
    WorldgenRandom::new(match kind {
        "wg_legacy" => Box::new(LegacyRandomSource::new(seed)),
        "wg_xoroshiro" => Box::new(XoroshiroRandomSource::new(seed)),
        "wg_single" => Box::new(SingleThreadedRandomSource::new(seed)),
        "wg_threadsafe" => Box::new(ThreadSafeLegacyRandomSource::new(seed)),
        "wg_wg" => wg(Box::new(LegacyRandomSource::new(seed))),
        _ => unreachable!(),
    })
}

fn make(kind: &str, seed: i64) -> Box<dyn RandomSource> {
    match kind {
        "legacy" => Box::new(LegacyRandomSource::new(seed)),
        "threadsafe" => Box::new(ThreadSafeLegacyRandomSource::new(seed)),
        "single" => Box::new(SingleThreadedRandomSource::new(seed)),
        "xoroshiro" => Box::new(XoroshiroRandomSource::new(seed)),
        _ => unreachable!(),
    }
}

fn child(o: &mut Out, tag: &str, r: &mut dyn RandomSource) {
    o.p(&format!("{tag}.l"), format!("{} {} {}", r.next_long(), r.next_long(), r.next_long()));
    o.p(&format!("{tag}.i"), format!("{} {}", r.next_int(), r.next_int_bounded(100)));
    o.p(&format!("{tag}.d"), d(r.next_double()));
}

/// Same script as RandomGen.run.
fn run(o: &mut Out, kind: &str, seed: i64) {
    if kind.starts_with("wg_") {
        let mut w = make_wg(kind, seed);
        script(o, kind, seed, &mut w);
        o.p(&format!("{kind} {seed} count"), w.count());
    } else {
        script(o, kind, seed, &mut *make(kind, seed));
    }
}

fn script(o: &mut Out, kind: &str, seed: i64, r: &mut dyn RandomSource) {
    let t = format!("{kind} {seed} ");
    o.p(&format!("{t}int"), format!("{} {} {} {}", r.next_int(), r.next_int(), r.next_int(), r.next_int()));
    for b in BOUNDS {
        o.p(
            &format!("{t}bound{b}"),
            format!("{} {} {}", r.next_int_bounded(b), r.next_int_bounded(b), r.next_int_bounded(b)),
        );
    }
    o.p(
        &format!("{t}between_incl"),
        format!(
            "{} {} {}",
            r.next_int_between_inclusive(-5, 5),
            r.next_int_between_inclusive(10, 10),
            r.next_int_between_inclusive(0, 1000)
        ),
    );
    o.p(
        &format!("{t}between"),
        format!("{} {} {}", r.next_int_between(-100, 100), r.next_int_between(0, 1), r.next_int_between(5, 99999)),
    );
    o.p(&format!("{t}long"), format!("{} {} {}", r.next_long(), r.next_long(), r.next_long()));
    o.p(
        &format!("{t}bool"),
        format!("{} {} {} {}", r.next_boolean(), r.next_boolean(), r.next_boolean(), r.next_boolean()),
    );
    o.p(&format!("{t}float"), format!("{} {} {}", f(r.next_float()), f(r.next_float()), f(r.next_float())));
    o.p(&format!("{t}double"), format!("{} {} {}", d(r.next_double()), d(r.next_double()), d(r.next_double())));
    for i in 0..5 {
        o.p(&format!("{t}gauss{i}"), d(r.next_gaussian()));
    }
    r.set_seed(seed.wrapping_add(1));
    o.p(&format!("{t}gauss_after_reseed"), format!("{} {}", d(r.next_gaussian()), d(r.next_double())));
    r.next_gaussian();
    r.set_seed(seed);
    o.p(&format!("{t}gauss_after_reseed2"), d(r.next_gaussian()));
    o.p(&format!("{t}triangle"), format!("{} {}", d(r.triangle(1.5, 2.5)), f(r.triangle_f32(1.0, 3.0))));
    r.consume_count(7);
    o.p(&format!("{t}after_consume"), r.next_int());
    let mut fk = r.fork();
    child(o, &format!("{t}fork"), &mut *fk);
    o.p(&format!("{t}parent_after_fork"), format!("{} {}", r.next_long(), r.next_long()));
    let pf = r.fork_positional();
    let mut cfg = String::new();
    pf.parity_config_string(&mut cfg);
    o.p(&format!("{t}pf_cfg"), cfg);
    child(o, &format!("{t}at000"), &mut *pf.at(0, 0, 0));
    child(o, &format!("{t}at123"), &mut *pf.at(1, 2, 3));
    child(o, &format!("{t}atneg"), &mut *pf.at(-5, 100, -30000000));
    child(o, &format!("{t}atmax"), &mut *pf.at(i32::MAX, i32::MIN, 29999999));
    child(o, &format!("{t}hash_overworld"), &mut *pf.from_hash_of("minecraft:overworld"));
    child(o, &format!("{t}hash_utf"), &mut *pf.from_hash_of("h\u{e9}llo \u{1F600}"));
    child(o, &format!("{t}hash_empty"), &mut *pf.from_hash_of(""));
    child(o, &format!("{t}fromseed"), &mut *pf.from_seed(42));
    o.p(&format!("{t}end"), r.next_int());
}

fn java_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

fn expected() -> Vec<String> {
    let mut o = Out(Vec::new());
    for kind in ["legacy", "threadsafe", "single", "xoroshiro", "wg_legacy", "wg_xoroshiro", "wg_single", "wg_threadsafe", "wg_wg"] {
        for s in SEEDS {
            run(&mut o, kind, s);
        }
    }
    for s in SEEDS {
        let u = upgrade_seed_to_128bit(s);
        let um = upgrade_seed_to_128bit_unmixed(s);
        o.p(
            &format!("upgrade {s}"),
            format!("{} {} {} {} {}", u.seed_lo, u.seed_hi, um.seed_lo, um.seed_hi, mix_stafford13(s)),
        );
        o.p("xoro_zero", XoroshiroRandomSource::from_seeds(0, 0).next_long());
    }
    for s in ["", "a", "minecraft:overworld", "h\u{e9}llo \u{1F600}", "minecraft:ore_iron"] {
        let h = seed_from_hash_of(s);
        o.p(&format!("hash {} {}", s.encode_utf16().count(), java_hash(s)), format!("{} {}", h.seed_lo, h.seed_hi));
    }
    let coords = [0, 1, -3, 5, 1000, -30000000, 29999999, i32::MAX, i32::MIN];
    for alg in [Algorithm::Legacy, Algorithm::Xoroshiro] {
        let name = if alg == Algorithm::Legacy { "LEGACY" } else { "XOROSHIRO" };
        for s in SEEDS {
            for x in coords {
                for z in coords {
                    let t = format!("wg {name} {s} {x} {z} ");
                    let mut w = WorldgenRandom::new(alg.new_instance(0));
                    let dec = w.set_decoration_seed(s, x, z);
                    o.p(&format!("{t}decoration"), format!("{dec} {} {}", w.next_long(), w.count()));
                    w.set_feature_seed(s, x, z);
                    o.p(&format!("{t}feature"), w.next_long());
                    w.set_large_feature_seed(s, x, z);
                    o.p(&format!("{t}large"), w.next_long());
                    w.set_large_feature_with_salt(s, x, z, 10387312);
                    o.p(&format!("{t}large_salt"), w.next_long());
                    let mut sl = WorldgenRandom::seed_slime_chunk(x, z, s, 987234911);
                    o.p(&format!("{t}slime"), format!("{} {}", sl.next_int_bounded(10), sl.next_long()));
                }
            }
        }
    }
    o.0
}

#[test]
fn matches_vanilla_oracle() {
    let exp = expected();
    let gold: Vec<&str> = GOLDEN.lines().collect();
    assert_eq!(exp.len(), gold.len(), "line count");
    for (i, (e, g)) in exp.iter().zip(&gold).enumerate() {
        assert_eq!(e, g, "line {i}");
    }
}

#[test]
fn xoroshiro_codec_longs_roundtrip_and_zero_seed() {
    let g = Xoroshiro128PlusPlus::new(0, 0);
    assert_eq!(g.to_longs(), [GOLDEN_RATIO_64, SILVER_RATIO_64]);
    assert_eq!(Xoroshiro128PlusPlus::from_longs([5, 7]).to_longs(), [5, 7]);
}

#[test]
fn bounds_panic_like_vanilla() {
    assert!(std::panic::catch_unwind(|| LegacyRandomSource::new(0).next_int_bounded(0)).is_err());
    assert!(std::panic::catch_unwind(|| XoroshiroRandomSource::new(0).next_int_bounded(-1)).is_err());
    assert!(std::panic::catch_unwind(|| XoroshiroRandomSource::new(0).next_int_between(3, 3)).is_err());
}

#[test]
fn sources_are_send_sync() {
    fn send<T: Send>() {}
    fn sync<T: Sync>() {}
    send::<Box<dyn RandomSource>>();
    send::<WorldgenRandom>();
    send::<ThreadSafeLegacyRandomSource>();
    sync::<ThreadSafeLegacyRandomSource>();
    sync::<Box<dyn PositionalRandomFactory>>();
}
