//! Differential test of the structure placement port (`structure_placement`) against the vanilla classes
//! (fixture `tests/fixtures/structure_placement.txt.gz` from `tests/oracle/StructurePlacementOracle.java`, inputs in
//! `tests/fixtures/structure_placement_cases.json`).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::java::java_float_to_string;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::{BiomeRegistry, BiomeSource, biome_source_from_json};
use mc_worldgen::data::{McDataSource, WorldgenRegistries};
use mc_worldgen::random_state::RandomState;
use mc_worldgen::structure_placement::{
    ChunkGeneratorStructureState, LocateResult, StructurePlacement, StructureSet,
    StructureSetHolder, StructureSetRegistry, find_nearest_map_structure,
    get_nearest_generated_structure_random_spread, get_nearest_generated_structure_rings,
    structure_biomes,
};

fn fixture() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/structure_placement.txt.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).expect("fixture"))
        .read_to_string(&mut text)
        .expect("gunzip");
    text.lines().map(str::to_owned).collect()
}

fn cases() -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/structure_placement_cases.json");
    serde_json::from_slice(&std::fs::read(path).expect("cases")).expect("cases json")
}

struct Env {
    registries: Arc<WorldgenRegistries>,
    biomes: BiomeRegistry,
    /// vanilla sets followed by the `registry` cases of the cases file
    sets: Arc<StructureSetRegistry>,
}

fn structure_exists(registries: &WorldgenRegistries) -> impl Fn(&Identifier) -> bool + '_ {
    |id| registries.raw("worldgen/structure", id).is_some()
}

fn env() -> Env {
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(McDataSource)));
    let biomes = BiomeRegistry::load(&registries).expect("biomes");
    let vanilla = StructureSetRegistry::load(&registries, &biomes).expect("structure sets");
    let mut entries: Vec<(Identifier, StructureSet)> = vanilla
        .iter()
        .map(|h| (h.id().clone(), h.value().clone()))
        .collect();
    for case in cases()["registry"].as_array().expect("registry") {
        let id = Identifier::parse(case["id"].as_str().unwrap()).unwrap();
        let set = StructureSet::from_json(&case["json"], &biomes, &structure_exists(&registries))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        entries.push((id, set));
    }
    let sets = Arc::new(StructureSetRegistry::from_entries(entries).expect("registry"));
    Env {
        registries,
        biomes,
        sets,
    }
}

fn biome_names(env: &Env, biomes: &[BiomeId]) -> String {
    biomes
        .iter()
        .map(|b| env.biomes.key(*b).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn placement_summary(env: &Env, p: &StructurePlacement) -> String {
    let spreading = match p {
        StructurePlacement::DimensionOrigin => return "origin".to_owned(),
        StructurePlacement::RandomSpread(r) => &r.spreading,
        StructurePlacement::ConcentricRings(r) => &r.spreading,
    };
    let o = spreading.locate_offset;
    let zone = spreading
        .exclusion_zone
        .as_ref()
        .map_or("-".to_owned(), |z| {
            format!("{}/{}", z.other_set, z.chunk_count)
        });
    let common = format!(
        " off={},{},{} m={} f={} salt={} ex={}",
        o.x,
        o.y,
        o.z,
        spreading.frequency_reduction_method.name(),
        java_float_to_string(spreading.frequency),
        spreading.salt,
        zone
    );
    match p {
        StructurePlacement::RandomSpread(r) => {
            format!(
                "rs{common} sp={} se={} t={}",
                r.spacing,
                r.separation,
                r.spread_type.name()
            )
        }
        StructurePlacement::ConcentricRings(r) => format!(
            "cr{common} d={} s={} c={} b={}",
            r.distance,
            r.spread,
            r.count,
            biome_names(env, &r.preferred_biomes)
        ),
        StructurePlacement::DimensionOrigin => unreachable!(),
    }
}

fn set_summary(env: &Env, s: &StructureSet) -> String {
    let mut out = String::from("[");
    for e in &s.structures {
        out.push_str(&format!("{}*{} ", e.structure, e.weight));
    }
    format!("{}] {}", out, placement_summary(env, &s.placement))
}

#[test]
fn codecs_match_vanilla() {
    let env = env();
    let cases = cases();
    let by_name: HashMap<&str, &serde_json::Value> = cases["codec"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap(), c))
        .collect();
    let exists = structure_exists(&env.registries);
    let (mut ok, mut err) = (0, 0);
    for line in fixture() {
        let Some(rest) = line.strip_prefix("K ") else {
            continue;
        };
        let (name, rest) = rest.split_once(' ').unwrap();
        let case = by_name[name];
        let json = &case["json"];
        let got: Result<String, String> = if case["kind"] == "placement" {
            StructurePlacement::from_json(json, &env.biomes)
                .map_err(|e| e.to_string())
                .and_then(|p| {
                    // `RegistryCodecs.holder`: the exclusion zone's set has to exist in the registry
                    let set = StructureSet {
                        structures: Vec::new(),
                        placement: p.clone(),
                    };
                    let mut entries: Vec<_> = env
                        .sets
                        .iter()
                        .map(|h| (h.id().clone(), h.value().clone()))
                        .collect();
                    entries.push((Identifier::parse("test:case").unwrap(), set));
                    StructureSetRegistry::from_entries(entries).map_err(|e| e.to_string())?;
                    Ok(placement_summary(&env, &p))
                })
        } else {
            StructureSet::from_json(json, &env.biomes, &exists)
                .map_err(|e| e.to_string())
                .and_then(|s| {
                    let mut entries: Vec<_> = env
                        .sets
                        .iter()
                        .map(|h| (h.id().clone(), h.value().clone()))
                        .collect();
                    entries.push((Identifier::parse("test:case").unwrap(), s.clone()));
                    StructureSetRegistry::from_entries(entries).map_err(|e| e.to_string())?;
                    Ok(set_summary(&env, &s))
                })
        };
        match (rest.strip_prefix("ok "), rest) {
            (Some(want), _) => {
                ok += 1;
                assert_eq!(got.as_deref(), Ok(want), "{name}: {json}");
            }
            (None, "err") => {
                err += 1;
                assert!(got.is_err(), "{name}: accepted {json}: {got:?}");
            }
            _ => panic!("{line}"),
        }
    }
    assert!(ok > 50 && err > 60, "{ok} {err}");
}

#[test]
fn registry_order_and_vanilla_sets() {
    let env = env();
    let lines: Vec<String> = fixture()
        .into_iter()
        .filter(|l| l.starts_with("L "))
        .collect();
    let vanilla: Vec<&StructureSetHolder> = env.sets.iter().take(lines.len()).collect();
    assert_eq!(lines.len(), 21);
    for (line, holder) in lines.iter().zip(vanilla) {
        let want = line.strip_prefix("L ").unwrap();
        assert_eq!(
            format!("{} {}", holder.id(), set_summary(&env, holder.value())),
            want
        );
    }
}

fn hash(h: i64, v: i64) -> i64 {
    h.wrapping_mul(1000003).wrapping_add(v)
}

#[test]
fn potential_structure_chunks_match_vanilla() {
    let env = env();
    let mut checked = 0;
    for line in fixture() {
        let Some(rest) = line.strip_prefix("Q ") else {
            continue;
        };
        let f: Vec<&str> = rest.split(' ').collect();
        let set = env.sets.get(&Identifier::parse(f[0]).unwrap()).unwrap();
        let StructurePlacement::RandomSpread(rs) = &set.placement else {
            panic!("{}", f[0])
        };
        let seed: i64 = f[1].parse().unwrap();
        let mut h = 0i64;
        for x in -100..100 {
            for z in -100..100 {
                let p = rs.get_potential_structure_chunk(seed, x, z);
                h = hash(hash(h, i64::from(p.x)), i64::from(p.z));
            }
        }
        assert_eq!(h.to_string(), f[2], "{} {}", f[0], seed);
        let samples = [
            (0, 0),
            (-1, -1),
            (199, -250),
            (-100000, 100000),
            (2000000, -2000000),
        ];
        for ((x, z), want) in samples.iter().zip(&f[3..]) {
            let p = rs.get_potential_structure_chunk(seed, *x, *z);
            assert_eq!(
                format!("{},{}", p.x, p.z),
                *want,
                "{} {} {x} {z}",
                f[0],
                seed
            );
        }
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
}

fn num(s: &str) -> i32 {
    s.parse().unwrap()
}

/// The oracle's fixed `StructureManager.checkStructurePresence` stand-in.
fn predicate_hit(
    seed: i64,
) -> impl FnMut(&[Identifier], &StructurePlacement, ChunkPos) -> LocateResult {
    move |structures, placement, chunk| {
        let hit = (i64::from(chunk.x) * 31)
            .wrapping_add(i64::from(chunk.z) * 17)
            .wrapping_add(seed)
            .rem_euclid(5)
            == 0;
        hit.then(|| (placement.get_locate_pos(chunk), structures[0].clone()))
    }
}

fn pair(r: LocateResult) -> String {
    r.map_or("null".to_owned(), |(p, s)| {
        format!("{} {} {} {s}", p.x(), p.y(), p.z())
    })
}

/// The structure lists of the oracle's `findNearestMapStructure` variants.
fn variant(dim: &str, name: &str) -> &'static [&'static str] {
    match (dim, name) {
        ("nether", "nether") => &["minecraft:fortress", "minecraft:bastion_remnant"],
        ("nether", "fort") => &["minecraft:fortress"],
        ("nether", "fossil") => &["minecraft:nether_fossil", "minecraft:ruined_portal_nether"],
        ("end", "end") => &["minecraft:end_city"],
        (_, "villages") => &[
            "minecraft:village_plains",
            "minecraft:village_desert",
            "minecraft:pillager_outpost",
        ],
        (_, "stronghold") => &["minecraft:stronghold"],
        (_, "mixed") => &[
            "minecraft:stronghold",
            "minecraft:village_plains",
            "minecraft:mansion",
            "minecraft:monument",
        ],
        (_, "rare") => &["minecraft:mansion"],
        (_, "twice") => &[
            "minecraft:village_plains",
            "minecraft:village_plains",
            "minecraft:stronghold",
        ],
        (_, "custom") => &[
            "minecraft:village_plains",
            "minecraft:mineshaft",
            "minecraft:fortress",
        ],
        (_, "none") => &["minecraft:end_city"],
        other => panic!("{other:?}"),
    }
}

fn dimension(env: &Env, name: &str) -> (Arc<dyn BiomeSource>, &'static str) {
    let json = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
    let (source, settings) = match name {
        "overworld" | "flat" => (
            r#"{"type":"minecraft:multi_noise","preset":"minecraft:overworld"}"#,
            "overworld",
        ),
        "nether" => (
            r#"{"type":"minecraft:multi_noise","preset":"minecraft:nether"}"#,
            "nether",
        ),
        "end" => (r#"{"type":"minecraft:the_end"}"#, "end"),
        other => panic!("{other}"),
    };
    (
        biome_source_from_json(&json(source), &env.registries, &env.biomes).unwrap(),
        settings,
    )
}

#[test]
fn structure_state_matches_vanilla() {
    let env = env();
    let mut groups: BTreeMap<(String, i64), Vec<Vec<String>>> = BTreeMap::new();
    for line in fixture() {
        if matches!(
            line.split(' ').next(),
            Some("P" | "I" | "H" | "R" | "Z" | "Y" | "T" | "C" | "M")
        ) {
            let f: Vec<String> = line.split(' ').map(str::to_owned).collect();
            groups
                .entry((f[1].clone(), f[2].parse().unwrap()))
                .or_default()
                .push(f);
        }
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    let structure_biomes_of =
        |id: &Identifier| structure_biomes(&env.registries, &env.biomes, id).unwrap();
    for ((dim, seed), lines) in &groups {
        let (source, settings) = dimension(&env, dim);
        let noise_settings = env
            .registries
            .noise_settings(&Identifier::parse(settings).unwrap())
            .unwrap();
        let random_state = Arc::new(RandomState::create(
            env.registries.clone(),
            *seed,
            &noise_settings,
        ));
        let origin = ChunkPos::new(((*seed & 7) as i32) - 3, (((*seed >> 3) & 7) as i32) - 3);
        let state = if dim == "flat" {
            let wanted = [
                "minecraft:strongholds",
                "minecraft:villages",
                "test:rings_a",
                "test:rings_b",
                "test:freq_default_0_5",
            ];
            let overrides: Vec<StructureSetHolder> = env
                .sets
                .iter()
                .filter(|h| wanted.contains(&h.id().to_string().as_str()))
                .cloned()
                .collect();
            ChunkGeneratorStructureState::create_for_flat(
                random_state,
                *seed,
                origin,
                source,
                env.sets.clone(),
                overrides,
                &structure_biomes_of,
            )
        } else {
            ChunkGeneratorStructureState::create_for_normal(
                random_state,
                *seed,
                origin,
                source,
                env.sets.clone(),
                &structure_biomes_of,
            )
        };
        for f in lines {
            let ctx = format!("{dim} {seed}: {}", f[..f.len().min(6)].join(" "));
            *counts.entry(f[0].clone()).or_default() += 1;
            match f[0].as_str() {
                "P" => {
                    let got: Vec<String> = state
                        .possible_structure_sets()
                        .iter()
                        .map(|h| h.id().to_string())
                        .collect();
                    assert_eq!(got, f[3..], "{ctx}");
                }
                "I" => {
                    let set = env.sets.get(&Identifier::parse(&f[3]).unwrap()).unwrap();
                    let (mut count, mut h, mut first) = (0, 0i64, Vec::new());
                    for x in -60..60 {
                        for z in -60..60 {
                            if state.is_structure_chunk(set, x, z) {
                                count += 1;
                                h = hash(h, i64::from(x) * 65537 + i64::from(z));
                                if count <= 5 {
                                    first.push(format!("{x},{z}"));
                                }
                            }
                        }
                    }
                    assert_eq!(count.to_string(), f[4], "{ctx} count");
                    assert_eq!(h.to_string(), f[5], "{ctx} hash");
                    assert_eq!(first, f[6..], "{ctx} first");
                }
                "H" => {
                    let set = env.sets.get(&Identifier::parse(&f[3]).unwrap()).unwrap();
                    let got = state.has_structure_chunk_in_range(
                        set,
                        f[4].parse().unwrap(),
                        f[5].parse().unwrap(),
                        f[6].parse().unwrap(),
                    );
                    assert_eq!(got.to_string(), f[7], "{ctx}");
                }
                "R" | "Z" => {
                    let set = env.sets.get(&Identifier::parse(&f[3]).unwrap()).unwrap();
                    let StructurePlacement::ConcentricRings(rings) = &set.placement else {
                        panic!("{ctx}")
                    };
                    let positions = state.get_ring_positions_for(rings);
                    if f[0] == "R" {
                        let got: Vec<String> = positions
                            .map(|p| p.iter().map(|c| format!("{},{}", c.x, c.z)).collect())
                            .unwrap_or_else(|| vec!["null".to_owned()]);
                        assert_eq!(got, f[4..], "{ctx}");
                    } else {
                        let positions = positions.unwrap();
                        let a = positions
                            .iter()
                            .filter(|p| state.is_structure_chunk(set, p.x, p.z))
                            .count();
                        let b = positions
                            .iter()
                            .filter(|p| state.is_structure_chunk(set, p.x + 1, p.z))
                            .count();
                        assert_eq!(
                            (a.to_string(), b.to_string()),
                            (f[4].clone(), f[5].clone()),
                            "{ctx}"
                        );
                    }
                }
                "Y" => {
                    let got: Vec<String> = state
                        .get_placements_for_structure(&Identifier::parse(&f[3]).unwrap())
                        .iter()
                        .map(|h| h.id().to_string())
                        .collect();
                    assert_eq!(got, f[4..], "{ctx}");
                }
                "T" => {
                    let set = env.sets.get(&Identifier::parse(&f[3]).unwrap()).unwrap();
                    let StructurePlacement::RandomSpread(rs) = &set.placement else {
                        panic!("{ctx}")
                    };
                    let structures = [set.structures[0].structure.clone()];
                    let (ox, oz, radius) = (num(&f[4]), num(&f[5]), num(&f[6]));
                    let got = get_nearest_generated_structure_random_spread(
                        &structures,
                        set,
                        rs,
                        *seed,
                        ox,
                        oz,
                        radius,
                        &mut predicate_hit(*seed),
                    );
                    assert_eq!(pair(got), f[7..].join(" "), "{ctx}");
                }
                "C" => {
                    let set = env.sets.get(&Identifier::parse(&f[3]).unwrap()).unwrap();
                    let StructurePlacement::ConcentricRings(rings) = &set.placement else {
                        panic!("{ctx}")
                    };
                    let structures = [set.structures[0].structure.clone()];
                    let pos = BlockPos::new(num(&f[4]), 64, num(&f[5]));
                    let got = get_nearest_generated_structure_rings(
                        &state,
                        &structures,
                        set,
                        rings,
                        pos,
                        &mut predicate_hit(*seed),
                    );
                    assert_eq!(pair(got), f[6..].join(" "), "{ctx}");
                }
                "M" => {
                    let wanted: Vec<Identifier> = variant(dim, &f[3])
                        .iter()
                        .map(|s| Identifier::parse(s).unwrap())
                        .collect();
                    let pos = BlockPos::new(num(&f[4]), 64, num(&f[5]));
                    let got = find_nearest_map_structure(
                        &state,
                        &wanted,
                        pos,
                        num(&f[6]),
                        &mut predicate_hit(*seed),
                    );
                    assert_eq!(pair(got), f[7..].join(" "), "{ctx}");
                }
                other => panic!("{other}"),
            }
        }
    }
    for kind in ["P", "I", "H", "R", "Z", "Y", "T", "C", "M"] {
        assert!(counts[kind] > 15, "{kind}: {}", counts[kind]);
    }
}
