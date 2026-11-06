//! `DensityFunction` (the declarative tree) and the `DensityFunctions` factory API.
//!
//! A [`DensityFunction`] is an immutable, cheaply clonable tree. Vanilla compares trees structurally
//! (Java records) for sampler/cache deduplication and by identity (`==`) in `rewriteChildren`; here
//! `PartialEq` is structural and [`DensityFunction::ptr_eq`] is identity.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use mc_core::ident::Identifier;
use mc_core::random::RandomSource;

use super::generator::{
    ConstantFunction, DistanceToPointFunction, EndIslandFunction, GradientFunction, NoiseFunction,
    ShiftNoiseFunction, SimpleDensityFunction,
};
use super::op::{
    BinaryFunction, BinaryType, BlendDensityFunction, CacheFunction, ClampFunction,
    FindTopSurfaceFunction, InterpolatedFunction, IntervalSelectFunction, LerpFunction,
    PowFunction, RangeChoiceFunction, RoundFunction, RoundType, SliceFunction, SplineFunction,
    UnaryFunction, UnaryType,
};
use super::sampler::SharedSampler;
use super::{Axis, DfRewriteRule};
use crate::synth::{BlendedNoise, Noise, NormalNoise};
use crate::util::Interval;

pub const AXIS_X: u8 = 1;
pub const AXIS_Y: u8 = 2;
pub const AXIS_Z: u8 = 4;
pub const NO_AXES: u8 = 0;
pub const ALL_AXES: u8 = 7;

/// `DensityFunction.axesFrom(Axis)`.
pub fn axes_from(axis: Axis) -> u8 {
    match axis {
        Axis::X => AXIS_X,
        Axis::Y => AXIS_Y,
        Axis::Z => AXIS_Z,
    }
}

/// `DensityFunction.CompileContext`.
pub trait CompileContext {
    /// `createNoiseSampler(Holder<NormalNoise>)`.
    fn create_noise_sampler(&self, parameters: &NoiseHolder) -> Arc<dyn Noise>;
    /// `createRandom(Identifier)`.
    fn create_random(&self, seed: &Identifier) -> Box<dyn RandomSource>;
    /// `createEndIslandRandom()`.
    fn create_end_island_random(&self) -> Box<dyn RandomSource>;
}

/// Behaviour every concrete function type provides (the non-codec part of vanilla's `DensityFunction`).
pub trait DfOp {
    /// `compileSampler(context)`.
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler;
    /// `rewriteChildren(rule)`: rewrite each child with `rule`; return `this.clone()` (the same Arc) when
    /// no child changed (vanilla relies on that identity), else a new function.
    fn rewrite_children(&self, this: &DensityFunction, rule: &dyn DfRewriteRule)
    -> DensityFunction;
    fn range(&self) -> Interval;
    fn domain_axes(&self) -> u8;
}

/// `Holder<NormalNoise>` referencing a registered noise (equality/hash by key).
#[derive(Clone)]
pub struct NoiseHolder {
    pub key: Identifier,
    pub value: Arc<NormalNoise>,
}
impl PartialEq for NoiseHolder {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for NoiseHolder {}
impl Hash for NoiseHolder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state)
    }
}
impl std::fmt::Debug for NoiseHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoiseHolder({})", self.key)
    }
}

/// `Holder.Reference<DensityFunction>` (equality/hash by key).
#[derive(Clone)]
pub struct DensityFunctionHolder {
    pub key: Identifier,
    pub value: DensityFunction,
}
impl PartialEq for DensityFunctionHolder {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for DensityFunctionHolder {}
impl Hash for DensityFunctionHolder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state)
    }
}
impl std::fmt::Debug for DensityFunctionHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DensityFunctionHolder({})", self.key)
    }
}

/// `DensityFunctions.HolderHolder`: a by-reference use of a registered function.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HolderHolder {
    pub function: DensityFunctionHolder,
}

impl DfOp for HolderHolder {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        self.function.value.compile_sampler(ctx)
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let new_function = rule.rewrite(&self.function.value);
        if new_function.ptr_eq(&self.function.value) {
            this.clone()
        } else {
            new_function
        }
    }
    fn range(&self) -> Interval {
        self.function.value.range()
    }
    fn domain_axes(&self) -> u8 {
        self.function.value.domain_axes()
    }
}

/// Every density function type.
#[derive(Debug)]
pub enum DfNode {
    Constant(ConstantFunction),
    DistanceToPoint(DistanceToPointFunction),
    EndIslands(EndIslandFunction),
    Gradient(GradientFunction),
    Noise(NoiseFunction),
    Shift(ShiftNoiseFunction),
    Simple(SimpleDensityFunction),
    OldBlendedNoise(BlendedNoise),
    Binary(BinaryFunction),
    BlendDensity(BlendDensityFunction),
    Cache(CacheFunction),
    Clamp(ClampFunction),
    FindTopSurface(FindTopSurfaceFunction),
    Interpolated(InterpolatedFunction),
    IntervalSelect(IntervalSelectFunction),
    Lerp(LerpFunction),
    Pow(PowFunction),
    RangeChoice(RangeChoiceFunction),
    Round(RoundFunction),
    Slice(SliceFunction),
    Spline(SplineFunction),
    Unary(UnaryFunction),
    HolderHolder(HolderHolder),
    /// `DensityFunctionCompiler.PreparedCache`: a cache function already bound to a cache id and compiled.
    PreparedCache(PreparedCache),
}

/// `DensityFunctionCompiler.PreparedCache`.
#[derive(Clone)]
pub struct PreparedCache {
    pub id: usize,
    pub range: Interval,
    pub domain_axes: u8,
    pub caching_sampler: SharedSampler,
}
impl std::fmt::Debug for PreparedCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PreparedCache#{}", self.id)
    }
}
impl PartialEq for PreparedCache {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for PreparedCache {}
impl Hash for PreparedCache {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}
impl DfOp for PreparedCache {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        self.caching_sampler.clone()
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }
    fn range(&self) -> Interval {
        self.range
    }
    fn domain_axes(&self) -> u8 {
        self.domain_axes
    }
}

struct Inner {
    node: DfNode,
    range: OnceLock<Interval>,
    domain_axes: OnceLock<u8>,
}

/// Immutable, shareable density function.
#[derive(Clone)]
pub struct DensityFunction(Arc<Inner>);

macro_rules! dispatch {
    ($node:expr, $op:ident => $body:expr) => {
        match $node {
            DfNode::Constant($op) => $body,
            DfNode::DistanceToPoint($op) => $body,
            DfNode::EndIslands($op) => $body,
            DfNode::Gradient($op) => $body,
            DfNode::Noise($op) => $body,
            DfNode::Shift($op) => $body,
            DfNode::Simple($op) => $body,
            DfNode::OldBlendedNoise($op) => $body,
            DfNode::Binary($op) => $body,
            DfNode::BlendDensity($op) => $body,
            DfNode::Cache($op) => $body,
            DfNode::Clamp($op) => $body,
            DfNode::FindTopSurface($op) => $body,
            DfNode::Interpolated($op) => $body,
            DfNode::IntervalSelect($op) => $body,
            DfNode::Lerp($op) => $body,
            DfNode::Pow($op) => $body,
            DfNode::RangeChoice($op) => $body,
            DfNode::Round($op) => $body,
            DfNode::Slice($op) => $body,
            DfNode::Spline($op) => $body,
            DfNode::Unary($op) => $body,
            DfNode::HolderHolder($op) => $body,
            DfNode::PreparedCache($op) => $body,
        }
    };
}

impl DensityFunction {
    pub fn new(node: DfNode) -> DensityFunction {
        DensityFunction(Arc::new(Inner {
            node,
            range: OnceLock::new(),
            domain_axes: OnceLock::new(),
        }))
    }
    pub fn node(&self) -> &DfNode {
        &self.0.node
    }
    /// Reference identity (Java `==`).
    pub fn ptr_eq(&self, other: &DensityFunction) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// `compileSampler(context)`.
    pub fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        dispatch!(&self.0.node, op => op.compile_sampler(ctx))
    }
    /// `rewriteChildren(rule)`.
    pub fn rewrite_children(&self, rule: &dyn DfRewriteRule) -> DensityFunction {
        dispatch!(&self.0.node, op => op.rewrite_children(self, rule))
    }
    /// `range()` (memoized).
    pub fn range(&self) -> Interval {
        *self
            .0
            .range
            .get_or_init(|| dispatch!(&self.0.node, op => op.range()))
    }
    /// `domainAxes()` (memoized).
    pub fn domain_axes(&self) -> u8 {
        *self
            .0
            .domain_axes
            .get_or_init(|| dispatch!(&self.0.node, op => op.domain_axes()))
    }

    // Method names mirror vanilla's `DensityFunction.add/sub/mul/div`.
    // ---- DensityFunctions factory API --------------------------------------------------------

    pub fn zero() -> DensityFunction {
        Self::constant(0.0)
    }
    pub fn constant(value: f32) -> DensityFunction {
        Self::new(DfNode::Constant(ConstantFunction { value }))
    }
    pub fn unary(self, ty: UnaryType) -> DensityFunction {
        Self::new(DfNode::Unary(UnaryFunction { ty, input: self }))
    }
    pub fn binary(
        ty: BinaryType,
        left: DensityFunction,
        right: DensityFunction,
    ) -> DensityFunction {
        Self::new(DfNode::Binary(BinaryFunction { ty, left, right }))
    }
    pub fn abs(self) -> DensityFunction {
        self.unary(UnaryType::Abs)
    }
    pub fn square(self) -> DensityFunction {
        self.unary(UnaryType::Square)
    }
    pub fn cube(self) -> DensityFunction {
        self.unary(UnaryType::Cube)
    }
    pub fn sqrt(self) -> DensityFunction {
        self.unary(UnaryType::Sqrt)
    }
    pub fn half_negative(self) -> DensityFunction {
        self.unary(UnaryType::HalfNegative)
    }
    pub fn quarter_negative(self) -> DensityFunction {
        self.unary(UnaryType::QuarterNegative)
    }
    pub fn reciprocal(self) -> DensityFunction {
        self.unary(UnaryType::Reciprocal)
    }
    pub fn negate(self) -> DensityFunction {
        self.unary(UnaryType::Negate)
    }
    pub fn squeeze(self) -> DensityFunction {
        self.unary(UnaryType::Squeeze)
    }
    pub fn log(self) -> DensityFunction {
        self.unary(UnaryType::Log)
    }
    pub fn sign(self) -> DensityFunction {
        self.unary(UnaryType::Sign)
    }
    pub fn clamp(self, min: f32, max: f32) -> DensityFunction {
        Self::new(DfNode::Clamp(ClampFunction {
            input: self,
            min,
            max,
        }))
    }
    pub fn add(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Add, self, right)
    }
    pub fn add_f(self, right: f32) -> DensityFunction {
        self.add(Self::constant(right))
    }
    pub fn sub(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Sub, self, right)
    }
    pub fn sub_f(self, right: f32) -> DensityFunction {
        self.sub(Self::constant(right))
    }
    pub fn mul(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Mul, self, right)
    }
    pub fn mul_f(self, right: f32) -> DensityFunction {
        self.mul(Self::constant(right))
    }
    pub fn div(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Div, self, right)
    }
    pub fn div_f(self, right: f32) -> DensityFunction {
        self.div(Self::constant(right))
    }
    pub fn min(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Min, self, right)
    }
    pub fn max(self, right: DensityFunction) -> DensityFunction {
        Self::binary(BinaryType::Max, self, right)
    }
    pub fn pow(self, exponent: DensityFunction) -> DensityFunction {
        Self::new(DfNode::Pow(PowFunction {
            base: self,
            exponent,
        }))
    }
    /// `DensityFunction.pow(float)`: special-cases 0.5, 2 and 3 like vanilla.
    pub fn pow_f(self, exponent: f32) -> DensityFunction {
        if exponent == 0.5 {
            self.sqrt()
        } else if exponent == 2.0 {
            self.square()
        } else if exponent == 3.0 {
            self.cube()
        } else {
            self.pow(Self::constant(exponent))
        }
    }
    pub fn interpolated(self, cell_size_xz: i32, cell_size_y: i32) -> DensityFunction {
        Self::new(DfNode::Interpolated(InterpolatedFunction {
            input: self,
            cell_size_xz,
            cell_size_y,
        }))
    }
    pub fn cache(self) -> DensityFunction {
        Self::new(DfNode::Cache(CacheFunction { input: self }))
    }
    pub fn blend_density(self) -> DensityFunction {
        Self::new(DfNode::BlendDensity(BlendDensityFunction { input: self }))
    }
    pub fn lerp(
        alpha: DensityFunction,
        first: DensityFunction,
        second: DensityFunction,
    ) -> DensityFunction {
        Self::new(DfNode::Lerp(LerpFunction {
            alpha,
            first,
            second,
        }))
    }
    pub fn round_with(self, ty: RoundType, multiple: DensityFunction) -> DensityFunction {
        Self::new(DfNode::Round(RoundFunction {
            ty,
            input: self,
            multiple,
        }))
    }
    pub fn range_choice(
        self,
        min_inclusive: f32,
        max_exclusive: f32,
        when_in_range: DensityFunction,
        when_out_of_range: DensityFunction,
    ) -> DensityFunction {
        Self::new(DfNode::RangeChoice(RangeChoiceFunction {
            input: self,
            min_inclusive,
            max_exclusive,
            when_in_range,
            when_out_of_range,
        }))
    }
    pub fn slice(self, axis: Axis, coordinate: i32) -> DensityFunction {
        Self::new(DfNode::Slice(SliceFunction {
            axis,
            coordinate,
            input: self,
        }))
    }
    /// `DensityFunctions.yClampedGradient`.
    pub fn y_clamped_gradient(
        from_y: i32,
        to_y: i32,
        from_value: f32,
        to_value: f32,
    ) -> DensityFunction {
        Self::new(DfNode::Gradient(GradientFunction {
            axis: Axis::Y,
            tiling: super::generator::TilingMode::ClampToEdge,
            from_coordinate: from_y,
            to_coordinate: to_y,
            from_value,
            to_value,
        }))
    }
    /// `DensityFunctions.remap`.
    pub fn remap(self, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> DensityFunction {
        let factor = (to_max - to_min) / (from_max - from_min);
        let offset = to_min - from_min * factor;
        if offset == 0.0 {
            self.mul_f(factor)
        } else {
            self.mul_f(factor).add_f(offset)
        }
    }
    /// `DensityFunctions.clampedMap`.
    pub fn clamped_map(
        self,
        from_min: f32,
        from_max: f32,
        to_min: f32,
        to_max: f32,
    ) -> DensityFunction {
        self.clamp(from_min, from_max)
            .remap(from_min, from_max, to_min, to_max)
    }
}

impl PartialEq for DensityFunction {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0.node == other.0.node
    }
}
impl Eq for DensityFunction {}
impl Hash for DensityFunction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.node.hash(state)
    }
}
impl std::fmt::Debug for DensityFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.node.fmt(f)
    }
}

impl PartialEq for DfNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DfNode::Constant(a), DfNode::Constant(b)) => a == b,
            (DfNode::DistanceToPoint(a), DfNode::DistanceToPoint(b)) => a == b,
            (DfNode::EndIslands(a), DfNode::EndIslands(b)) => a == b,
            (DfNode::Gradient(a), DfNode::Gradient(b)) => a == b,
            (DfNode::Noise(a), DfNode::Noise(b)) => a == b,
            (DfNode::Shift(a), DfNode::Shift(b)) => a == b,
            (DfNode::Simple(a), DfNode::Simple(b)) => a == b,
            (DfNode::OldBlendedNoise(a), DfNode::OldBlendedNoise(b)) => a == b,
            (DfNode::Binary(a), DfNode::Binary(b)) => a == b,
            (DfNode::BlendDensity(a), DfNode::BlendDensity(b)) => a == b,
            (DfNode::Cache(a), DfNode::Cache(b)) => a == b,
            (DfNode::Clamp(a), DfNode::Clamp(b)) => a == b,
            (DfNode::FindTopSurface(a), DfNode::FindTopSurface(b)) => a == b,
            (DfNode::Interpolated(a), DfNode::Interpolated(b)) => a == b,
            (DfNode::IntervalSelect(a), DfNode::IntervalSelect(b)) => a == b,
            (DfNode::Lerp(a), DfNode::Lerp(b)) => a == b,
            (DfNode::Pow(a), DfNode::Pow(b)) => a == b,
            (DfNode::RangeChoice(a), DfNode::RangeChoice(b)) => a == b,
            (DfNode::Round(a), DfNode::Round(b)) => a == b,
            (DfNode::Slice(a), DfNode::Slice(b)) => a == b,
            (DfNode::Spline(a), DfNode::Spline(b)) => a == b,
            (DfNode::Unary(a), DfNode::Unary(b)) => a == b,
            (DfNode::HolderHolder(a), DfNode::HolderHolder(b)) => a == b,
            (DfNode::PreparedCache(a), DfNode::PreparedCache(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for DfNode {}
impl Hash for DfNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        dispatch!(self, op => op.hash(state))
    }
}
