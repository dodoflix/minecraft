//! Differential test of the biome registry, the preset parameter lists, the climate R-tree, the biome
//! temperature queries and `BiomeManager.obfuscateSeed` against the vanilla classes
//! (fixture `tests/fixtures/biome_params.txt.gz` from `tests/oracle/BiomeParamsOracle.java`).

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use mc_core::pos::block_pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_worldgen::biome::climate::{Parameter, ParameterList, ParameterPoint, TargetPoint};
use mc_worldgen::biome::{BiomeManager, BiomeRegistry, Preset};
use mc_worldgen::data::{McDataSource, WorldgenRegistries};
use mc_world::palette::BiomeId;

fn fixture() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/biome_params.txt.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).expect("fixture"))
        .read_to_string(&mut text)
        .expect("gunzip");
    text.lines().map(str::to_owned).collect()
}

fn registries() -> WorldgenRegistries {
    WorldgenRegistries::new(Arc::new(McDataSource))
}

fn num(s: &str) -> i64 {
    s.parse().expect("number")
}

#[test]
fn registry_order_and_preset_lists_match_vanilla() {
    let lines = fixture();
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).expect("biomes load");
    let names = biomes.names();
    let mut r_count = 0;
    for line in lines.iter().filter(|l| l.starts_with("R ")) {
        let f: Vec<&str> = line.split(' ').collect();
        assert_eq!(names[num(f[1]) as usize], f[2], "registry id {}", f[1]);
        r_count += 1;
    }
    assert_eq!(r_count, biomes.len());
    assert_eq!(r_count, 67);

    let mut expected: HashMap<&str, Vec<Vec<i64>>> = HashMap::new();
    for line in lines.iter().filter(|l| l.starts_with("L ")) {
        let f: Vec<&str> = line.split(' ').collect();
        assert_eq!(num(f[2]) as usize, expected.entry(f[1]).or_default().len());
        expected.entry(f[1]).or_default().push(f[3..].iter().map(|v| num(v)).collect());
    }
    for (name, preset) in [("nether", Preset::Nether), ("overworld", Preset::Overworld)] {
        let want = &expected[name];
        let list = preset.parameter_list(|key| biomes.id_of_name(key).expect("biome key"));
        assert_eq!(list.values().len(), want.len(), "{name} entry count");
        for (i, ((point, biome), want)) in list.values().iter().zip(want).enumerate() {
            let got = [
                i64::from(biome.0),
                point.temperature.min, point.temperature.max,
                point.humidity.min, point.humidity.max,
                point.continentalness.min, point.continentalness.max,
                point.erosion.min, point.erosion.max,
                point.depth.min, point.depth.max,
                point.weirdness.min, point.weirdness.max,
                point.offset,
            ];
            assert_eq!(got.as_slice(), want.as_slice(), "{name} entry {i}");
        }
    }
    assert_eq!(expected["overworld"].len(), 7594);
    assert_eq!(Preset::Overworld.used_biomes().len() > 40, true);
}

fn synthetic_list(count: usize, children_per_node: usize) -> (ParameterList<usize>, LegacyRandomSource) {
    let mut r = LegacyRandomSource::new(5 + count as i64);
    let mut values = Vec::new();
    for i in 0..count {
        let mut ps = [Parameter::new(0, 0); 6];
        for p in ps.iter_mut() {
            let min = i64::from(r.next_int_bounded(9) - 4) * 2500;
            let width = i64::from(r.next_int_bounded(3)) * 2500;
            *p = Parameter::new(min, min + width);
        }
        let offset = i64::from(r.next_int_bounded(2)) * 1000;
        values.push((
            ParameterPoint {
                temperature: ps[0],
                humidity: ps[1],
                continentalness: ps[2],
                erosion: ps[3],
                depth: ps[4],
                weirdness: ps[5],
                offset,
            },
            i,
        ));
    }
    (ParameterList::new(values).rebuild_with_children_per_node(children_per_node), r)
}

#[test]
fn rtree_searches_match_vanilla() {
    let lines = fixture();
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).expect("biomes load");
    let mut lists: HashMap<(String, usize), ParameterList<usize>> = HashMap::new();
    let mut checked = 0usize;
    for line in lines.iter().filter(|l| l.starts_with("Q ")) {
        let f: Vec<&str> = line.split(' ').collect();
        let (name, cpn) = (f[1].to_owned(), f[2].parse::<usize>().expect("cpn"));
        let list = lists.entry((name.clone(), cpn)).or_insert_with(|| {
            let list = match name.as_str() {
                "nether" => Preset::Nether.parameter_list(|k| k.len()),
                "overworld" => Preset::Overworld.parameter_list(|k| k.len()),
                other => {
                    let count: usize = other.trim_start_matches("synthetic").parse().expect("count");
                    return synthetic_list(count, cpn).0;
                }
            };
            // the preset lists store the entry index as value
            let values = list.values().iter().enumerate().map(|(i, (p, _))| (*p, i)).collect();
            let list = ParameterList::new(values);
            if cpn == 19 { list } else { list.rebuild_with_children_per_node(cpn) }
        });
        let t: Vec<i64> = f[3..9].iter().map(|v| num(v)).collect();
        let target = TargetPoint {
            temperature: t[0],
            humidity: t[1],
            continentalness: t[2],
            erosion: t[3],
            depth: t[4],
            weirdness: t[5],
        };
        let want: usize = f[9].parse().expect("index");
        assert_eq!(list.find_value_index(&target), want, "{line}");
        checked += 1;
    }
    assert!(checked > 20000);
    let _ = biomes;
}

#[test]
fn obfuscate_seed_matches_vanilla() {
    let mut n = 0;
    for line in fixture().iter().filter(|l| l.starts_with("O ")) {
        let f: Vec<&str> = line.split(' ').collect();
        assert_eq!(BiomeManager::obfuscate_seed(num(f[1])), num(f[2]), "seed {}", f[1]);
        n += 1;
    }
    assert!(n > 40);
}

#[test]
fn biome_temperature_queries_match_vanilla() {
    let registries = registries();
    let biomes = BiomeRegistry::load(&registries).expect("biomes load");
    let mut n = 0;
    for line in fixture().iter().filter(|l| l.starts_with("B ")) {
        let f: Vec<&str> = line.split(' ').collect();
        let biome = biomes.get(BiomeId(num(f[1]) as u32));
        let pos = BlockPos::new(num(f[2]) as i32, num(f[3]) as i32, num(f[4]) as i32);
        let sea = num(f[5]) as i32;
        let temp = biome.get_height_adjusted_temperature(pos, sea);
        assert_eq!(format!("{:x}", temp.to_bits()), f[6], "{line}");
        assert_eq!(biome.cold_enough_to_snow(pos, sea), f[7] == "1", "{line}");
        assert_eq!(biome.warm_enough_to_rain(pos, sea), f[8] == "1", "{line}");
        assert_eq!(biome.should_melt_frozen_ocean_iceberg_slightly(pos, sea), f[9] == "1", "{line}");
        assert_eq!(biome.get_precipitation_at(pos, sea) as i32, f[10].parse::<i32>().expect("ordinal"), "{line}");
        n += 1;
    }
    assert!(n > 3000);
}
