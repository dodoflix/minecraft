//! `generator/GradientFunction.java` and `TilingMode.java`.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::function::{CompileContext, DensityFunction, DfOp, axes_from};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::densityfunction::{Axis, DfRewriteRule};
use crate::util::{Interval, floor_div, floor_mod, mth};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TilingMode {
    ClampToEdge,
    Repeat,
    MirroredRepeat,
}

impl TilingMode {
    pub const ALL: [TilingMode; 3] = [Self::ClampToEdge, Self::Repeat, Self::MirroredRepeat];

    /// Serialized name ("clamp_to_edge", "repeat", "mirrored_repeat").
    pub fn name(self) -> &'static str {
        match self {
            Self::ClampToEdge => "clamp_to_edge",
            Self::Repeat => "repeat",
            Self::MirroredRepeat => "mirrored_repeat",
        }
    }

    pub fn from_name(name: &str) -> Option<TilingMode> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

#[derive(Clone, Debug)]
pub struct GradientFunction {
    pub axis: Axis,
    pub tiling: TilingMode,
    pub from_coordinate: i32,
    pub to_coordinate: i32,
    pub from_value: f32,
    pub to_value: f32,
}
impl PartialEq for GradientFunction {
    fn eq(&self, o: &Self) -> bool {
        self.axis == o.axis
            && self.tiling == o.tiling
            && self.from_coordinate == o.from_coordinate
            && self.to_coordinate == o.to_coordinate
            && self.from_value.to_bits() == o.from_value.to_bits()
            && self.to_value.to_bits() == o.to_value.to_bits()
    }
}
impl Eq for GradientFunction {}
impl Hash for GradientFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.axis.hash(s);
        self.tiling.hash(s);
        self.from_coordinate.hash(s);
        self.to_coordinate.hash(s);
        self.from_value.to_bits().hash(s);
        self.to_value.to_bits().hash(s);
    }
}

impl DfOp for GradientFunction {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        let coordinate_range = self.to_coordinate.wrapping_sub(self.from_coordinate);
        let coordinate_factor = (self.to_value - self.from_value) / coordinate_range as f32;
        match self.tiling {
            TilingMode::ClampToEdge => Arc::new(ClampedSampler {
                axis: self.axis,
                from_coordinate: self.from_coordinate,
                min_coordinate: self.from_coordinate.min(self.to_coordinate),
                max_coordinate: self.from_coordinate.max(self.to_coordinate),
                from_value: self.from_value,
                coordinate_factor,
            }),
            TilingMode::Repeat => Arc::new(RepeatSampler {
                axis: self.axis,
                from_coordinate: self.from_coordinate,
                coordinate_range,
                from_value: self.from_value,
                coordinate_factor,
            }),
            TilingMode::MirroredRepeat => Arc::new(MirroredRepeatSampler {
                axis: self.axis,
                from_coordinate: self.from_coordinate,
                coordinate_range,
                from_value: self.from_value,
                coordinate_factor,
            }),
        }
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }
    fn range(&self) -> Interval {
        Interval::encapsulating_f(self.from_value, self.to_value)
    }
    fn domain_axes(&self) -> u8 {
        axes_from(self.axis)
    }
}

/// `GradientFunction.GradientSampler`: a value that depends on one coordinate only.
trait GradientSampler: DensitySampler {
    fn axis(&self) -> Axis;
    fn compute(&self, coordinate: i32) -> f32;
}

/// `GradientSampler.sampleVolume`.
fn sample_gradient_volume(
    sampler: &impl GradientSampler,
    out: &mut DensityBuffer,
    volume: &DensityVolume,
) {
    let size_y = volume.size_y as usize;
    match sampler.axis() {
        Axis::X => {
            for x in 0..volume.size_x {
                let value = sampler.compute(volume.block_x(x));
                for z in 0..volume.size_z {
                    out.set_range(volume.index_unchecked(x, 0, z), size_y, value);
                }
            }
        }
        Axis::Y => {
            for y in 0..volume.size_y {
                let value = sampler.compute(volume.block_y(y));
                for z in 0..volume.size_z {
                    for x in 0..volume.size_x {
                        out.set(volume.index_unchecked(x, y, z), value);
                    }
                }
            }
        }
        Axis::Z => {
            for z in 0..volume.size_z {
                let value = sampler.compute(volume.block_z(z));
                out.set_range(
                    volume.index_unchecked(0, 0, z),
                    (volume.size_x * volume.size_y) as usize,
                    value,
                );
            }
        }
    }
}

macro_rules! impl_gradient_density_sampler {
    ($ty:ty) => {
        impl DensitySampler for $ty {
            fn sample_volume(
                &self,
                _context: &SamplerContext,
                out: &mut DensityBuffer,
                volume: &DensityVolume,
            ) {
                sample_gradient_volume(self, out, volume);
            }
            fn sample_value(
                &self,
                _context: &SamplerContext,
                block_x: i32,
                block_y: i32,
                block_z: i32,
            ) -> f32 {
                self.compute(self.axis.choose(block_x, block_y, block_z))
            }
        }
    };
}

/// `GradientFunction.ClampedSampler`.
struct ClampedSampler {
    axis: Axis,
    from_coordinate: i32,
    min_coordinate: i32,
    max_coordinate: i32,
    from_value: f32,
    coordinate_factor: f32,
}

impl GradientSampler for ClampedSampler {
    fn axis(&self) -> Axis {
        self.axis
    }
    fn compute(&self, coordinate: i32) -> f32 {
        let relative_coordinate =
            mth::clamp_i(coordinate, self.min_coordinate, self.max_coordinate)
                .wrapping_sub(self.from_coordinate);
        self.from_value + relative_coordinate as f32 * self.coordinate_factor
    }
}
impl_gradient_density_sampler!(ClampedSampler);

/// `GradientFunction.MirroredRepeatSampler`.
struct MirroredRepeatSampler {
    axis: Axis,
    from_coordinate: i32,
    coordinate_range: i32,
    from_value: f32,
    coordinate_factor: f32,
}

impl GradientSampler for MirroredRepeatSampler {
    fn axis(&self) -> Axis {
        self.axis
    }
    fn compute(&self, coordinate: i32) -> f32 {
        let relative_coordinate = coordinate.wrapping_sub(self.from_coordinate);
        let tile_index = floor_div(relative_coordinate, self.coordinate_range);
        let local_coordinate =
            relative_coordinate.wrapping_sub(tile_index.wrapping_mul(self.coordinate_range));
        if tile_index & 1 == 0 {
            self.from_value + local_coordinate as f32 * self.coordinate_factor
        } else {
            self.from_value
                + self.coordinate_range.wrapping_sub(local_coordinate) as f32
                    * self.coordinate_factor
        }
    }
}
impl_gradient_density_sampler!(MirroredRepeatSampler);

/// `GradientFunction.RepeatSampler`.
struct RepeatSampler {
    axis: Axis,
    from_coordinate: i32,
    coordinate_range: i32,
    from_value: f32,
    coordinate_factor: f32,
}

impl GradientSampler for RepeatSampler {
    fn axis(&self) -> Axis {
        self.axis
    }
    fn compute(&self, coordinate: i32) -> f32 {
        let relative_coordinate = coordinate.wrapping_sub(self.from_coordinate);
        self.from_value
            + floor_mod(relative_coordinate, self.coordinate_range) as f32 * self.coordinate_factor
    }
}
impl_gradient_density_sampler!(RepeatSampler);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::function::DfNode;
    use crate::densityfunction::generator::constant::tests::NoCtx;

    fn gradient(
        axis: Axis,
        tiling: TilingMode,
        from: i32,
        to: i32,
        from_value: f32,
        to_value: f32,
    ) -> SharedSampler {
        let function = DensityFunction::new(DfNode::Gradient(GradientFunction {
            axis,
            tiling,
            from_coordinate: from,
            to_coordinate: to,
            from_value,
            to_value,
        }));
        function.compile_sampler(&NoCtx)
    }

    fn values(sampler: &SharedSampler, coordinates: &[i32]) -> Vec<f32> {
        let ctx = SamplerContext::empty_uncached();
        coordinates
            .iter()
            .map(|&y| sampler.sample_value(&ctx, 0, y, 0))
            .collect()
    }

    #[test]
    fn clamp_to_edge_incl_reversed_range() {
        // factor = (-1 - 1) / 8 = -0.25 per block, clamped to [0, 8].
        let sampler = gradient(Axis::Y, TilingMode::ClampToEdge, 0, 8, 1.0, -1.0);
        assert_eq!(
            values(&sampler, &[-100, 0, 2, 8, 100]),
            [1.0, 1.0, 0.5, -1.0, -1.0]
        );
        // from > to: range -8, factor (0 - 4) / -8 = 0.5
        let reversed = gradient(Axis::Y, TilingMode::ClampToEdge, 8, 0, 4.0, 0.0);
        assert_eq!(
            values(&reversed, &[-5, 0, 6, 8, 20]),
            [0.0, 0.0, 3.0, 4.0, 4.0]
        );
    }

    #[test]
    fn repeat_with_negative_coordinates() {
        // range 4, factor 1: value = floorMod(y - 2, 4)
        let sampler = gradient(Axis::Y, TilingMode::Repeat, 2, 6, 0.0, 4.0);
        assert_eq!(
            values(&sampler, &[2, 5, 6, 7, 1, -2, -3]),
            [0.0, 3.0, 0.0, 1.0, 3.0, 0.0, 3.0]
        );
    }

    #[test]
    fn mirrored_repeat_with_negative_coordinates() {
        // range 4, factor 1, from 0: tile 0 ascends, tile 1 descends, tile -1 descends.
        let sampler = gradient(Axis::Y, TilingMode::MirroredRepeat, 0, 4, 0.0, 4.0);
        assert_eq!(
            values(&sampler, &[0, 1, 3, 4, 5, 7, 8, -1, -3, -4, -5]),
            [0.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, 1.0, 3.0, 4.0, 3.0]
        );
    }

    #[test]
    fn volume_matches_value_on_every_axis() {
        let ctx = SamplerContext::empty_uncached();
        let volume = DensityVolume::new(3, 4, 5, -2, -3, -4, 2, 3, 1);
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for tiling in TilingMode::ALL {
                let sampler = gradient(axis, tiling, -3, 5, 0.5, -2.0);
                let mut out = DensityBuffer::create_unpooled(volume.size());
                sampler.sample_volume(&ctx, &mut out, &volume);
                for z in 0..volume.size_z {
                    for x in 0..volume.size_x {
                        for y in 0..volume.size_y {
                            let expected = sampler.sample_value(
                                &ctx,
                                volume.block_x(x),
                                volume.block_y(y),
                                volume.block_z(z),
                            );
                            assert_eq!(
                                out.get(volume.index_unchecked(x, y, z)),
                                expected,
                                "{axis:?} {tiling:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn range_and_axes() {
        let function = GradientFunction {
            axis: Axis::Z,
            tiling: TilingMode::Repeat,
            from_coordinate: 0,
            to_coordinate: 1,
            from_value: 3.0,
            to_value: -2.0,
        };
        assert_eq!(function.range(), Interval::of(-2.0, 3.0));
        assert_eq!(function.domain_axes(), 4);
        assert_eq!(
            TilingMode::from_name("mirrored_repeat"),
            Some(TilingMode::MirroredRepeat)
        );
    }
}
