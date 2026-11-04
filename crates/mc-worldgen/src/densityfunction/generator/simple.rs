//! `generator/SimpleDensityFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{
    ALL_AXES, AXIS_X, AXIS_Z, CompileContext, DensityFunction, DfOp,
};
use crate::densityfunction::generator::constant::ConstantSampler;
use crate::densityfunction::sampler::{
    BEARDIFIER_KEY, BLENDER_ALPHA_KEY, BLENDER_OFFSET_KEY, ContextBoundSampler, SharedSampler,
};
use crate::util::Interval;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SimpleDensityFunction {
    BlendAlpha,
    BlendOffset,
    Beardifier,
}

impl SimpleDensityFunction {
    pub const ALL: [SimpleDensityFunction; 3] =
        [Self::BlendAlpha, Self::BlendOffset, Self::Beardifier];

    /// Registry id: "blend_alpha", "blend_offset", "beardifier".
    pub fn id(self) -> &'static str {
        match self {
            Self::BlendAlpha => "blend_alpha",
            Self::BlendOffset => "blend_offset",
            Self::Beardifier => "beardifier",
        }
    }
}

impl DfOp for SimpleDensityFunction {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        let (key, fallback) = match self {
            Self::BlendAlpha => (BLENDER_ALPHA_KEY, 1.0),
            Self::BlendOffset => (BLENDER_OFFSET_KEY, 0.0),
            Self::Beardifier => (BEARDIFIER_KEY, 0.0),
        };
        Arc::new(ContextBoundSampler {
            key,
            fallback_sampler: Arc::new(ConstantSampler::new(fallback)),
        })
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }
    fn range(&self) -> Interval {
        match self {
            Self::BlendAlpha => Interval::of(0.0, 1.0),
            Self::BlendOffset | Self::Beardifier => Interval::INFINITE,
        }
    }
    fn domain_axes(&self) -> u8 {
        match self {
            Self::BlendAlpha | Self::BlendOffset => AXIS_X | AXIS_Z,
            Self::Beardifier => ALL_AXES,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::densityfunction::ContextMap;
    use crate::densityfunction::generator::constant::tests::NoCtx;
    use crate::densityfunction::sampler::SamplerContext;

    #[test]
    fn falls_back_to_constant_without_bound_sampler() {
        let ctx = SamplerContext::empty_uncached();
        let alpha = SimpleDensityFunction::BlendAlpha.compile_sampler(&NoCtx);
        let offset = SimpleDensityFunction::BlendOffset.compile_sampler(&NoCtx);
        let beardifier = SimpleDensityFunction::Beardifier.compile_sampler(&NoCtx);
        assert_eq!(alpha.sample_value(&ctx, 1, 2, 3), 1.0);
        assert_eq!(offset.sample_value(&ctx, 1, 2, 3), 0.0);
        assert_eq!(beardifier.sample_value(&ctx, 1, 2, 3), 0.0);
    }

    #[test]
    fn uses_context_bound_sampler() {
        let fields = ContextMap::new().with(
            BLENDER_ALPHA_KEY,
            Arc::new(ConstantSampler::new(0.25)) as SharedSampler,
        );
        let ctx = SamplerContext::builder().set_user_fields(fields).build();
        let alpha = SimpleDensityFunction::BlendAlpha.compile_sampler(&NoCtx);
        assert_eq!(alpha.sample_value(&ctx, 0, 0, 0), 0.25);
        let offset = SimpleDensityFunction::BlendOffset.compile_sampler(&NoCtx);
        assert_eq!(offset.sample_value(&ctx, 0, 0, 0), 0.0);
    }

    #[test]
    fn ranges_axes_and_ids() {
        assert_eq!(
            SimpleDensityFunction::BlendAlpha.range(),
            Interval::of(0.0, 1.0)
        );
        assert_eq!(
            SimpleDensityFunction::Beardifier.range(),
            Interval::INFINITE
        );
        assert_eq!(SimpleDensityFunction::BlendOffset.domain_axes(), 5);
        assert_eq!(SimpleDensityFunction::Beardifier.domain_axes(), 7);
        assert_eq!(SimpleDensityFunction::BlendOffset.id(), "blend_offset");
    }
}
