//! `BlendDensityFunction`: blends the input density with the neighbouring old chunks via the context's `Blender`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    BLENDER_KEY, Blender, DensityBuffer, DensitySampler, DensityVolume, SamplerContext,
    SharedSampler,
};
use crate::util::Interval;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlendDensityFunction {
    pub input: DensityFunction,
}

impl DfOp for BlendDensityFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(Sampler::new(self.input.compile_sampler(ctx)))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        if input.ptr_eq(&self.input) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::BlendDensity(BlendDensityFunction { input }))
        }
    }
    fn range(&self) -> Interval {
        self.input.range()
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
    }
}

/// `BlendDensityFunction.Sampler`.
pub struct Sampler {
    input: SharedSampler,
}

impl Sampler {
    pub fn new(input: SharedSampler) -> Sampler {
        Sampler { input }
    }
}

/// The context's blender, unless absent or empty.
fn active_blender(context: &SamplerContext) -> Option<&dyn Blender> {
    context
        .get_field(&BLENDER_KEY)
        .map(|b| &**b)
        .filter(|b| !b.is_empty())
}

impl DensitySampler for Sampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        if let Some(blender) = active_blender(context) {
            let mut index = 0;
            for z in 0..volume.size_z {
                let block_z = volume.block_z(z);
                for x in 0..volume.size_x {
                    let block_x = volume.block_x(x);
                    for y in 0..volume.size_y {
                        let block_y = volume.block_y(y);
                        out.set(
                            index,
                            blender.blend_density(block_x, block_y, block_z, out.get(index)),
                        );
                        index += 1;
                    }
                }
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let input = self.input.sample_value(context, block_x, block_y, block_z);
        match active_blender(context) {
            Some(blender) => blender.blend_density(block_x, block_y, block_z, input),
            None => input,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::op::slice::test_support::fn_sampler;
    use crate::densityfunction::sampler::{ContextMap, SharedBlender};

    struct Blender2x {
        empty: bool,
    }
    impl Blender for Blender2x {
        fn is_empty(&self) -> bool {
            self.empty
        }
        fn blend_density(&self, x: i32, y: i32, z: i32, input: f32) -> f32 {
            input * 2.0 + (x + 10 * y + 100 * z) as f32
        }
    }

    fn context(blender: Option<bool>) -> SamplerContext {
        let mut fields = ContextMap::new();
        if let Some(empty) = blender {
            fields = fields.with(BLENDER_KEY, Arc::new(Blender2x { empty }) as SharedBlender);
        }
        SamplerContext::builder().set_user_fields(fields).build()
    }

    #[test]
    fn blends_only_with_non_empty_blender() {
        let sampler = Sampler::new(fn_sampler(|x, _, _| x as f32));
        // 2 * 3 + (3 + 10 * 4 + 100 * 5)
        assert_eq!(sampler.sample_value(&context(Some(false)), 3, 4, 5), 549.0);
        assert_eq!(sampler.sample_value(&context(Some(true)), 3, 4, 5), 3.0);
        assert_eq!(sampler.sample_value(&context(None), 3, 4, 5), 3.0);
    }

    #[test]
    fn volume_blends_every_cell_in_layout_order() {
        let sampler = Sampler::new(fn_sampler(|x, _, _| x as f32));
        let volume = DensityVolume::new(2, 2, 2, 1, 2, 3, 1, 1, 1);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&context(Some(false)), &mut out, &volume);
        for z in 0..2 {
            for x in 0..2 {
                for y in 0..2 {
                    let (bx, by, bz) = (volume.block_x(x), volume.block_y(y), volume.block_z(z));
                    let expected = bx as f32 * 2.0 + (bx + 10 * by + 100 * bz) as f32;
                    assert_eq!(out.get(volume.index_unchecked(x, y, z)), expected);
                }
            }
        }
        sampler.sample_volume(&context(Some(true)), &mut out, &volume);
        assert_eq!(out.get(volume.index_unchecked(1, 1, 1)), 2.0);
    }
}
