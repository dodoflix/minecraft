//! The `exploration_map` loot function against the real vanilla one: the chest tables that use it and the cartographer map
//! trades, rolled in a normal world with structures (fixture from `tests/oracle/ExplorationMapGolden.java`, see its header for
//! the line format). The cases share one level, in the oracle's order: the structure reference counters and map ids they use
//! carry over from case to case. Compared per case: the stacks (item, count, patch components), `map_decorations` and the
//! map's saved data (center, scale, colors digest, decorations).

use crate::common;
use crate::common::feature_harness::{WORLD, level_height, noise_world};
use crate::trades_golden::TestMerchant;

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_game::server::level::ServerLevel;
use mc_game::server::level::ServerLevelParams;
use mc_game::server::level::level_data::{PrimaryServerLevelData, default_world_data};
use mc_game::server::shared::ServerShared;
use mc_game::world::entity::Entity;
use mc_game::world::item::ItemStack;
use mc_game::world::item::trading::get_trade;
use mc_game::world::level::levelgen::hooks::VanillaWorldGenHooks;
use mc_game::world::level::storage::loot::loading::get_loot_table;
use mc_game::world::level::storage::loot::{LootContext, LootContextParamSets, LootParams};
use mc_protocol::components::{MAP_DECORATIONS, MAP_ID};
use mc_world::level_storage::WorldDataConfiguration;
use mc_world::palette::PalettedContainerFactory;
use mc_worldgen::biome::BiomeManager;
use mc_worldgen::generator::ChunkGenerator;
use sha1::{Digest, Sha1};

const SEED: i64 = 4242;

/// The overworld of the oracle's `normal` seeded world as the server builds it (`level_lifecycle::create_level`): the real
/// noise generator and the structure placement state (the structure search needs it).
fn open_level(dir: &std::path::Path) -> ServerLevel {
    let w = &*WORLD;
    let dimension = Identifier::parse("minecraft:overworld").unwrap();
    let (env, backend) = noise_world(SEED, true);
    let (shared, _rx) = ServerShared::new();
    shared.bind_game_thread();
    let structures = w.structures.clone();
    let mut level = ServerLevel::new(ServerLevelParams {
        shared,
        dimension,
        level_id: "exploration_map".to_owned(),
        dimension_dir: dir.to_path_buf(),
        level_height: level_height(),
        sea_level: w.generator.get_sea_level(),
        has_sky_light: true,
        factory: Arc::new(
            PalettedContainerFactory::create(mc_world::palette::BiomeRegistry::new(
                w.biomes.names(),
            ))
            .expect("plains registered"),
        ),
        generator: backend,
        generate_structures: true,
        hooks: Arc::new(VanillaWorldGenHooks::new(env.clone())),
        seed: SEED,
        is_known_structure: Arc::new(move |id| {
            Identifier::try_parse(id).is_some_and(|id| structures.get(&id).is_some())
        }),
        view_distance: 10,
        simulation_distance: 10,
        sync_writes: true,
        is_debug: false,
        level_data: Box::new(PrimaryServerLevelData(default_world_data())),
    });
    level.gen_env = Some(env);
    level.tick_state.biomes = Some(w.biomes.clone());
    level.tick_state.biome_zoom_seed = BiomeManager::obfuscate_seed(SEED);
    // `VanillaLevel.preload(1)`: the chunks around the origin are forced and fully generated
    for x in -1..=1 {
        for z in -1..=1 {
            level.set_chunk_forced(x, z, true);
        }
    }
    for x in -1..=1 {
        for z in -1..=1 {
            level.chunk_blocking_mut(x, z);
        }
    }
    level
}

/// The `I` / `D` / `M` lines of the oracle for a stack.
fn render(level: &ServerLevel, stack: &ItemStack, out: &mut Vec<String>) {
    let mut comps: Vec<String> = stack
        .get_components_patch()
        .entries()
        .iter()
        .map(|(t, v)| format!("{}{}", if v.is_some() { "+" } else { "-" }, t.name()))
        .collect();
    comps.sort();
    out.push(
        format!(
            "I {} {} {}",
            stack.item().name(),
            stack.count(),
            comps.join(",")
        )
        .trim_end()
        .to_owned(),
    );
    if let Some(decorations) = stack.get(&MAP_DECORATIONS) {
        let mut entries = decorations.decorations.clone();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, e) in entries {
            out.push(format!(
                "D {key} {} {:?} {:?} {:?}",
                e.ty.name(),
                e.x,
                e.z,
                e.rotation
            ));
        }
    }
    if let Some(id) = stack.get(&MAP_ID) {
        let line = level
            .read_map_data(*id, |d| {
                let digest: String = Sha1::digest(&d.colors)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let mut line = format!(
                    "M {} {} {} {} {} {digest}",
                    id.id(),
                    d.center_x,
                    d.center_z,
                    d.scale,
                    d.locked
                );
                for d in d.get_decorations() {
                    line.push_str(&format!(" {} {} {} {}", d.ty.name(), d.x, d.y, d.rot));
                }
                line
            })
            .expect("map saved data");
        out.push(line);
    }
}

#[test]
fn exploration_maps_equal_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (_config, stem) = common::load_world(dir.path(), WorldDataConfiguration::DEFAULT, true)
        .expect("default world loads");
    let mut level = open_level(&dir.path().join("overworld"));
    level.registries = stem.registries.composite_access().clone();

    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/exploration_map_golden.txt"
    ))
    .expect("fixture");
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if line.starts_with("LOOT ") || line.starts_with("TRADE ") {
            cases.push((line.to_owned(), Vec::new()));
        } else if !line.is_empty() {
            cases.last_mut().unwrap().1.push(line.trim_end().to_owned());
        }
    }
    assert!(
        cases
            .iter()
            .any(|(_, lines)| lines.iter().any(|l| l.starts_with("M ")))
    );
    let mut failures = Vec::new();
    for (header, expected) in &cases {
        let parts: Vec<&str> = header.split(' ').collect();
        let mut got = Vec::new();
        if parts[0] == "LOOT" {
            let table = get_loot_table(&level.registries, &Identifier::parse(parts[1]).unwrap());
            let origin = Vec3::at_center_of(&BlockPos::new(
                parts[2].parse().unwrap(),
                parts[3].parse().unwrap(),
                parts[4].parse().unwrap(),
            ));
            let params = LootParams::builder(&level)
                .origin(origin)
                .create(&LootContextParamSets::CHEST);
            let seed = parts[5].parse::<i64>().unwrap();
            for stack in table.get_random_items_with_seed(&params, seed) {
                render(&level, &stack, &mut got);
            }
        } else {
            let trade =
                get_trade(&level.registries, &Identifier::parse(parts[1]).unwrap()).expect("trade");
            let merchant = TestMerchant::villager(parts[2]);
            let params = LootParams::builder(&level)
                .origin(merchant.position())
                .this_entity(&merchant)
                .additional_cost_component_allowed()
                .create(&LootContextParamSets::VILLAGER_TRADE);
            let mut context = LootContext::builder(&params)
                .with_optional_random_seed(parts[3].parse().unwrap())
                .create(None);
            match trade.get_offer(&mut context) {
                Some(offer) => render(&level, offer.get_result(), &mut got),
                None => got.push("NONE".to_owned()),
            }
        }
        if &got != expected {
            failures.push(format!(
                "{header}\n  vanilla {expected:?}\n  ours    {got:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
