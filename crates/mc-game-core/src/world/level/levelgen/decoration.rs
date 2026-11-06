//! `FeatureSorter` and `ChunkGenerator.applyBiomeDecoration`.
//!
//! [`build_features_per_step`] is the generic topological sort (vanilla's `FeatureSorter.buildFeaturesPerStep`);
//! [`StepFeatureData::build`] runs it over the biome source of a [`GenEnv`] and resolves the features to placed features
//! (flat worlds' inline fill-layer features are built here). [`apply_biome_decoration`] is the decoration loop: the
//! `setDecorationSeed` / `setFeatureSeed(decorationSeed, index, step)` calls, the per-step sorted feature index set,
//! `FeaturePlacer.placeWithBiomeCheck`, `setCurrentlyGenerating` and vanilla's crash-report wrapping.
//!
//! Structures take part in the loop through `structure::generation::place_structure`.

use super::env::GenEnv;
use super::feature::fill_layer_feature::FillLayerFeature;
use super::generation_step::Decoration;
use super::holder::Holder;
use super::placement::{FeaturePlacer, PlacedFeature};
use super::structure::generation;
use super::world_gen_level::WorldGenLevel;
use crate::crash_report::{CrashReport, ReportedException};
use crate::server::level::world_gen::WorldGenRegion;
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{WorldgenRandom, XoroshiroRandomSource, generate_unique_seed};
use mc_data::block::BlockState;
use mc_world::chunk::LevelHeight;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::{BiomeRegistry, PlacedFeatureRef};
use mc_worldgen::generator::ChunkGenerator;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

/// `GenerationStep.Decoration.values().length`.
pub const DECORATION_STEP_COUNT: usize = Decoration::ALL.len();

const CYCLE_ERROR: &str = "Feature order cycle found";

// ---------------------------------------------------------------------------------------------------------------
// FeatureSorter
// ---------------------------------------------------------------------------------------------------------------

/// `Graph.depthFirstSearch`: `true` when a cycle was found.
fn depth_first_search(
    edges: &BTreeMap<(usize, i32), BTreeSet<(usize, i32)>>,
    discovered: &mut BTreeSet<(usize, i32)>,
    currently_visiting: &mut BTreeSet<(usize, i32)>,
    reverse_topological_order: &mut Vec<(usize, i32)>,
    current: (usize, i32),
) -> bool {
    if discovered.contains(&current) {
        return false;
    }
    if currently_visiting.contains(&current) {
        return true;
    }
    currently_visiting.insert(current);
    if let Some(next_nodes) = edges.get(&current) {
        for &next in next_nodes {
            if depth_first_search(
                edges,
                discovered,
                currently_visiting,
                reverse_topological_order,
                next,
            ) {
                return true;
            }
        }
    }
    currently_visiting.remove(&current);
    discovered.insert(current);
    reverse_topological_order.push(current);
    false
}

/// `FeatureSorter.buildFeaturesPerStep`: the features of every source (biome), per step, in an order that keeps every
/// source's own feature order (ties broken by the first-seen feature index, as vanilla's `TreeMap`/`TreeSet` order).
///
/// `feature_getter` returns the features of a source per `GenerationStep.Decoration`; `describe` is the source's
/// `toString` for the cycle message. Errors are vanilla's `IllegalStateException` messages.
pub fn build_features_per_step<S, K: Clone + Eq + Hash>(
    sources: &[S],
    feature_getter: &impl Fn(&S) -> Vec<Vec<K>>,
    describe: &impl Fn(&S) -> String,
    try_reducing_error: bool,
) -> Result<Vec<Vec<K>>, String> {
    let refs: Vec<&S> = sources.iter().collect();
    build_inner(&refs, feature_getter, describe, try_reducing_error)
}

fn build_inner<S, K: Clone + Eq + Hash>(
    sources: &[&S],
    feature_getter: &impl Fn(&S) -> Vec<Vec<K>>,
    describe: &impl Fn(&S) -> String,
    try_reducing_error: bool,
) -> Result<Vec<Vec<K>>, String> {
    let mut feature_index: HashMap<K, i32> = HashMap::new();
    let mut features: Vec<K> = Vec::new();
    // `TreeMap<FeatureData, Set<FeatureData>>` with the (step, featureIndex) comparator.
    let mut edges: BTreeMap<(usize, i32), BTreeSet<(usize, i32)>> = BTreeMap::new();
    let mut max_step = 0;

    for source in sources {
        let mut feature_list: Vec<(usize, i32)> = Vec::new();
        let features_for_step = feature_getter(source);
        max_step = max_step.max(features_for_step.len());
        for (step, step_features) in features_for_step.iter().enumerate() {
            for feature in step_features {
                let index = *feature_index.entry(feature.clone()).or_insert_with(|| {
                    features.push(feature.clone());
                    features.len() as i32 - 1
                });
                feature_list.push((step, index));
            }
        }
        for i in 0..feature_list.len() {
            let data = edges.entry(feature_list[i]).or_default();
            if i + 1 < feature_list.len() {
                data.insert(feature_list[i + 1]);
            }
        }
    }

    let mut discovered = BTreeSet::new();
    let mut currently_visiting = BTreeSet::new();
    let mut sorted_features: Vec<(usize, i32)> = Vec::new();
    for &feature in edges.keys() {
        if !currently_visiting.is_empty() {
            return Err("You somehow broke the universe; DFS bork (iteration finished with non-empty in-progress vertex set".to_owned());
        }
        if !discovered.contains(&feature)
            && depth_first_search(
                &edges,
                &mut discovered,
                &mut currently_visiting,
                &mut sorted_features,
                feature,
            )
        {
            if !try_reducing_error {
                return Err(CYCLE_ERROR.to_owned());
            }
            let mut reduced: Vec<&S> = sources.to_vec();
            loop {
                let last_size = reduced.len();
                let mut i = 0;
                while i < reduced.len() {
                    let source = reduced.remove(i);
                    if build_inner(&reduced, feature_getter, describe, false).is_err() {
                        continue;
                    }
                    reduced.insert(i, source);
                    i += 1;
                }
                if last_size == reduced.len() {
                    break;
                }
            }
            let involved: Vec<String> = reduced.iter().map(|s| describe(s)).collect();
            return Err(format!(
                "{CYCLE_ERROR}, involved sources: [{}]",
                involved.join(", ")
            ));
        }
    }

    sorted_features.reverse();
    Ok((0..max_step)
        .map(|step| {
            sorted_features
                .iter()
                .filter(|(s, _)| *s == step)
                .map(|&(_, index)| features[index as usize].clone())
                .collect()
        })
        .collect())
}

/// Identity of a placed feature in the biomes' feature lists (`Reference2IntMap<PlacedFeature>`: registry entries are
/// unique per id; every inline fill-layer feature of a flat world is unique per layer).
#[derive(Clone, PartialEq, Eq, Hash)]
enum FeatureKey {
    Named(Identifier),
    FillLayer(i32, BlockState),
}

impl From<FeatureKey> for PlacedFeatureRef {
    fn from(key: FeatureKey) -> Self {
        match key {
            FeatureKey::Named(id) => PlacedFeatureRef::Named(id),
            FeatureKey::FillLayer(height, state) => {
                PlacedFeatureRef::InlineFillLayer { height, state }
            }
        }
    }
}

impl From<&PlacedFeatureRef> for FeatureKey {
    fn from(feature: &PlacedFeatureRef) -> Self {
        match feature {
            PlacedFeatureRef::Named(id) => FeatureKey::Named(id.clone()),
            PlacedFeatureRef::InlineFillLayer { height, state } => {
                FeatureKey::FillLayer(*height, *state)
            }
        }
    }
}

/// A feature of a biome's feature list resolved to its placed feature.
pub struct StepFeature {
    /// How the biomes list it (`PlacementContext.topFeature` identity for `BiomeFilter`).
    pub reference: PlacedFeatureRef,
    pub feature: Arc<PlacedFeature>,
}

impl StepFeature {
    /// `featureRegistry.getResourceKey(feature).map(Object::toString).orElseGet(feature::toString)`.
    fn description(&self) -> String {
        match &self.reference {
            PlacedFeatureRef::Named(id) => {
                format!("ResourceKey[minecraft:worldgen/placed_feature / {id}]")
            }
            PlacedFeatureRef::InlineFillLayer { .. } => {
                "Placed Direct{FillLayerFeature}".to_owned()
            }
        }
    }
}

/// `FeatureSorter.StepFeatureData`.
pub struct StepFeatureData {
    pub features: Vec<StepFeature>,
    /// `indexMapping`.
    index_mapping: HashMap<FeatureKey, usize>,
}

impl StepFeatureData {
    /// `StepFeatureData(features)`: the identity index lookup over the step's features.
    pub fn new(features: Vec<StepFeature>) -> StepFeatureData {
        let index_mapping = features
            .iter()
            .enumerate()
            .map(|(index, f)| (FeatureKey::from(&f.reference), index))
            .collect();
        StepFeatureData {
            features,
            index_mapping,
        }
    }

    /// `indexMapping.applyAsInt(feature)`: `None` is vanilla's `-1`.
    pub fn index_of(&self, feature: &PlacedFeatureRef) -> Option<usize> {
        self.index_mapping.get(&FeatureKey::from(feature)).copied()
    }

    /// `ChunkGenerator.featuresPerStep`: [`sort_features_per_step`] with the features resolved against the registries
    /// (the inline fill-layer features of flat worlds are built here).
    pub fn build(env: &GenEnv) -> Result<Vec<StepFeatureData>, String> {
        let mut fill_layers: HashMap<FeatureKey, Arc<PlacedFeature>> = HashMap::new();
        sort_features_per_step(env.generator.as_ref(), &env.biomes)?
            .into_iter()
            .map(|step| {
                step.into_iter()
                    .map(|reference| {
                        let feature = match &reference {
                            PlacedFeatureRef::Named(id) => env
                                .registries
                                .placed_feature(id)
                                .ok_or_else(|| format!("Unknown placed feature {id}"))?
                                .value()
                                .clone(),
                            // `PlacementUtils.inlinePlaced(new FillLayerFeature(height, state))`: no placement modifiers
                            PlacedFeatureRef::InlineFillLayer { height, state } => fill_layers
                                .entry(FeatureKey::from(&reference))
                                .or_insert_with(|| {
                                    Arc::new(PlacedFeature {
                                        feature: Holder::direct(Arc::new(FillLayerFeature {
                                            height: *height,
                                            state: *state,
                                        })),
                                        placement: Vec::new(),
                                    })
                                })
                                .clone(),
                        };
                        Ok(StepFeature { reference, feature })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map(StepFeatureData::new)
            })
            .collect()
    }
}

/// `FeatureSorter.buildFeaturesPerStep(List.copyOf(biomeSource.possibleBiomes()), biome -> generator
/// .getBiomeGenerationSettings(biome).features(), true)`: the sorted features of every step.
pub fn sort_features_per_step(
    generator: &dyn ChunkGenerator,
    biomes: &BiomeRegistry,
) -> Result<Vec<Vec<PlacedFeatureRef>>, String> {
    let sorted = build_features_per_step(
        generator.biome_source().possible_biomes(),
        &|biome: &BiomeId| {
            generator
                .get_biome_generation_settings(*biome)
                .features()
                .iter()
                .map(|step| step.iter().map(FeatureKey::from).collect())
                .collect()
        },
        &|biome: &BiomeId| {
            format!(
                "Reference{{ResourceKey[minecraft:worldgen/biome / {}]=Biome}}",
                biomes.key(*biome)
            )
        },
        true,
    )?;
    Ok(sorted
        .into_iter()
        .map(|step| step.into_iter().map(PlacedFeatureRef::from).collect())
        .collect())
}

// ---------------------------------------------------------------------------------------------------------------
// applyBiomeDecoration
// ---------------------------------------------------------------------------------------------------------------

// The structure part of the loop: for every step `apply_biome_decoration` walks
// `env.structures.structures_for_step(step)` (`structuresByStep`, registry order) with a counter `index` starting at 0,
// calls `random.set_feature_seed(decorationSeed, index, step)` before each structure, runs
// `structure::generation::place_structure` (`startsForStructure(..).forEach(start -> start.placeInChunk(..))`) and
// increments `index` after it (also if the structure has no start here); the feature indices of the same step are
// seeded independently afterwards. Only with `GenEnv.generate_structures` (`structureManager.shouldGenerateStructures()`).
/// What `applyBiomeDecoration` reads from the chunk neighbourhood besides the writable level.
pub struct DecorationInput {
    /// `chunk.getPos()`.
    pub center: ChunkPos,
    /// `chunk.getHeightAccessorForGeneration()` of the center chunk.
    pub generation_height: LevelHeight,
    /// Every biome in the palettes of the sections of the 3x3 chunks around `center` (`possibleBiomes` before the
    /// `retainAll`; order is irrelevant).
    pub biomes_in_range: Vec<BiomeId>,
}

impl DecorationInput {
    /// Reads the center chunk and its 8 neighbours from the region.
    pub fn from_region(region: &WorldGenRegion) -> DecorationInput {
        let center = region.center;
        let center_chunk = region
            .get_chunk(center.x, center.z)
            .expect("the center chunk is part of the region");
        let mut biomes_in_range = Vec::new();
        let mut seen = HashSet::new();
        for chunk_pos in ChunkPos::range_closed(&center, 1) {
            let chunk = region
                .get_chunk(chunk_pos.x, chunk_pos.z)
                .expect("the decoration region holds the 3x3 chunks");
            for section in &chunk.access.sections {
                section.biomes().get_all(|biome| {
                    if seen.insert(biome) {
                        biomes_in_range.push(biome);
                    }
                });
            }
        }
        DecorationInput {
            center,
            generation_height: center_chunk.get_height_accessor_for_generation(),
            biomes_in_range,
        }
    }
}

/// `ChunkGenerator.getWritableArea(chunk)`.
fn get_writable_area(center: ChunkPos, height: LevelHeight) -> BoundingBox {
    let target_block_x = center.get_min_block_x();
    let target_block_z = center.get_min_block_z();
    BoundingBox::new(
        target_block_x,
        height.min_y + 1,
        target_block_z,
        target_block_x + 15,
        height.max_y(),
        target_block_z + 15,
    )
}

/// One `random.setFeatureSeed(..)` + placement of the loop, reported to a test observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecorationEvent {
    pub step: i32,
    /// The structure counter or the feature's index in the step's sorted list.
    pub index: i32,
    pub structure: bool,
    pub name: String,
    pub decoration_seed: i64,
}

/// `ChunkGenerator.applyBiomeDecoration(level, chunk, structureManager)`; `level` is the region around
/// `input.center`, `features` the memoized [`StepFeatureData::build`] of `env`.
///
/// Panics inside a structure/feature are re-raised as vanilla's crash reports ([`ReportedException`]).
pub fn apply_biome_decoration(
    level: &mut dyn WorldGenLevel,
    env: &GenEnv,
    features: &[StepFeatureData],
    input: &DecorationInput,
) {
    apply_biome_decoration_observed(level, env, features, input, &mut |_| {});
}

/// [`apply_biome_decoration`] reporting every seeded step to `observer` (golden tests compare the sequence).
pub fn apply_biome_decoration_observed(
    level: &mut dyn WorldGenLevel,
    env: &GenEnv,
    features: &[StepFeatureData],
    input: &DecorationInput,
    observer: &mut dyn FnMut(&DecorationEvent),
) {
    let center_pos = input.center;
    // `SharedConstants.debugVoidTerrain(centerPos)` is a debug flag that is never set.
    let min_section_y = level.level_height().min_section_y();
    let origin = BlockPos::new(center_pos.x << 4, min_section_y << 4, center_pos.z << 4);
    let mut random =
        WorldgenRandom::new(Box::new(XoroshiroRandomSource::new(generate_unique_seed())));
    let decoration_seed = random.set_decoration_seed(level.get_seed(), origin.x(), origin.z());
    let biome_source_biomes: HashSet<BiomeId> = env
        .biome_source()
        .possible_biomes()
        .iter()
        .copied()
        .collect();
    let possible_biomes: Vec<BiomeId> = input
        .biomes_in_range
        .iter()
        .copied()
        .filter(|b| biome_source_biomes.contains(b))
        .collect();
    let feature_step_count = features.len();
    let writable_area = get_writable_area(center_pos, input.generation_height);
    let mut placer = FeaturePlacer::new();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let generation_steps = DECORATION_STEP_COUNT.max(feature_step_count);
        for step_index in 0..generation_steps {
            let mut index = 0;
            if env.generate_structures {
                for holder in env.structures.structures_for_step(step_index) {
                    let structure = holder
                        .id()
                        .expect("structures of the registry are references");
                    random.set_feature_seed(decoration_seed, index, step_index as i32);
                    let name = format!("ResourceKey[minecraft:worldgen/structure / {structure}]");
                    observer(&DecorationEvent {
                        step: step_index as i32,
                        index,
                        structure: true,
                        name: name.clone(),
                        decoration_seed,
                    });
                    let currently_generating = name.clone();
                    level.set_currently_generating(Some(Box::new(move || {
                        currently_generating.clone()
                    })));
                    let placed = catch_unwind(AssertUnwindSafe(|| {
                        generation::place_structure(
                            level,
                            env,
                            holder,
                            &mut random,
                            &writable_area,
                            center_pos,
                        );
                    }));
                    if let Err(payload) = placed {
                        feature_placement_failure(payload, name);
                    }
                    index += 1;
                }
            }

            if step_index < feature_step_count {
                let step_feature_data = &features[step_index];
                let mut possible_features_this_step: BTreeSet<usize> = BTreeSet::new();
                for &biome in &possible_biomes {
                    let features_in_biome = env.get_biome_generation_settings(biome).features();
                    if let Some(features_in_biome_this_step) = features_in_biome.get(step_index) {
                        for feature in features_in_biome_this_step {
                            possible_features_this_step.insert(step_feature_data.index_of(feature).unwrap_or_else(|| panic!("feature {feature:?} is not part of step {step_index} of the sorted features")));
                        }
                    }
                }

                // `IntArraySet` + `Arrays.sort`: ascending global indices
                for global_index in possible_features_this_step {
                    let feature = &step_feature_data.features[global_index];
                    let name = feature.description();
                    random.set_feature_seed(
                        decoration_seed,
                        global_index as i32,
                        step_index as i32,
                    );
                    observer(&DecorationEvent {
                        step: step_index as i32,
                        index: global_index as i32,
                        structure: false,
                        name: name.clone(),
                        decoration_seed,
                    });
                    let currently_generating = name.clone();
                    level.set_currently_generating(Some(Box::new(move || {
                        currently_generating.clone()
                    })));
                    let placed = catch_unwind(AssertUnwindSafe(|| {
                        placer.place_with_biome_check(
                            &feature.feature,
                            &feature.reference,
                            level,
                            env,
                            &mut random,
                            origin,
                        );
                    }));
                    if let Err(payload) = placed {
                        feature_placement_failure(payload, name);
                    }
                }
            }
        }
        level.set_currently_generating(None);
    }));

    if let Err(payload) = result {
        let mut report = CrashReport::for_throwable(payload, "Biome decoration");
        report
            .add_category("Generation")
            .set_detail("CenterX", center_pos.x)
            .set_detail("CenterZ", center_pos.z)
            .set_detail("Decoration Seed", decoration_seed);
        ReportedException::throw(report);
    }
}

/// `CrashReport.forThrowable(e, "Feature placement")` with the `Feature` category, thrown as a `ReportedException`.
fn feature_placement_failure(payload: Box<dyn std::any::Any + Send>, description: String) -> ! {
    let mut report = CrashReport::for_throwable(payload, "Feature placement");
    report
        .add_category("Feature")
        .set_detail("Description", description);
    ReportedException::throw(report);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sort(
        sources: &[Vec<Vec<&'static str>>],
        reduce: bool,
    ) -> Result<Vec<Vec<&'static str>>, String> {
        build_features_per_step(
            sources,
            &|s: &Vec<Vec<&'static str>>| s.clone(),
            &|s| format!("{s:?}"),
            reduce,
        )
    }

    #[test]
    fn merges_orders_per_step() {
        // a biome lists b before a in step 0; another lists a, c: the shared order must be consistent
        let sorted = sort(
            &[
                vec![vec!["b", "a"], vec!["x"]],
                vec![vec!["a", "c"], vec!["y", "x"]],
            ],
            true,
        )
        .unwrap();
        assert_eq!(sorted, vec![vec!["b", "a", "c"], vec!["y", "x"]]);
    }

    #[test]
    fn same_feature_in_two_steps_is_two_nodes() {
        let sorted = sort(&[vec![vec!["a"], vec!["a"]]], true).unwrap();
        assert_eq!(sorted, vec![vec!["a"], vec!["a"]]);
    }

    #[test]
    fn cycle_without_reduction() {
        let err = sort(&[vec![vec!["a", "b"]], vec![vec!["b", "a"]]], false).unwrap_err();
        assert_eq!(err, "Feature order cycle found");
    }

    #[test]
    fn cycle_lists_only_involved_sources() {
        let err = sort(
            &[
                vec![vec!["a", "b"]],
                vec![vec!["c"]],
                vec![vec!["b", "a"]],
                vec![vec!["d", "e"]],
            ],
            true,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "Feature order cycle found, involved sources: [[[\"a\", \"b\"]], [[\"b\", \"a\"]]]"
        );
    }

    #[test]
    fn repeated_feature_is_a_cycle() {
        assert!(sort(&[vec![vec!["a", "a"]]], false).is_err());
    }

    #[test]
    fn writable_area_skips_the_bottom_layer() {
        let area = get_writable_area(ChunkPos::new(-2, 3), LevelHeight::new(-64, 384));
        assert_eq!(
            (
                area.min_x(),
                area.min_y(),
                area.min_z(),
                area.max_x(),
                area.max_y(),
                area.max_z()
            ),
            (-32, -63, 48, -17, 319, 63)
        );
    }
}
