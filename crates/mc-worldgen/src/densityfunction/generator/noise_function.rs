//! `generator/NoiseFunction.java`.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{
    ALL_AXES, AXIS_X, AXIS_Y, AXIS_Z, CompileContext, DensityFunction, DfNode, DfOp, NoiseHolder,
};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::synth::Noise;
use crate::util::Interval;

#[derive(Clone, Debug)]
pub struct NoiseFunction {
    pub noise: NoiseHolder,
    pub xz_scale: f64,
    pub y_scale: f64,
    pub shift_x: DensityFunction,
    pub shift_y: DensityFunction,
    pub shift_z: DensityFunction,
}
impl PartialEq for NoiseFunction {
    fn eq(&self, o: &Self) -> bool {
        self.noise == o.noise
            && self.xz_scale.to_bits() == o.xz_scale.to_bits()
            && self.y_scale.to_bits() == o.y_scale.to_bits()
            && self.shift_x == o.shift_x
            && self.shift_y == o.shift_y
            && self.shift_z == o.shift_z
    }
}
impl Eq for NoiseFunction {}
impl Hash for NoiseFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.noise.hash(s);
        self.xz_scale.to_bits().hash(s);
        self.y_scale.to_bits().hash(s);
        self.shift_x.hash(s);
        self.shift_y.hash(s);
        self.shift_z.hash(s);
    }
}

impl DfOp for NoiseFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let noise = ctx.create_noise_sampler(&self.noise);
        let zero = DensityFunction::zero();
        if self.shift_x == zero && self.shift_y == zero && self.shift_z == zero {
            return Arc::new(NoiseSampler::new(noise, self.xz_scale, self.y_scale));
        }
        let shift_x = self.shift_x.compile_sampler(ctx);
        let shift_z = self.shift_z.compile_sampler(ctx);
        if self.shift_y == zero {
            Arc::new(ShiftedXzSampler::new(
                shift_x,
                shift_z,
                noise,
                self.xz_scale,
                self.y_scale,
            ))
        } else {
            let shift_y = self.shift_y.compile_sampler(ctx);
            Arc::new(ShiftedXyzSampler::new(
                shift_x,
                shift_y,
                shift_z,
                noise,
                self.xz_scale,
                self.y_scale,
            ))
        }
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let shift_x = rule.rewrite(&self.shift_x);
        let shift_y = rule.rewrite(&self.shift_y);
        let shift_z = rule.rewrite(&self.shift_z);
        if shift_x.ptr_eq(&self.shift_x)
            && shift_y.ptr_eq(&self.shift_y)
            && shift_z.ptr_eq(&self.shift_z)
        {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Noise(NoiseFunction {
                noise: self.noise.clone(),
                xz_scale: self.xz_scale,
                y_scale: self.y_scale,
                shift_x,
                shift_y,
                shift_z,
            }))
        }
    }
    fn range(&self) -> Interval {
        self.noise.value.range()
    }
    fn domain_axes(&self) -> u8 {
        let mut axes = ALL_AXES;
        if self.y_scale == 0.0 {
            axes &= !AXIS_Y;
        }
        if self.xz_scale == 0.0 {
            axes &= !(AXIS_X | AXIS_Z);
        }
        axes | self.shift_x.domain_axes() | self.shift_y.domain_axes() | self.shift_z.domain_axes()
    }
}

/// `NoiseFunction.Sampler`.
pub struct NoiseSampler {
    noise: Arc<dyn Noise>,
    xz_scale: f64,
    y_scale: f64,
}

impl NoiseSampler {
    pub fn new(noise: Arc<dyn Noise>, xz_scale: f64, y_scale: f64) -> NoiseSampler {
        NoiseSampler {
            noise,
            xz_scale,
            y_scale,
        }
    }
}

impl DensitySampler for NoiseSampler {
    fn sample_volume(
        &self,
        _context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        out.fill(0.0);
        self.noise
            .add_to_volume(out, volume, self.xz_scale, self.y_scale, 1.0);
    }
    fn sample_value(
        &self,
        _context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.noise.get3(
            block_x as f64 * self.xz_scale,
            block_y as f64 * self.y_scale,
            block_z as f64 * self.xz_scale,
        )
    }
}

/// `NoiseFunction.ShiftedXyzSampler`.
pub struct ShiftedXyzSampler {
    shift_x: SharedSampler,
    shift_y: SharedSampler,
    shift_z: SharedSampler,
    noise: Arc<dyn Noise>,
    xz_scale: f64,
    y_scale: f64,
}

impl ShiftedXyzSampler {
    pub fn new(
        shift_x: SharedSampler,
        shift_y: SharedSampler,
        shift_z: SharedSampler,
        noise: Arc<dyn Noise>,
        xz_scale: f64,
        y_scale: f64,
    ) -> ShiftedXyzSampler {
        ShiftedXyzSampler {
            shift_x,
            shift_y,
            shift_z,
            noise,
            xz_scale,
            y_scale,
        }
    }
}

impl DensitySampler for ShiftedXyzSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.shift_x.sample_volume(context, out, volume);

        let mut shift_y_buffer = context.acquire_buffer(volume);
        self.shift_y
            .sample_volume(context, &mut shift_y_buffer, volume);

        let mut shift_z_buffer = context.acquire_buffer(volume);
        self.shift_z
            .sample_volume(context, &mut shift_z_buffer, volume);

        let mut index = 0;
        for z in 0..volume.size_z {
            let base_noise_z = volume.block_z(z) as f64 * self.xz_scale;
            for x in 0..volume.size_x {
                let base_noise_x = volume.block_x(x) as f64 * self.xz_scale;
                for y in 0..volume.size_y {
                    let noise_x = base_noise_x + out.get(index) as f64;
                    let noise_y =
                        volume.block_y(y) as f64 * self.y_scale + shift_y_buffer.get(index) as f64;
                    let noise_z = base_noise_z + shift_z_buffer.get(index) as f64;
                    out.set(index, self.noise.get3(noise_x, noise_y, noise_z));
                    index += 1;
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
        let x = block_x as f64 * self.xz_scale
            + self
                .shift_x
                .sample_value(context, block_x, block_y, block_z) as f64;
        let y = block_y as f64 * self.y_scale
            + self
                .shift_y
                .sample_value(context, block_x, block_y, block_z) as f64;
        let z = block_z as f64 * self.xz_scale
            + self
                .shift_z
                .sample_value(context, block_x, block_y, block_z) as f64;
        self.noise.get3(x, y, z)
    }
}

/// `NoiseFunction.ShiftedXzSampler`.
pub struct ShiftedXzSampler {
    shift_x: SharedSampler,
    shift_z: SharedSampler,
    noise: Arc<dyn Noise>,
    xz_scale: f64,
    y_scale: f64,
}

impl ShiftedXzSampler {
    pub fn new(
        shift_x: SharedSampler,
        shift_z: SharedSampler,
        noise: Arc<dyn Noise>,
        xz_scale: f64,
        y_scale: f64,
    ) -> ShiftedXzSampler {
        ShiftedXzSampler {
            shift_x,
            shift_z,
            noise,
            xz_scale,
            y_scale,
        }
    }
}

impl DensitySampler for ShiftedXzSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.shift_x.sample_volume(context, out, volume);

        let mut shift_z_buffer = context.acquire_buffer(volume);
        self.shift_z
            .sample_volume(context, &mut shift_z_buffer, volume);

        let mut index = 0;
        for z in 0..volume.size_z {
            let base_noise_z = volume.block_z(z) as f64 * self.xz_scale;
            for x in 0..volume.size_x {
                let base_noise_x = volume.block_x(x) as f64 * self.xz_scale;
                for y in 0..volume.size_y {
                    let noise_x = base_noise_x + out.get(index) as f64;
                    let noise_y = volume.block_y(y) as f64 * self.y_scale;
                    let noise_z = base_noise_z + shift_z_buffer.get(index) as f64;
                    out.set(index, self.noise.get3(noise_x, noise_y, noise_z));
                    index += 1;
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
        let x = block_x as f64 * self.xz_scale
            + self
                .shift_x
                .sample_value(context, block_x, block_y, block_z) as f64;
        let y = block_y as f64 * self.y_scale;
        let z = block_z as f64 * self.xz_scale
            + self
                .shift_z
                .sample_value(context, block_x, block_y, block_z) as f64;
        self.noise.get3(x, y, z)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::assert_volume_matches_value;
    use crate::synth::NormalNoise;
    use mc_core::ident::Identifier;
    use mc_core::random::RandomSource;

    /// `get3(x, y, z) = x + 2y + 3z`, `get2(x, y) = x - y`.
    pub(crate) struct MockNoise;
    impl Noise for MockNoise {
        fn range(&self) -> Interval {
            Interval::of_symmetric(10.0)
        }
        fn get2(&self, x: f64, y: f64) -> f32 {
            (x - y) as f32
        }
        fn get3(&self, x: f64, y: f64, z: f64) -> f32 {
            (x + 2.0 * y + 3.0 * z) as f32
        }
    }

    /// Compile context whose every noise is [`MockNoise`].
    pub(crate) struct MockNoiseCtx;
    impl CompileContext for MockNoiseCtx {
        fn create_noise_sampler(&self, _: &NoiseHolder) -> Arc<dyn Noise> {
            Arc::new(MockNoise)
        }
        fn create_random(&self, _: &Identifier) -> Box<dyn RandomSource> {
            unreachable!("test does not use random")
        }
        fn create_end_island_random(&self) -> Box<dyn RandomSource> {
            unreachable!("test does not use random")
        }
    }

    pub(crate) fn holder() -> NoiseHolder {
        NoiseHolder {
            key: Identifier::parse("test:noise").expect("valid identifier"),
            value: Arc::new(NormalNoise::builder().build()),
        }
    }

    fn noise_function(
        xz_scale: f64,
        y_scale: f64,
        shifts: [DensityFunction; 3],
    ) -> DensityFunction {
        let [shift_x, shift_y, shift_z] = shifts;
        DensityFunction::new(DfNode::Noise(NoiseFunction {
            noise: holder(),
            xz_scale,
            y_scale,
            shift_x,
            shift_y,
            shift_z,
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

    fn no_shift() -> [DensityFunction; 3] {
        [
            DensityFunction::zero(),
            DensityFunction::zero(),
            DensityFunction::zero(),
        ]
    }

    #[test]
    fn unshifted_samples_scaled_coordinates() {
        // (4*0.5) + 2*(2*0.25) + 3*(1*0.5) = 2 + 1 + 1.5
        assert_eq!(at(&noise_function(0.5, 0.25, no_shift()), 4, 2, 1), 4.5);
    }

    #[test]
    fn shifted_xz_leaves_y_unshifted() {
        let [_, _, zero] = no_shift();
        // x = 2 + 3, y = 0.5, z = 0.5 + 0 -> 5 + 1 + 1.5
        let shifted = noise_function(
            0.5,
            0.25,
            [DensityFunction::constant(3.0), zero.clone(), zero],
        );
        assert_eq!(at(&shifted, 4, 2, 1), 7.5);
    }

    #[test]
    fn shifted_xyz_shifts_every_axis() {
        let shifted = noise_function(
            0.5,
            0.25,
            [
                DensityFunction::constant(3.0),
                DensityFunction::constant(1.0),
                DensityFunction::constant(-2.0),
            ],
        );
        // x = 5, y = 1.5, z = -1.5 -> 5 + 3 - 4.5
        assert_eq!(at(&shifted, 4, 2, 1), 3.5);
    }

    #[test]
    fn volume_path_matches_point_path_for_every_sampler() {
        let varying = DensityFunction::y_clamped_gradient(-8, 8, -3.0, 5.0);
        let x_varying = DensityFunction::new(DfNode::Gradient(
            crate::densityfunction::generator::GradientFunction {
                axis: crate::densityfunction::Axis::X,
                tiling: crate::densityfunction::generator::TilingMode::Repeat,
                from_coordinate: 0,
                to_coordinate: 3,
                from_value: 0.5,
                to_value: 2.0,
            },
        ));
        let zero = DensityFunction::zero();
        for shifts in [
            no_shift(),
            [varying.clone(), zero.clone(), x_varying.clone()],
            [varying.clone(), x_varying.clone(), varying.clone()],
        ] {
            let function = noise_function(0.75, 0.125, shifts);
            assert_volume_matches_value(&*function.compile_sampler(&MockNoiseCtx));
        }
    }

    #[test]
    fn domain_axes_drop_axes_of_zero_scales_then_add_shift_axes() {
        let zero = DensityFunction::zero;
        let y_axis = DensityFunction::y_clamped_gradient(0, 1, 0.0, 1.0);
        let axes = |xz: f64, y: f64, shifts| noise_function(xz, y, shifts).domain_axes();
        assert_eq!(axes(1.0, 1.0, no_shift()), 7);
        assert_eq!(axes(1.0, 0.0, no_shift()), 5);
        assert_eq!(axes(0.0, 1.0, no_shift()), 2);
        assert_eq!(axes(0.0, 0.0, no_shift()), 0);
        assert_eq!(axes(0.0, 0.0, [zero(), y_axis.clone(), zero()]), 2);
        assert_eq!(axes(1.0, 0.0, [zero(), y_axis, zero()]), 7);
    }

    #[test]
    fn range_is_the_noise_range_and_rewrite_preserves_identity() {
        let function = noise_function(1.0, 1.0, no_shift());
        assert_eq!(function.range(), NormalNoise::builder().build().range());
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let rewritten =
            function.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(1.0));
        assert!(!rewritten.ptr_eq(&function));
        assert!(
            matches!(rewritten.node(), DfNode::Noise(n) if n.shift_y == DensityFunction::constant(1.0))
        );
    }
}
