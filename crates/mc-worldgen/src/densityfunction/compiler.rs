//! `DfRewriteRule` and `DensityFunctionCompiler`: reference inlining, cache deduplication, uniform-axis
//! slicing, then `compileSampler`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::Axis;
use super::function::{CompileContext, DensityFunction, DfNode, PreparedCache, axes_from};
use super::generator::{ConstantFunction, GradientFunction};
use super::op::{CacheFunction, SliceFunction};
use super::sampler::{CachingDensitySampler, SharedSampler};

/// `DfRewriteRule`: rewrites a function; must return the same `Arc` when nothing changed.
pub trait DfRewriteRule {
    fn rewrite(&self, function: &DensityFunction) -> DensityFunction;
}

impl<F: Fn(&DensityFunction) -> DensityFunction> DfRewriteRule for F {
    fn rewrite(&self, function: &DensityFunction) -> DensityFunction {
        self(function)
    }
}

/// `DfRewriteRule.inline_reference`: replaces a by-reference use by the referenced function.
pub fn inline_reference(function: &DensityFunction) -> DensityFunction {
    match function.node() {
        DfNode::HolderHolder(h) => h.function.value.clone(),
        _ => function.clone(),
    }
}

/// `DfRewriteRule.SliceUniformAxes`: wraps subtrees in `slice` nodes for the axes they are uniform in.
#[derive(Clone, Copy, Debug)]
pub struct SliceUniformAxes {
    pub parent_domain_axes: u8,
}

impl SliceUniformAxes {
    /// `DfRewriteRule.SLICE_UNIFORM_AXES`.
    pub const ROOT: SliceUniformAxes = SliceUniformAxes {
        parent_domain_axes: 7,
    };

    fn should_skip(function: &DensityFunction) -> bool {
        matches!(
            function.node(),
            DfNode::Constant(ConstantFunction { .. }) | DfNode::Gradient(GradientFunction { .. })
        )
    }

    fn existing_removed_axes(function: &DensityFunction) -> u8 {
        let mut axes = 0;
        let mut function = function.clone();
        loop {
            let next = match function.node() {
                DfNode::Slice(slice) => {
                    axes |= axes_from(slice.axis);
                    slice.input.clone()
                }
                _ => return axes,
            };
            function = next;
        }
    }

    fn remove_axes(mut function: DensityFunction, axes: u8) -> DensityFunction {
        let filtered = axes & !Self::existing_removed_axes(&function);
        if filtered & 1 != 0 {
            function = DensityFunction::new(DfNode::Slice(SliceFunction {
                axis: Axis::X,
                coordinate: 0,
                input: function,
            }));
        }
        if filtered & 4 != 0 {
            function = DensityFunction::new(DfNode::Slice(SliceFunction {
                axis: Axis::Z,
                coordinate: 0,
                input: function,
            }));
        }
        if filtered & 2 != 0 {
            function = DensityFunction::new(DfNode::Slice(SliceFunction {
                axis: Axis::Y,
                coordinate: 0,
                input: function,
            }));
        }
        function
    }
}

impl DfRewriteRule for SliceUniformAxes {
    fn rewrite(&self, function: &DensityFunction) -> DensityFunction {
        if Self::should_skip(function) {
            return function.clone();
        }
        let domain_axes = function.domain_axes();
        if self.parent_domain_axes == domain_axes {
            function.rewrite_children(self)
        } else {
            let new_function = function.rewrite_children(&SliceUniformAxes {
                parent_domain_axes: domain_axes,
            });
            let removed_axes = self.parent_domain_axes & !domain_axes;
            Self::remove_axes(new_function, removed_axes)
        }
    }
}

struct CompilerState {
    samplers: HashMap<DensityFunction, SharedSampler>,
    prepared_caches: HashMap<DensityFunction, DensityFunction>,
    next_cache_id: usize,
}

/// `DensityFunctionCompiler`: turns declarative functions into (memoized) samplers.
pub struct DensityFunctionCompiler {
    context: Box<dyn CompileContext + Send + Sync>,
    state: Mutex<CompilerState>,
}

impl DensityFunctionCompiler {
    pub fn new(context: Box<dyn CompileContext + Send + Sync>) -> DensityFunctionCompiler {
        DensityFunctionCompiler {
            context,
            state: Mutex::new(CompilerState {
                samplers: HashMap::new(),
                prepared_caches: HashMap::new(),
                next_cache_id: 0,
            }),
        }
    }

    pub fn context(&self) -> &(dyn CompileContext + Send + Sync) {
        &*self.context
    }

    /// `getSampler(function)`.
    pub fn get_sampler(&self, function: &DensityFunction) -> SharedSampler {
        let mut state = self
            .state
            .lock()
            .expect("density function compiler lock poisoned");
        if let Some(sampler) = state.samplers.get(function) {
            return sampler.clone();
        }
        let optimized = self.optimize(&mut state, function);
        let sampler = optimized.compile_sampler(&*self.context);
        state.samplers.insert(function.clone(), sampler.clone());
        sampler
    }

    /// `optimizerRule.rewrite(function)` = `sequence(inline-references + cache preparation, SLICE_UNIFORM_AXES)`.
    fn optimize(&self, state: &mut CompilerState, function: &DensityFunction) -> DensityFunction {
        let function = self.prepare(state, function);
        SliceUniformAxes::ROOT.rewrite(&function)
    }

    /// The first rule of the optimizer sequence (applied recursively to children by itself).
    fn prepare(&self, state: &mut CompilerState, function: &DensityFunction) -> DensityFunction {
        let function = inline_reference(function);
        if let DfNode::Cache(cache) = function.node() {
            return self.reuse_or_prepare_cache(state, cache);
        }
        // `rewrite_children` takes a `&dyn DfRewriteRule`; the state is threaded through a RefCell.
        let cell = std::cell::RefCell::new(&mut *state);
        function.rewrite_children(&|child: &DensityFunction| {
            self.prepare(&mut cell.borrow_mut(), child)
        })
    }

    fn reuse_or_prepare_cache(
        &self,
        state: &mut CompilerState,
        cache: &CacheFunction,
    ) -> DensityFunction {
        if let Some(prepared) = state.prepared_caches.get(&cache.input) {
            return prepared.clone();
        }
        let prepared = self.prepare_cache(state, cache);
        state
            .prepared_caches
            .insert(cache.input.clone(), prepared.clone());
        prepared
    }

    fn prepare_cache(&self, state: &mut CompilerState, cache: &CacheFunction) -> DensityFunction {
        let id = state.next_cache_id;
        state.next_cache_id += 1;
        let input = self.optimize(state, &cache.input);
        let caching_sampler: SharedSampler = Arc::new(CachingDensitySampler {
            id,
            input: input.compile_sampler(&*self.context),
        });
        DensityFunction::new(DfNode::PreparedCache(PreparedCache {
            id,
            range: input.range(),
            domain_axes: input.domain_axes(),
            caching_sampler,
        }))
    }
}
