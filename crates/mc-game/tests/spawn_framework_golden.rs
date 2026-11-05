//! Spawning data classes against the real vanilla classes (`tests/fixtures/SpawnFrameworkGolden.java`, regenerate with the
//! command in its header): `MobCategory` constants, the `SpawnPlacements` table, `PotentialCalculator` charge sums and the
//! `MobSpawnSettings` of every vanilla biome (decoded through the biome registry's `BiomeSpawnTable`).

use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_data::EntityType;
use mc_data::enums::MobCategory;
use mc_game::world::entity::spawn_placements;
use mc_game::world::level::spawner::MobCategoryExt;
use mc_game::world::level::spawner::biome_spawns::{BiomeSpawnTable, DimensionSpawnBase};
use mc_game::world::level::spawner::mob_spawn_settings::EMPTY;
use mc_game::world::level::spawner::potential_calculator::PotentialCalculator;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{FsDataSource, WorldgenRegistries};
use std::path::PathBuf;
use std::sync::Arc;

const GOLDEN: &str = include_str!("fixtures/spawn_framework_golden.txt");

fn section<'a>(prefix: &'a str) -> impl Iterator<Item = &'a str> {
    GOLDEN.lines().filter(move |l| l.starts_with(prefix))
}

#[test]
fn mob_categories() {
    let expected: Vec<&str> = section("category ").take(8).collect();
    let actual: Vec<String> = MobCategory::ALL
        .iter()
        .map(|c| format!("category {} {} {} {} {} {}", c.get_name(), c.get_max_instances_per_chunk(), c.is_friendly(), c.is_persistent(), c.get_despawn_distance(), c.get_no_despawn_distance()))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn spawn_placement_table() {
    let mut expected: Vec<&str> = section("placement ").collect();
    let mut actual: Vec<String> = EntityType::iter()
        .map(|ty| {
            let placement = spawn_placements::get_placement_type(ty).name();
            let heightmap = ty.data().spawn_placement.as_ref().map_or("MOTION_BLOCKING_NO_LEAVES", |p| p.heightmap.name());
            format!("placement {} {placement} {heightmap}", ty.name())
        })
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn potential_calculator() {
    let mut random = LegacyRandomSource::new(1234);
    let mut calculator = PotentialCalculator::new();
    let mut lines = GOLDEN.lines().filter(|l| l.starts_with("charge ") || l.starts_with("probe "));
    for _batch in 0..6 {
        for _ in 0..5 {
            let pos = BlockPos::new(random.next_int_bounded(40) - 20, 60 + random.next_int_bounded(8), random.next_int_bounded(40) - 20);
            let charge = if random.next_int_bounded(4) == 0 { 0.0 } else { f64::from(random.next_int_bounded(100)) / 100.0 - 0.2 };
            calculator.add_charge(pos, charge);
            assert_eq!(format!("charge {} {} {} {}", pos.x(), pos.y(), pos.z(), charge.to_bits() as i64), lines.next().unwrap());
        }
        for _ in 0..6 {
            let pos = BlockPos::new(random.next_int_bounded(40) - 20, 60 + random.next_int_bounded(8), random.next_int_bounded(40) - 20);
            let charge = if random.next_int_bounded(5) == 0 { 0.0 } else { f64::from(random.next_int_bounded(100)) / 100.0 };
            let change = calculator.get_potential_energy_change(pos, charge);
            assert_eq!(format!("probe {} {} {} {} {}", pos.x(), pos.y(), pos.z(), charge.to_bits() as i64, change.to_bits() as i64), lines.next().unwrap());
        }
    }
    assert!(lines.next().is_none());
}

#[test]
fn biome_mob_spawn_settings() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let registries = Arc::new(WorldgenRegistries::new(Arc::new(FsDataSource::new(root.clone()))));
    let biomes = BiomeRegistry::load(&registries).expect("biome registry");
    let table = BiomeSpawnTable::build(&biomes);
    let base = DimensionSpawnBase::default();

    let mut names: Vec<String> = std::fs::read_dir(root.join("minecraft/worldgen/biome"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().trim_end_matches(".json").to_owned())
        .collect();
    names.sort();
    let mut actual = Vec::new();
    for name in &names {
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("minecraft/worldgen/biome/{name}.json"))).unwrap()).unwrap();
        let id = biomes.id_of_name(&format!("minecraft:{name}")).unwrap_or_else(|| panic!("biome {name}"));
        let settings = table.natural_mob_spawns(&base, Some(id));
        if Arc::ptr_eq(&settings, &EMPTY) {
            actual.push(format!("biome {name} none"));
        } else {
            actual.push(format!("biome {name} overlay"));
            for category in MobCategory::ALL {
                let Some(list) = settings.get_mobs_in_category(*category) else { continue };
                actual.push(format!("  category {} {}", category.get_name(), list.unwrap().len()));
                for entry in list.unwrap() {
                    actual.push(format!("    {} {} {} {}", entry.value.entity_type.name(), entry.value.count.min_inclusive(), entry.value.count.max_inclusive(), entry.weight));
                }
            }
            let mut costs: Vec<_> = settings.all_spawn_costs().iter().collect();
            costs.sort_by_key(|(ty, _)| ty.name());
            for (ty, cost) in costs {
                actual.push(format!("  cost {} {} {}", ty.name(), cost.energy_budget.to_bits() as i64, cost.charge.to_bits() as i64));
            }
        }
        let has_probability = json["attributes"].get("minecraft:gameplay/creature_world_gen_spawn_probability").is_some();
        let probability = table.creature_world_gen_spawn_probability(&base, Some(id));
        actual.push(format!("  creature_probability {}", if has_probability { (probability.to_bits() as i32).to_string() } else { assert_eq!(probability, 0.1f32); "default".to_owned() }));
    }
    let expected: Vec<&str> = GOLDEN.lines().skip_while(|l| !l.starts_with("biome ")).collect();
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!(a, e);
    }
}
