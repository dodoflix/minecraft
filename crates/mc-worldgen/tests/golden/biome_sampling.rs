//! Differential test of climate sampling, biome resolvers (point and chunk-volume paths), `BiomeManager`, the biome
//! searches and the spawn finder against the vanilla classes
//! (fixture `tests/fixtures/biome_sampling.txt.gz` from `tests/oracle/BiomeSamplingOracle.java`).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::random::LegacyRandomSource;
use mc_world::chunk::LevelHeight;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::{
    BiomeManager, BiomeRegistry, BiomeSource, NoiseSpawnFinder, biome_source_from_json,
};
use mc_worldgen::data::{McDataSource, WorldgenRegistries};
use mc_worldgen::densityfunction::SamplerContext;
use mc_worldgen::random_state::RandomState;

fn fixture() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/biome_sampling.txt.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).expect("fixture"))
        .read_to_string(&mut text)
        .expect("gunzip");
    text.lines().map(str::to_owned).collect()
}

fn num(s: &str) -> i64 {
    s.parse().expect("number")
}

struct Case {
    source: Arc<dyn BiomeSource>,
    settings: &'static str,
    min_quart_y: i32,
    size_y: i32,
}

fn cases(registries: &WorldgenRegistries, biomes: &BiomeRegistry) -> BTreeMap<&'static str, Case> {
    let json = |s: &str| serde_json::from_str::<serde_json::Value>(s).expect("json");
    let mk = |source: &str, settings, min_quart_y, size_y| Case {
        source: biome_source_from_json(&json(source), registries, biomes).expect("source"),
        settings,
        min_quart_y,
        size_y,
    };
    let overworld = r#"{"type":"minecraft:multi_noise","preset":"minecraft:overworld"}"#;
    BTreeMap::from([
        ("overworld", mk(overworld, "overworld", -16, 96)),
        ("large_biomes", mk(overworld, "large_biomes", -16, 96)),
        ("amplified", mk(overworld, "amplified", -16, 96)),
        (
            "nether",
            mk(
                r#"{"type":"minecraft:multi_noise","preset":"minecraft:nether"}"#,
                "nether",
                0,
                32,
            ),
        ),
        ("end", mk(r#"{"type":"minecraft:the_end"}"#, "end", 0, 32)),
        (
            "fixed",
            mk(
                r#"{"type":"minecraft:fixed","biome":"minecraft:plains"}"#,
                "overworld",
                -16,
                96,
            ),
        ),
        (
            "checker",
            mk(
                r#"{"type":"minecraft:checkerboard","scale":3,"biomes":["minecraft:plains","minecraft:forest","minecraft:desert","minecraft:ocean","minecraft:jungle","minecraft:forest"]}"#,
                "overworld",
                -16,
                96,
            ),
        ),
        (
            "checker_default",
            mk(
                r#"{"type":"minecraft:checkerboard","biomes":["minecraft:badlands","minecraft:swamp","minecraft:taiga"]}"#,
                "overworld",
                -16,
                96,
            ),
        ),
    ])
}

fn result(r: Option<(BlockPos, BiomeId)>) -> String {
    match r {
        None => "null".to_owned(),
        Some((p, b)) => format!("{} {} {} {}", p.x(), p.y(), p.z(), b.0),
    }
}

#[test]
fn biome_sampling_matches_vanilla() {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = BiomeRegistry::load(&registries).expect("biomes");
    let cases = cases(&registries, &biomes);
    let lines = fixture();

    let mut groups: BTreeMap<(String, i64), Vec<Vec<&str>>> = BTreeMap::new();
    let mut spawns = Vec::new();
    for line in &lines {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "P" => {
                let case = &cases[f[1]];
                let got: Vec<u32> = case.source.possible_biomes().iter().map(|b| b.0).collect();
                let want: Vec<u32> = f[2..].iter().map(|v| num(v) as u32).collect();
                assert_eq!(got, want, "possible biomes of {}", f[1]);
            }
            "S" => spawns.push(f),
            _ => groups
                .entry((f[1].to_owned(), num(f[2])))
                .or_default()
                .push(f),
        }
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for ((name, seed), group) in &groups {
        let case = &cases[name.as_str()];
        let settings = registries
            .noise_settings(&Identifier::parse(case.settings).expect("id"))
            .expect("noise settings");
        let rs = Arc::new(RandomState::create(registries.clone(), *seed, &settings));
        let resolver = case.source.create_uncached_resolver(&rs);
        let sampler = rs.create_climate_sampler(SamplerContext::empty_uncached());
        let manager = BiomeManager::new(&*resolver, BiomeManager::obfuscate_seed(*seed));
        let allowed = |b: BiomeId| b.0 % 4 == 1;
        let any = |_: BiomeId| true;
        let level = LevelHeight::new(case.min_quart_y * 4, case.size_y * 4);
        for f in group {
            let ctx = f.join(" ");
            *counts.entry(f[0]).or_default() += 1;
            match f[0] {
                "T" => {
                    let (qx, qy, qz) = (num(f[3]) as i32, num(f[4]) as i32, num(f[5]) as i32);
                    let t = sampler.sample(qx, qy, qz);
                    let got = [
                        t.temperature,
                        t.humidity,
                        t.continentalness,
                        t.erosion,
                        t.depth,
                        t.weirdness,
                    ];
                    let want: Vec<i64> = f[6..12].iter().map(|v| num(v)).collect();
                    assert_eq!(got.as_slice(), want.as_slice(), "{ctx}");
                    assert_eq!(
                        i64::from(resolver.get_noise_biome(qx, qy, qz).0),
                        num(f[12]),
                        "{ctx}"
                    );
                }
                "K" => {
                    let b = resolver.get_noise_biome(
                        num(f[3]) as i32,
                        num(f[4]) as i32,
                        num(f[5]) as i32,
                    );
                    assert_eq!(i64::from(b.0), num(f[6]), "{ctx}");
                }
                "M" => {
                    let b = manager.get_biome(BlockPos::new(
                        num(f[3]) as i32,
                        num(f[4]) as i32,
                        num(f[5]) as i32,
                    ));
                    assert_eq!(i64::from(b.0), num(f[6]), "{ctx}");
                }
                "V" => {
                    let (cx, cz) = (num(f[3]) as i32, num(f[4]) as i32);
                    let (min_qy, size_y) = (num(f[5]) as i32, num(f[6]) as i32);
                    let context = SamplerContext::builder().enable_caches().build();
                    let cs = rs.create_climate_sampler(context);
                    let (min_qx, min_qz) = ((cx * 16) >> 2, (cz * 16) >> 2);
                    let cr = case
                        .source
                        .create_resolver_for_chunk(cs, min_qx, min_qy, min_qz, 4, size_y, 4);
                    let mut got = String::new();
                    for x in 0..4 {
                        for z in 0..4 {
                            for y in 0..size_y {
                                got.push(
                                    (b'0'
                                        + cr.get_noise_biome(min_qx + x, min_qy + y, min_qz + z).0
                                            as u8) as char,
                                );
                            }
                        }
                    }
                    assert_eq!(got, f[7], "{ctx}");
                }
                "H" => {
                    let (x, y, z) = (num(f[4]) as i32, num(f[5]) as i32, num(f[6]) as i32);
                    let source = &case.source;
                    let got = match f[3] {
                        "r400" => source.find_biome_horizontal(
                            x,
                            y,
                            z,
                            400,
                            &allowed,
                            &mut LegacyRandomSource::new(*seed),
                            &rs,
                        ),
                        "r400any" => source.find_biome_horizontal(
                            x,
                            y,
                            z,
                            400,
                            &any,
                            &mut LegacyRandomSource::new(*seed),
                            &rs,
                        ),
                        "closest" => source.find_biome_horizontal_stepped(
                            x,
                            y,
                            z,
                            2000,
                            8,
                            &allowed,
                            &mut LegacyRandomSource::new(*seed),
                            true,
                            &rs,
                        ),
                        "skip" => source.find_biome_horizontal_stepped(
                            x,
                            y,
                            z,
                            1200,
                            4,
                            &allowed,
                            &mut LegacyRandomSource::new(seed.wrapping_add(1)),
                            false,
                            &rs,
                        ),
                        other => panic!("variant {other}"),
                    };
                    assert_eq!(result(got), f[7..].join(" "), "{ctx}");
                }
                "D" | "D2" => {
                    let origin =
                        BlockPos::new(num(f[3]) as i32, num(f[4]) as i32, num(f[5]) as i32);
                    let (radius, hres, vres) = if f[0] == "D" {
                        (1600, 32, 64)
                    } else {
                        (600, 16, 24)
                    };
                    let got = case
                        .source
                        .find_closest_biome_3d(origin, radius, hres, vres, &allowed, &rs, level);
                    assert_eq!(result(got), f[6..].join(" "), "{ctx}");
                }
                other => panic!("unknown line kind {other}"),
            }
        }
    }
    for kind in ["T", "K", "M", "V", "H", "D", "D2"] {
        assert!(counts[kind] > 20, "{kind}: {}", counts[kind]);
    }

    for f in spawns {
        let settings = registries
            .noise_settings(&Identifier::parse(f[1]).expect("id"))
            .expect("noise settings");
        let seed = num(f[2]);
        let rs = Arc::new(RandomState::create(registries.clone(), seed, &settings));
        let samplers = rs.samplers_with_context(SamplerContext::builder().enable_caches().build());
        let pos = NoiseSpawnFinder::find_spawn_position(&settings.spawn_target, &samplers);
        assert_eq!(
            (i64::from(pos.x()), i64::from(pos.z())),
            (num(f[3]), num(f[4])),
            "spawn {}",
            f.join(" ")
        );
    }
}
