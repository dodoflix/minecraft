//! The vanilla data pack decodes completely into the feature/structure registries (a failing load is a startup failure,
//! as in vanilla), and the decoded registries agree with the real vanilla registries: entry counts, `Registry<Structure>`
//! order per generation step (the structure loop of `applyBiomeDecoration`), the per-step feature lists of every biome
//! and the `FeatureSorter` step sizes of the overworld/nether/end biome sources.
//! Fixture: `tests/fixtures/decoration/registries.txt.gz` from `tests/oracle/RegistryDecodeOracle.java`.

mod decoration_common;

use decoration_common::feature_name;
use flate2::read::GzDecoder;
use mc_core::ident::Identifier;
use mc_game::world::level::levelgen::decoration::sort_features_per_step;
use mc_game::world::level::levelgen::registries::GenRegistries;
use mc_game::world::level::levelgen::structure::registries::StructureRegistries;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{FsDataSource, WorldgenDataSource, WorldgenRegistries};
use mc_worldgen::structure_placement::StructureSetRegistry;
use mc_worldgen::world_preset::level_generator_from_json;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

fn golden() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decoration/registries.txt.gz");
    let mut text = String::new();
    GzDecoder::new(std::fs::File::open(path).unwrap()).read_to_string(&mut text).unwrap();
    text
}

// Fails until every structure type is ported (`level_factory` then stops falling back to terrain only).
#[test]
#[ignore = "structure types missing: jigsaw, mineshaft, fortress, stronghold, ocean_monument, woodland_mansion, end_city"]
fn vanilla_data_pack_registries_match_vanilla() {
    let text = golden();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root));
    let worldgen = Arc::new(WorldgenRegistries::new(source.clone()));
    let biomes = Arc::new(BiomeRegistry::load(&worldgen).unwrap());
    let features = GenRegistries::load(source.clone(), &worldgen, &biomes).unwrap_or_else(|e| panic!("feature registries: {e}"));
    let structures = StructureRegistries::load(source, &worldgen, &biomes).unwrap_or_else(|e| panic!("structure registries: {e}"));
    let sets = StructureSetRegistry::load(&worldgen, &biomes).unwrap_or_else(|e| panic!("structure sets: {e}"));

    let mut counts = 0;
    for line in text.lines().filter_map(|l| l.strip_prefix("COUNT ")) {
        let (registry, n) = line.split_once(' ').unwrap();
        let n: usize = n.parse().unwrap();
        let got = match registry {
            "minecraft:worldgen/biome" => biomes.len(),
            "minecraft:worldgen/feature" => features.features.len(),
            "minecraft:worldgen/placed_feature" => features.placed_features.len(),
            "minecraft:worldgen/structure" => structures.structures.len(),
            "minecraft:worldgen/structure_set" => sets.len(),
            "minecraft:worldgen/processor_list" => structures.processor_lists.len(),
            // pools are decoded on demand by the jigsaw structures that reference them
            "minecraft:worldgen/template_pool" => continue,
            other => panic!("unknown registry {other}"),
        };
        assert_eq!(got, n, "{registry} entry count");
        counts += 1;
    }
    assert_eq!(counts, 6);

    // the structure loop of `applyBiomeDecoration`: `Registry<Structure>` order within each step
    for line in text.lines().filter_map(|l| l.strip_prefix("STRUCTURES ")) {
        let (step, ids) = line.split_once(' ').unwrap();
        let got: Vec<String> = structures.structures_for_step(step.parse().unwrap()).iter().map(|h| h.id().unwrap().to_string()).collect();
        assert_eq!(got.join(","), ids, "structures of step {step}");
    }

    // the feature lists of every biome (`BiomeGenerationSettings.features()`)
    let mut biome_lines = 0;
    for line in text.lines().filter_map(|l| l.strip_prefix("BIOME ")) {
        let mut parts = line.splitn(3, ' ');
        let (id, step, expected) = (parts.next().unwrap(), parts.next().unwrap().parse::<usize>().unwrap(), parts.next().unwrap_or(""));
        let biome = biomes.id_of(&Identifier::parse(id).unwrap()).unwrap_or_else(|| panic!("biome {id}"));
        let got: Vec<String> = biomes.get(biome).generation_settings().features()[step].iter().map(feature_name).collect();
        assert_eq!(got.join(","), expected, "{id} step {step}");
        biome_lines += 1;
    }
    assert_eq!(biome_lines, biomes.ids().map(|b| biomes.get(b).generation_settings().features().len()).sum::<usize>());

    // every placed feature a biome lists is a registered, decoded placed feature
    for biome in biomes.ids() {
        for feature in biomes.get(biome).generation_settings().features().iter().flatten() {
            if let mc_worldgen::biome::PlacedFeatureRef::Named(id) = feature {
                assert!(features.placed_feature(id).is_some(), "{id}");
            }
        }
    }

    // FeatureSorter step sizes of the dimensions' biome sources
    let preset = worldgen.raw("worldgen/world_preset", &Identifier::parse("minecraft:normal").unwrap()).unwrap();
    let mut sorted = 0;
    for line in text.lines().filter_map(|l| l.strip_prefix("SORTED ")) {
        let mut parts = line.split(' ');
        let dimension = parts.next().unwrap();
        let steps: usize = parts.next().unwrap().parse().unwrap();
        let sizes: Vec<usize> = parts.map(|s| s.parse().unwrap()).collect();
        assert_eq!(sizes.len(), steps);
        let generator = level_generator_from_json(&preset["dimensions"][format!("minecraft:{dimension}")]["generator"], &worldgen, &biomes).unwrap();
        let per_step = sort_features_per_step(generator.into_boxed().as_ref(), &biomes).unwrap();
        assert_eq!(per_step.iter().map(Vec::len).collect::<Vec<_>>(), sizes, "{dimension} feature counts per step");
        sorted += 1;
    }
    assert_eq!(sorted, 3);
}
