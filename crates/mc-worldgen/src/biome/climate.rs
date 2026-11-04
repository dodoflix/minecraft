//! `net.minecraft.world.level.biome.Climate`: quantized climate parameters, the parameter-point R-tree
//! and the climate [`Sampler`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::data::LoadError;
use crate::densityfunction::SharedSampler;
use crate::densityfunction::codec::{Fields, LoadResult, parse_f32, parse_list};
use crate::random_state::{BoundSampler, DensitySamplerSet};
use mc_core::pos::quart_pos;

const QUANTIZATION_FACTOR: f32 = 10000.0;
/// `Climate.PARAMETER_COUNT`.
pub const PARAMETER_COUNT: usize = 7;
/// `ParameterList.ParameterList(values)`: the children-per-node of the default R-tree.
pub const CHILDREN_PER_NODE: usize = 19;

/// `Climate.target(...)`.
pub fn target(
    temperature: f32,
    humidity: f32,
    continentalness: f32,
    erosion: f32,
    depth: f32,
    weirdness: f32,
) -> TargetPoint {
    TargetPoint {
        temperature: quantize_coord(temperature),
        humidity: quantize_coord(humidity),
        continentalness: quantize_coord(continentalness),
        erosion: quantize_coord(erosion),
        depth: quantize_coord(depth),
        weirdness: quantize_coord(weirdness),
    }
}

/// `Climate.parameters(float x7)`.
pub fn parameters_point(
    temperature: f32,
    humidity: f32,
    continentalness: f32,
    erosion: f32,
    depth: f32,
    weirdness: f32,
    offset: f32,
) -> ParameterPoint {
    ParameterPoint {
        temperature: Parameter::point(temperature),
        humidity: Parameter::point(humidity),
        continentalness: Parameter::point(continentalness),
        erosion: Parameter::point(erosion),
        depth: Parameter::point(depth),
        weirdness: Parameter::point(weirdness),
        offset: quantize_coord(offset),
    }
}

/// `Climate.parameters(Parameter x6, float offset)`.
pub fn parameters(
    temperature: Parameter,
    humidity: Parameter,
    continentalness: Parameter,
    erosion: Parameter,
    depth: Parameter,
    weirdness: Parameter,
    offset: f32,
) -> ParameterPoint {
    ParameterPoint {
        temperature,
        humidity,
        continentalness,
        erosion,
        depth,
        weirdness,
        offset: quantize_coord(offset),
    }
}

/// `Climate.quantizeCoord`.
pub fn quantize_coord(coord: f32) -> i64 {
    (coord * QUANTIZATION_FACTOR) as i64
}

/// `Climate.unquantizeCoord`.
pub fn unquantize_coord(coord: i64) -> f32 {
    coord as f32 / QUANTIZATION_FACTOR
}

/// `Climate.Parameter`: a closed interval of quantized coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Parameter {
    pub min: i64,
    pub max: i64,
}

impl Parameter {
    pub const fn new(min: i64, max: i64) -> Parameter {
        Parameter { min, max }
    }

    /// `Parameter.point(min)`.
    pub fn point(min: f32) -> Parameter {
        Self::span(min, min)
    }

    /// `Parameter.span(float, float)`. Panics if `min > max` (vanilla: `IllegalArgumentException`).
    pub fn span(min: f32, max: f32) -> Parameter {
        assert!(min <= max, "min > max: {min} {max}");
        Parameter {
            min: quantize_coord(min),
            max: quantize_coord(max),
        }
    }

    /// `Parameter.span(Parameter, Parameter)`. Panics if `min.min > max.max`.
    pub fn span_of(min: Parameter, max: Parameter) -> Parameter {
        assert!(min.min <= max.max, "min > max: {min:?} {max:?}");
        Parameter {
            min: min.min,
            max: max.max,
        }
    }

    /// `Parameter.distance(long)`.
    pub fn distance(&self, target: i64) -> i64 {
        let above = target.wrapping_sub(self.max);
        let below = self.min.wrapping_sub(target);
        if above > 0 { above } else { below.max(0) }
    }

    /// `Parameter.span(@Nullable Parameter other)`.
    pub fn span_with(&self, other: Option<Parameter>) -> Parameter {
        match other {
            None => *self,
            Some(other) => Parameter {
                min: self.min.min(other.min),
                max: self.max.max(other.max),
            },
        }
    }

    /// `Parameter.CODEC` (`ExtraCodecs.intervalCodec(Codec.floatRange(-2, 2), "min", "max", ...)`): a number,
    /// a `[min, max]` list or a `{min, max}` object.
    pub fn from_json(value: &Value) -> LoadResult<Parameter> {
        fn point(value: &Value) -> LoadResult<f32> {
            let v = parse_f32(value)?;
            if (-2.0..=2.0).contains(&v) {
                Ok(v)
            } else {
                Err(LoadError::new(format!(
                    "Value {v} outside of range [-2.0:2.0]"
                )))
            }
        }
        fn make(min: f32, max: f32) -> LoadResult<Parameter> {
            if min > max {
                Err(LoadError::new(format!(
                    "Cannon construct interval, min > max ({min} > {max})"
                )))
            } else {
                Ok(Parameter {
                    min: quantize_coord(min),
                    max: quantize_coord(max),
                })
            }
        }
        match value {
            Value::Number(_) => {
                let v = point(value)?;
                make(v, v)
            }
            Value::Array(list) => {
                if list.len() != 2 {
                    return Err(LoadError::new(format!(
                        "Input is not a list of 2 elements: {} elements",
                        list.len()
                    )));
                }
                let items = parse_list(value, point)?;
                make(items[0], items[1])
            }
            Value::Object(_) => {
                let fields = Fields::new(value)?;
                let min = fields.required("min", point)?;
                let max = fields.required("max", point)?;
                make(min, max)
            }
            _ => Err(LoadError::new("expected number, list or object")),
        }
    }
}

/// `Climate.ParameterPoint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParameterPoint {
    pub temperature: Parameter,
    pub humidity: Parameter,
    pub continentalness: Parameter,
    pub erosion: Parameter,
    pub depth: Parameter,
    pub weirdness: Parameter,
    pub offset: i64,
}

impl ParameterPoint {
    /// `ParameterPoint.CODEC`.
    pub fn from_json(value: &Value) -> LoadResult<ParameterPoint> {
        let fields = Fields::new(value)?;
        Ok(ParameterPoint {
            temperature: fields.required("temperature", Parameter::from_json)?,
            humidity: fields.required("humidity", Parameter::from_json)?,
            continentalness: fields.required("continentalness", Parameter::from_json)?,
            erosion: fields.required("erosion", Parameter::from_json)?,
            depth: fields.required("depth", Parameter::from_json)?,
            weirdness: fields.required("weirdness", Parameter::from_json)?,
            offset: fields.required("offset", |v| {
                let offset = parse_f32(v)?;
                if (0.0..=1.0).contains(&offset) {
                    Ok(quantize_coord(offset))
                } else {
                    Err(LoadError::new(format!(
                        "Value {offset} outside of range [0.0:1.0]"
                    )))
                }
            })?,
        })
    }

    /// `ParameterPoint.fitness`.
    pub fn fitness(&self, target: &TargetPoint) -> i64 {
        square(self.temperature.distance(target.temperature))
            .wrapping_add(square(self.humidity.distance(target.humidity)))
            .wrapping_add(square(self.continentalness.distance(target.continentalness)))
            .wrapping_add(square(self.erosion.distance(target.erosion)))
            .wrapping_add(square(self.depth.distance(target.depth)))
            .wrapping_add(square(self.weirdness.distance(target.weirdness)))
            .wrapping_add(square(self.offset))
    }

    /// `ParameterPoint.parameterSpace()`.
    pub fn parameter_space(&self) -> [Parameter; PARAMETER_COUNT] {
        [
            self.temperature,
            self.humidity,
            self.continentalness,
            self.erosion,
            self.depth,
            self.weirdness,
            Parameter::new(self.offset, self.offset),
        ]
    }
}

fn square(v: i64) -> i64 {
    v.wrapping_mul(v)
}

/// `Climate.TargetPoint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetPoint {
    pub temperature: i64,
    pub humidity: i64,
    pub continentalness: i64,
    pub erosion: i64,
    pub depth: i64,
    pub weirdness: i64,
}

impl TargetPoint {
    /// `TargetPoint.toParameterArray()`.
    pub fn to_parameter_array(&self) -> [i64; PARAMETER_COUNT] {
        [
            self.temperature,
            self.humidity,
            self.continentalness,
            self.erosion,
            self.depth,
            self.weirdness,
            0,
        ]
    }
}

// ---------------------------------------------------------------------------------------------
// RTree
// ---------------------------------------------------------------------------------------------

type Space = [Parameter; PARAMETER_COUNT];

enum NodeKind {
    /// Index into the owning [`ParameterList`]'s values.
    Leaf(usize),
    SubTree(Vec<u32>),
}

struct Node {
    space: Space,
    kind: NodeKind,
}

fn node_distance(space: &Space, target: &[i64; PARAMETER_COUNT]) -> i64 {
    let mut distance = 0i64;
    for i in 0..PARAMETER_COUNT {
        distance = distance.wrapping_add(square(space[i].distance(target[i])));
    }
    distance
}

static NEXT_TREE_ID: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// `RTree.lastResult` (a `ThreadLocal` per tree): tree id -> last found leaf node.
    static LAST_RESULT: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
}

/// `Climate.RTree`: the index over a parameter list, built exactly like vanilla (the build order decides
/// which of several equally good leaves a search returns).
pub struct RTree {
    nodes: Vec<Node>,
    root: u32,
    id: u64,
}

impl RTree {
    /// `RTree.create(values, childrenPerNode)`. Panics on an empty list (vanilla:
    /// `IllegalArgumentException`).
    pub fn create(points: &[ParameterPoint], children_per_node: usize) -> RTree {
        assert!(
            !points.is_empty(),
            "Need at least one value to build the search tree."
        );
        let mut nodes: Vec<Node> = points
            .iter()
            .enumerate()
            .map(|(i, p)| Node {
                space: p.parameter_space(),
                kind: NodeKind::Leaf(i),
            })
            .collect();
        let leaves: Vec<u32> = (0..nodes.len() as u32).collect();
        let root = Self::build(&mut nodes, leaves, children_per_node);
        RTree {
            nodes,
            root,
            id: NEXT_TREE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    fn push_subtree(nodes: &mut Vec<Node>, children: Vec<u32>) -> u32 {
        let space = Self::build_parameter_space(nodes, &children);
        nodes.push(Node {
            space,
            kind: NodeKind::SubTree(children),
        });
        (nodes.len() - 1) as u32
    }

    fn build(nodes: &mut Vec<Node>, mut children: Vec<u32>, children_per_node: usize) -> u32 {
        assert!(
            !children.is_empty(),
            "Need at least one child to build a node"
        );
        if children.len() == 1 {
            return children[0];
        }
        if children.len() <= children_per_node {
            children.sort_by_key(|&c| {
                let mut total_magnitude = 0i64;
                for parameter in &nodes[c as usize].space {
                    total_magnitude = total_magnitude
                        .wrapping_add((parameter.min.wrapping_add(parameter.max) / 2).wrapping_abs());
                }
                total_magnitude
            });
            return Self::push_subtree(nodes, children);
        }
        let mut min_cost = i64::MAX;
        let mut min_dimension = usize::MAX;
        let mut min_buckets: Vec<u32> = Vec::new();
        for d in 0..PARAMETER_COUNT {
            Self::sort(nodes, &mut children, d, false);
            let buckets = Self::bucketize(nodes, &children, children_per_node);
            let mut total_cost = 0i64;
            for &bucket in &buckets {
                total_cost = total_cost.wrapping_add(Self::cost(&nodes[bucket as usize].space));
            }
            if min_cost > total_cost {
                min_cost = total_cost;
                min_dimension = d;
                min_buckets = buckets;
            }
        }
        Self::sort(nodes, &mut min_buckets, min_dimension, true);
        let mut built = Vec::with_capacity(min_buckets.len());
        for bucket in min_buckets {
            let NodeKind::SubTree(bucket_children) = &nodes[bucket as usize].kind else {
                unreachable!("buckets are subtrees");
            };
            let bucket_children = bucket_children.clone();
            built.push(Self::build(nodes, bucket_children, children_per_node));
        }
        Self::push_subtree(nodes, built)
    }

    /// `RTree.sort`: orders by the center of `dimension`, then of the following dimensions (wrapping).
    fn sort(nodes: &[Node], children: &mut [u32], dimension: usize, absolute: bool) {
        children.sort_by(|&a, &b| {
            for d in 0..PARAMETER_COUNT {
                let dim = (dimension + d) % PARAMETER_COUNT;
                let key = |c: u32| {
                    let parameter = nodes[c as usize].space[dim];
                    let center = parameter.min.wrapping_add(parameter.max) / 2;
                    if absolute { center.wrapping_abs() } else { center }
                };
                let ordering = key(a).cmp(&key(b));
                if ordering.is_ne() {
                    return ordering;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    fn bucketize(nodes: &mut Vec<Node>, children: &[u32], children_per_node: usize) -> Vec<u32> {
        let mut buckets = Vec::new();
        let mut current: Vec<u32> = Vec::new();
        let cpn = children_per_node as f64;
        let exponent = ((children.len() as f64 - 0.01).ln() / cpn.ln()).floor();
        let expected_children_count = cpn.powf(exponent) as i32;
        for &child in children {
            current.push(child);
            if current.len() as i32 >= expected_children_count {
                buckets.push(Self::push_subtree(nodes, std::mem::take(&mut current)));
            }
        }
        if !current.is_empty() {
            buckets.push(Self::push_subtree(nodes, current));
        }
        buckets
    }

    fn cost(space: &Space) -> i64 {
        let mut result = 0i64;
        for parameter in space {
            result = result.wrapping_add(parameter.max.wrapping_sub(parameter.min).wrapping_abs());
        }
        result
    }

    fn build_parameter_space(nodes: &[Node], children: &[u32]) -> Space {
        assert!(!children.is_empty(), "SubTree needs at least one child");
        let mut bounds: [Option<Parameter>; PARAMETER_COUNT] = [None; PARAMETER_COUNT];
        for &child in children {
            for d in 0..PARAMETER_COUNT {
                bounds[d] = Some(nodes[child as usize].space[d].span_with(bounds[d]));
            }
        }
        bounds.map(|b| b.expect("at least one child"))
    }

    /// `RTree.search`: the index of the best matching value; the previous result of the calling thread is
    /// the initial candidate, as in vanilla.
    pub fn search(&self, target: &TargetPoint) -> usize {
        let target = target.to_parameter_array();
        let candidate = LAST_RESULT.with(|last| last.borrow().get(&self.id).copied());
        let leaf = self.search_node(self.root, &target, candidate);
        LAST_RESULT.with(|last| last.borrow_mut().insert(self.id, leaf));
        match self.nodes[leaf as usize].kind {
            NodeKind::Leaf(value) => value,
            NodeKind::SubTree(_) => unreachable!("searches return leaves"),
        }
    }

    fn search_node(
        &self,
        node: u32,
        target: &[i64; PARAMETER_COUNT],
        candidate: Option<u32>,
    ) -> u32 {
        let NodeKind::SubTree(children) = &self.nodes[node as usize].kind else {
            return node;
        };
        let mut min_distance = match candidate {
            None => i64::MAX,
            Some(c) => node_distance(&self.nodes[c as usize].space, target),
        };
        let mut closest = candidate;
        for &child in children {
            let child_distance = node_distance(&self.nodes[child as usize].space, target);
            if min_distance > child_distance {
                let leaf = self.search_node(child, target, closest);
                let leaf_distance = if child == leaf {
                    child_distance
                } else {
                    node_distance(&self.nodes[leaf as usize].space, target)
                };
                if min_distance > leaf_distance {
                    min_distance = leaf_distance;
                    closest = Some(leaf);
                }
            }
        }
        // A subtree is only searched when `min_distance > child_distance` held for the first time with a
        // `None` candidate (distance `i64::MAX`), so a leaf was always found.
        closest.expect("a leaf is found")
    }
}

/// `Climate.ParameterList`.
pub struct ParameterList<T> {
    values: Vec<(ParameterPoint, T)>,
    index: RTree,
}

impl<T> ParameterList<T> {
    /// `new ParameterList(values)` (19 children per node). Panics on an empty list.
    pub fn new(values: Vec<(ParameterPoint, T)>) -> ParameterList<T> {
        Self::with_children_per_node(values, CHILDREN_PER_NODE)
    }

    fn with_children_per_node(
        values: Vec<(ParameterPoint, T)>,
        children_per_node: usize,
    ) -> ParameterList<T> {
        let points: Vec<ParameterPoint> = values.iter().map(|(p, _)| *p).collect();
        let index = RTree::create(&points, children_per_node);
        ParameterList { values, index }
    }

    /// `ParameterList.codec(valueCodec)`: a non-empty list of `{parameters, ...value fields}` objects;
    /// `parse_value` decodes the value from the whole element object.
    pub fn from_json(
        value: &Value,
        parse_value: impl Fn(&Value) -> LoadResult<T>,
    ) -> LoadResult<ParameterList<T>> {
        let values = parse_list(value, |entry| {
            let fields = Fields::new(entry)?;
            let point = fields.required("parameters", ParameterPoint::from_json)?;
            Ok((point, parse_value(entry)?))
        })?;
        if values.is_empty() {
            return Err(LoadError::new("List must have contents"));
        }
        Ok(Self::new(values))
    }

    /// `rebuildWithChildrenPerNode`.
    pub fn rebuild_with_children_per_node(self, children_per_node: usize) -> ParameterList<T> {
        Self::with_children_per_node(self.values, children_per_node)
    }

    /// `values()`.
    pub fn values(&self) -> &[(ParameterPoint, T)] {
        &self.values
    }

    /// `findValue` / `findValueIndex` (the R-tree search).
    pub fn find_value(&self, target: &TargetPoint) -> &T {
        &self.values[self.find_value_index(target)].1
    }

    /// The position of [`Self::find_value`]'s result in [`Self::values`].
    pub fn find_value_index(&self, target: &TargetPoint) -> usize {
        self.index.search(target)
    }

    /// `findValueBruteForce`.
    pub fn find_value_brute_force(&self, target: &TargetPoint) -> &T {
        let mut best = &self.values[0];
        let mut best_fitness = best.0.fitness(target);
        for entry in &self.values[1..] {
            let fitness = entry.0.fitness(target);
            if fitness < best_fitness {
                best_fitness = fitness;
                best = entry;
            }
        }
        &best.1
    }
}

// ---------------------------------------------------------------------------------------------
// Sampler
// ---------------------------------------------------------------------------------------------

/// `Climate.Sampler`: the six climate density functions bound to a sampling context.
pub struct Sampler {
    samplers: DensitySamplerSet,
    temperature: SharedSampler,
    humidity: SharedSampler,
    continentalness: SharedSampler,
    erosion: SharedSampler,
    depth: SharedSampler,
    weirdness: SharedSampler,
}

impl Sampler {
    /// `new Climate.Sampler(temperature, humidity, continentalness, erosion, depth, weirdness)` over
    /// `samplers` (`NoiseRouter.createClimateSampler`).
    pub fn new(
        samplers: DensitySamplerSet,
        temperature: SharedSampler,
        humidity: SharedSampler,
        continentalness: SharedSampler,
        erosion: SharedSampler,
        depth: SharedSampler,
        weirdness: SharedSampler,
    ) -> Sampler {
        Sampler {
            samplers,
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
        }
    }

    fn bind(&self, sampler: &SharedSampler) -> BoundSampler<'_> {
        BoundSampler::new(sampler.clone(), self.samplers.context())
    }

    pub fn temperature(&self) -> BoundSampler<'_> {
        self.bind(&self.temperature)
    }

    pub fn humidity(&self) -> BoundSampler<'_> {
        self.bind(&self.humidity)
    }

    pub fn continentalness(&self) -> BoundSampler<'_> {
        self.bind(&self.continentalness)
    }

    pub fn erosion(&self) -> BoundSampler<'_> {
        self.bind(&self.erosion)
    }

    pub fn depth(&self) -> BoundSampler<'_> {
        self.bind(&self.depth)
    }

    pub fn weirdness(&self) -> BoundSampler<'_> {
        self.bind(&self.weirdness)
    }

    /// The sampler set (random state + context) this sampler was bound to.
    pub fn samplers(&self) -> &DensitySamplerSet {
        &self.samplers
    }

    /// `Sampler.sample(quartX, quartY, quartZ)`.
    pub fn sample(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> TargetPoint {
        let block_x = quart_pos::to_block(quart_x);
        let block_y = quart_pos::to_block(quart_y);
        let block_z = quart_pos::to_block(quart_z);
        let context = self.samplers.context();
        let value = |s: &SharedSampler| s.sample_value(context, block_x, block_y, block_z);
        target(
            value(&self.temperature),
            value(&self.humidity),
            value(&self.continentalness),
            value(&self.erosion),
            value(&self.depth),
            value(&self.weirdness),
        )
    }
}
