//! The structure parts of `ChunkGenerator`: `createStructures` (+ `tryGenerateStructure`, `Structure.generate`),
//! `createReferences`, the decoration step entry (`place_structure`), `getMobsAt`'s spawn override lookup.

use super::manager::{LevelSource, StructureDataSource, StructureManager};
use super::piece::StructurePieceSerializationContext;
use super::structure::{GenerationContext, Structure, StructureSpawnOverride, BoundingBoxType, SpawnerData};
use super::structure_start::{StartHandle, StructureStart};
use crate::server::level::world_gen::WorldGenRegion;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{LegacyRandomSource, RandomSource, WorldgenRandom};
use mc_data::enums::MobCategory;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_worldgen::provider::WeightedList;
use mc_worldgen::structure_placement::{ChunkGeneratorStructureState, StructureSelectionEntry};
use mc_world::palette::BiomeId;

impl StructureDataSource for WorldGenRegion {
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        WorldGenRegion::structure_data(self, pos)
    }
}

/// `Structure.generate(selected, dimension, registryAccess, chunkGenerator, biomeSource, climateSampler, randomState,
/// structureTemplateManager, seed, sourceChunkPos, references, heightAccessor, validBiome)`: the start (invalid when the
/// structure finds no valid generation point).
pub fn generate_structure(
    env: &GenEnv,
    selected: &Holder<dyn Structure>,
    seed: i64,
    source_chunk_pos: ChunkPos,
    references: i32,
    height_accessor: mc_world::chunk::LevelHeight,
    valid_biome: &dyn Fn(BiomeId) -> bool,
) -> StructureStart {
    let structure = selected.value();
    let mut context = GenerationContext::new(env, seed, source_chunk_pos, height_accessor, valid_biome);
    if let Some(stub) = structure.find_valid_generation_point(&mut context) {
        let builder = stub.get_pieces_builder(&mut context);
        let test_start = StructureStart::new(selected.clone(), source_chunk_pos, references, builder.build());
        if test_start.is_valid() {
            return test_start;
        }
    }
    StructureStart::invalid()
}

/// `fetchReferences(structureManager, centerChunk, structure)`.
fn fetch_references(manager: &StructureManager<'_>, chunk: &ProtoChunk, structure: &Holder<dyn Structure>) -> i32 {
    manager
        .get_start_for_structure(chunk.access.pos, &chunk.access.structures, structure)
        .map_or(0, |start| start.lock().get_references())
}

/// `tryGenerateStructure(selected, ...)`.
fn try_generate_structure(
    env: &GenEnv,
    manager: &StructureManager<'_>,
    state: &ChunkGeneratorStructureState,
    selected: &StructureSelectionEntry,
    center_chunk: &mut ProtoChunk,
) -> bool {
    let Some(holder) = env.structures.get(&selected.structure) else { return false };
    let structure = holder.value();
    let source_chunk_pos = center_chunk.access.pos;
    let references = fetch_references(manager, center_chunk, holder);
    let biome_allowed = structure.biomes();
    let height_accessor = center_chunk.get_height_accessor_for_generation();
    let mut start = generate_structure(env, holder, state.level_seed(), source_chunk_pos, references, height_accessor, &|b| biome_allowed.contains(&b));
    if !start.is_valid() {
        return false;
    }
    let context = StructurePieceSerializationContext { env };
    let tag = start.create_tag(&context, source_chunk_pos);
    let bb = start.get_bounding_box();
    let id = selected.structure.to_string();
    let handle = StartHandle::new(start);
    env.start_cache.put(source_chunk_pos, &id, tag.clone(), handle);
    center_chunk.set_start_for_structure(id, tag, Some((bb.min_y(), bb.max_y())));
    true
}

/// `ChunkGenerator.createStructures(registryAccess, state, structureManager, centerChunk, structureTemplateManager, level)`.
pub fn create_structures(env: &GenEnv, center_chunk: &mut ProtoChunk) {
    let Some(state) = env.structure_state.as_deref() else { return };
    let manager = StructureManager::new(env);
    let source_chunk_pos = center_chunk.access.pos;
    for set in state.possible_structure_sets() {
        let feature_placement = &set.placement;
        let structures = &set.structures;
        let mut has_existing = false;
        for entry in structures {
            if let Some(holder) = env.structures.get(&entry.structure)
                && let Some(existing) = manager.get_start_for_structure(source_chunk_pos, &center_chunk.access.structures, holder)
                && existing.lock().is_valid()
            {
                has_existing = true;
                break;
            }
        }
        if has_existing {
            continue;
        }
        if !feature_placement.is_structure_chunk(state, source_chunk_pos.x, source_chunk_pos.z) {
            continue;
        }
        if structures.len() == 1 {
            try_generate_structure(env, &manager, state, &structures[0], center_chunk);
        } else {
            let mut options: Vec<&StructureSelectionEntry> = structures.iter().collect();
            let mut random = WorldgenRandom::new(Box::new(LegacyRandomSource::new(0)));
            random.set_large_feature_seed(state.level_seed(), source_chunk_pos.x, source_chunk_pos.z);
            let mut total: i32 = options.iter().map(|o| o.weight).fold(0i32, i32::wrapping_add);
            while !options.is_empty() {
                let mut choice = random.next_int_bounded(total);
                let mut index = 0;
                for option in &options {
                    choice -= option.weight;
                    if choice < 0 {
                        break;
                    }
                    index += 1;
                }
                let selected = options[index];
                if try_generate_structure(env, &manager, state, selected, center_chunk) {
                    break;
                }
                options.remove(index);
                total -= selected.weight;
            }
        }
    }
}

/// `ChunkGenerator.createReferences(level, structureManager, centerChunk)`: every start within 8 chunks that reaches
/// the centre chunk is referenced from it. The region must hold the structure data of the 17x17 chunks around.
pub fn create_references(env: &GenEnv, region: &mut WorldGenRegion) {
    let manager = StructureManager::new(env);
    let center = region.get_center();
    let (target_x, target_z) = (center.x, center.z);
    let target_block_x = center.get_min_block_x();
    let target_block_z = center.get_min_block_z();
    let mut to_add: Vec<(String, i64)> = Vec::new();
    for source_x in target_x - 8..=target_x + 8 {
        for source_z in target_z - 8..=target_z + 8 {
            let source_chunk_pos = ChunkPos::new(source_x, source_z);
            let source_chunk_key = source_chunk_pos.pack();
            let Some(data) = region.structure_data(source_chunk_pos) else { continue };
            for key in data.starts.keys() {
                let Some(id) = mc_core::ident::Identifier::try_parse(key) else { continue };
                let Some(structure) = env.structures.get(&id) else { continue };
                let Some(start) = manager.get_start_for_structure(source_chunk_pos, data, structure) else { continue };
                let mut start = start.lock();
                if start.is_valid() && start.get_bounding_box().intersects_xz(target_block_x, target_block_z, target_block_x + 15, target_block_z + 15) {
                    to_add.push((key.clone(), source_chunk_key));
                }
            }
        }
    }
    let chunk = region.get_chunk_mut_unchecked(center);
    for (structure, key) in to_add {
        chunk.access.add_reference_for_structure(structure, key);
    }
}

/// The inner part of `applyBiomeDecoration`'s structure loop for one structure:
/// `structureManager.startsForStructure(sectionX, sectionZ, structure).forEach(start -> start.placeInChunk(..))`.
pub fn place_structure(
    level: &mut dyn WorldGenLevel,
    env: &GenEnv,
    structure: &Holder<dyn Structure>,
    random: &mut dyn RandomSource,
    writable_area: &BoundingBox,
    center: ChunkPos,
) {
    let manager = StructureManager::new(env);
    let starts = manager.starts_for_structure(&LevelSource(&*level), center.x, center.z, structure);
    for start in starts {
        start.lock().place_in_chunk(level, &manager, env, random, writable_area, center);
    }
}

/// The structure part of `ChunkGenerator.getMobsAt(level, structureManager, category, pos)`: the spawn list of the first
/// structure override that contains `pos`, `None` when the caller has to use the natural spawns of the biome.
pub fn get_structure_spawn_override(env: &GenEnv, source: &dyn StructureDataSource, category: MobCategory, pos: BlockPos) -> Option<WeightedList<SpawnerData>> {
    let manager = StructureManager::new(env);
    let structures = manager.get_all_structures_at(source, pos)?;
    for (key, references) in structures {
        let Some(id) = mc_core::ident::Identifier::try_parse(key) else { continue };
        let Some(holder) = env.structures.get(&id) else { continue };
        let Some(StructureSpawnOverride { bounding_box, spawns }) = holder.value().settings().spawn_override(category) else { continue };
        let mut in_override_box = false;
        manager.fill_starts_for_structure(source, holder, references, &mut |start| {
            if !in_override_box {
                let mut start = start.lock();
                in_override_box = match bounding_box {
                    BoundingBoxType::Piece => manager.structure_has_piece_at(pos, &start),
                    BoundingBoxType::Structure => start.get_bounding_box().is_inside(&pos),
                };
            }
        });
        if in_override_box {
            return Some(spawns.clone());
        }
    }
    None
}

/// Expands a flat generator's `structure_overrides` (`HolderSet<StructureSet>`) to registry ids; a tag is resolved
/// through `tags/worldgen/structure_set` (nested `#tag` entries followed, optional entries that do not exist skipped).
fn expand_structure_set_holder_set(set: &mc_worldgen::flat::settings::StructureSetHolderSet, worldgen: &mc_worldgen::data::WorldgenRegistries) -> Vec<mc_core::ident::Identifier> {
    use mc_worldgen::flat::settings::StructureSetHolderSet;
    fn expand_tag(id: &mc_core::ident::Identifier, worldgen: &mc_worldgen::data::WorldgenRegistries, out: &mut Vec<mc_core::ident::Identifier>, depth: u32) {
        if depth > 16 {
            return;
        }
        let Some(json) = worldgen.raw("tags/worldgen/structure_set", id) else { return };
        for entry in json.get("values").and_then(|v| v.as_array()).into_iter().flatten() {
            let text = entry.as_str().or_else(|| entry.get("id").and_then(|v| v.as_str()));
            let Some(text) = text else { continue };
            if let Some(tag) = text.strip_prefix('#') {
                if let Ok(tag) = mc_core::ident::Identifier::read(tag) {
                    expand_tag(&tag, worldgen, out, depth + 1);
                }
            } else if let Ok(element) = mc_core::ident::Identifier::read(text)
                && !out.contains(&element)
            {
                out.push(element);
            }
        }
    }
    match set {
        StructureSetHolderSet::Direct(ids) => ids.clone(),
        StructureSetHolderSet::Tag(tag) => {
            let mut out = Vec::new();
            expand_tag(tag, worldgen, &mut out, 0);
            out
        }
    }
}

/// `ChunkGenerator.createState(structureSets, randomState, levelSeed)` / `FlatLevelSource.createState(..)`.
/// `flat`: `None` for a normal (noise/debug) generator, `Some(overrides)` for a flat generator with its
/// `structure_overrides` (`None` there = all sets).
pub fn create_structure_state(
    generator: &dyn mc_worldgen::generator::ChunkGenerator,
    biome_source: std::sync::Arc<dyn mc_worldgen::biome::BiomeSource>,
    flat: Option<Option<&mc_worldgen::flat::settings::StructureSetHolderSet>>,
    random_state: std::sync::Arc<mc_worldgen::random_state::RandomState>,
    seed: i64,
    worldgen: &mc_worldgen::data::WorldgenRegistries,
    biomes: &mc_worldgen::biome::BiomeRegistry,
) -> crate::world::level::levelgen::codec::LoadResult<std::sync::Arc<ChunkGeneratorStructureState>> {
    use mc_worldgen::structure_placement::StructureSetRegistry;
    let registry = std::sync::Arc::new(StructureSetRegistry::load(worldgen, biomes)?);
    let origin = generator.get_origin(&random_state);
    let structure_biomes = |id: &mc_core::ident::Identifier| registry.structure_biomes(id).to_vec();
    let state = match flat {
        None => ChunkGeneratorStructureState::create_for_normal(random_state, seed, origin, biome_source, registry.clone(), &structure_biomes),
        Some(overrides) => {
            let holders: Vec<_> = match overrides {
                Some(set) => expand_structure_set_holder_set(set, worldgen).iter().filter_map(|id| registry.get(id).cloned()).collect(),
                None => registry.iter().cloned().collect(),
            };
            ChunkGeneratorStructureState::create_for_flat(random_state, seed, origin, biome_source, registry.clone(), holders, &structure_biomes)
        }
    };
    Ok(std::sync::Arc::new(state))
}
