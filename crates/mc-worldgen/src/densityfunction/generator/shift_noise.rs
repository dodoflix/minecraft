//! `generator/ShiftNoiseFunction.java` (`Shift`, `ShiftA`, `ShiftB`).
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{
    ALL_AXES, AXIS_X, AXIS_Z, CompileContext, DensityFunction, DfNode, DfOp, NoiseHolder,
};
use crate::densityfunction::generator::noise_function::NoiseSampler;
use crate::densityfunction::op::binary::ConstMulSampler;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::synth::Noise;
use crate::util::Interval;

/// `ShiftNoiseFunction.COORDINATE_FACTOR`.
const COORDINATE_FACTOR: f64 = 0.25;
/// `ShiftNoiseFunction.VALUE_FACTOR`.
const VALUE_FACTOR: f32 = 4.0;

/// `shift_a` / `shift_b` / `shift` (vanilla: three records implementing `ShiftNoiseFunction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShiftKind {
    Shift,
    ShiftA,
    ShiftB,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShiftNoiseFunction {
    pub kind: ShiftKind,
    pub offset_noise: NoiseHolder,
}

impl DfOp for ShiftNoiseFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let noise = ctx.create_noise_sampler(&self.offset_noise);
        match self.kind {
            ShiftKind::Shift => Arc::new(ConstMulSampler::new(
                Arc::new(NoiseSampler::new(
                    noise,
                    COORDINATE_FACTOR,
                    COORDINATE_FACTOR,
                )),
                VALUE_FACTOR,
            )),
            ShiftKind::ShiftA => Arc::new(ConstMulSampler::new(
                Arc::new(NoiseSampler::new(noise, COORDINATE_FACTOR, 0.0)),
                VALUE_FACTOR,
            )),
            ShiftKind::ShiftB => Arc::new(ShiftBSampler { noise }),
        }
    }
    /// Vanilla returns a fresh record here (not `this`).
    fn rewrite_children(
        &self,
        _this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        DensityFunction::new(DfNode::Shift(self.clone()))
    }
    fn range(&self) -> Interval {
        Interval::mul(
            self.offset_noise.value.range(),
            Interval::of_exact(VALUE_FACTOR),
        )
    }
    fn domain_axes(&self) -> u8 {
        match self.kind {
            ShiftKind::Shift => ALL_AXES,
            ShiftKind::ShiftA | ShiftKind::ShiftB => AXIS_X | AXIS_Z,
        }
    }
}

/// Anonymous sampler of `ShiftNoiseFunction.ShiftB`: samples the noise at `(z, x, 0)`.
struct ShiftBSampler {
    noise: Arc<dyn Noise>,
}

impl DensitySampler for ShiftBSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        let transposed_volume = DensityVolume::new(
            volume.size_z,
            volume.size_x,
            1,
            volume.min_block_z,
            volume.min_block_x,
            0,
            volume.step_block_z,
            volume.step_block_x,
            1,
        );

        let mut transposed_buffer = context.acquire_buffer(&transposed_volume);
        transposed_buffer.fill(0.0);
        self.noise.add_to_volume(
            &mut transposed_buffer,
            &transposed_volume,
            COORDINATE_FACTOR,
            COORDINATE_FACTOR,
            VALUE_FACTOR,
        );

        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                let value = transposed_buffer.get(transposed_volume.index_unchecked(z, x, 0));
                out.set_range(
                    volume.index_unchecked(x, 0, z),
                    volume.size_y as usize,
                    value,
                );
            }
        }
    }
    fn sample_value(
        &self,
        _context: &SamplerContext,
        block_x: i32,
        _block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.noise.get3(
            block_z as f64 * COORDINATE_FACTOR,
            block_x as f64 * COORDINATE_FACTOR,
            0.0,
        ) * VALUE_FACTOR
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::assert_volume_matches_value;
    use crate::densityfunction::generator::noise_function::tests::{MockNoiseCtx, holder};

    fn shift(kind: ShiftKind) -> DensityFunction {
        DensityFunction::new(DfNode::Shift(ShiftNoiseFunction {
            kind,
            offset_noise: holder(),
        }))
    }

    fn at(function: &DensityFunction, x: i32, y: i32, z: i32) -> f32 {
        function.compile_sampler(&MockNoiseCtx).sample_value(
            &SamplerContext::empty_uncached(),
            x,
            y,
            z,
        )
    }

    #[test]
    fn point_values_follow_vanilla_coordinate_mapping() {
        // MockNoise.get3 = x + 2y + 3z, all times 4.
        // Shift: (x/4, y/4, z/4); ShiftA: (x/4, 0, z/4); ShiftB: (z/4, x/4, 0)
        assert_eq!(
            at(&shift(ShiftKind::Shift), 8, 4, 12),
            (2.0 + 2.0 + 9.0) * 4.0
        );
        assert_eq!(
            at(&shift(ShiftKind::ShiftA), 8, 4, 12),
            (2.0 + 0.0 + 9.0) * 4.0
        );
        assert_eq!(
            at(&shift(ShiftKind::ShiftB), 8, 4, 12),
            (3.0 + 4.0 + 0.0) * 4.0
        );
    }

    #[test]
    fn volume_path_matches_point_path_including_transposed_shift_b() {
        for kind in [ShiftKind::Shift, ShiftKind::ShiftA, ShiftKind::ShiftB] {
            assert_volume_matches_value(&*shift(kind).compile_sampler(&MockNoiseCtx));
        }
    }

    #[test]
    fn axes_and_rewrite() {
        assert_eq!(shift(ShiftKind::Shift).domain_axes(), 7);
        assert_eq!(shift(ShiftKind::ShiftA).domain_axes(), 5);
        assert_eq!(shift(ShiftKind::ShiftB).domain_axes(), 5);
        let function = shift(ShiftKind::ShiftB);
        let rewritten = function.rewrite_children(&|f: &DensityFunction| f.clone());
        // vanilla returns a new record, equal but not identical
        assert!(!rewritten.ptr_eq(&function));
        assert_eq!(rewritten, function);
    }

    #[test]
    fn range_is_noise_range_times_four() {
        let noise_range = holder().value.range();
        assert_eq!(
            shift(ShiftKind::Shift).range(),
            Interval::mul(noise_range, Interval::of_exact(4.0))
        );
    }
}
