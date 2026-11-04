//! `generator/DistanceToPointFunction.java` and `DistanceMetric.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
    sample_volume_naive,
};
use crate::util::{Interval, jmax, mth};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DistanceMetric {
    Euclidean,
    EuclideanSquared,
    Manhattan,
    Chebyshev,
}

impl DistanceMetric {
    pub const ALL: [DistanceMetric; 4] = [
        Self::Euclidean,
        Self::EuclideanSquared,
        Self::Manhattan,
        Self::Chebyshev,
    ];

    /// Serialized name ("euclidean", ...).
    pub fn name(self) -> &'static str {
        match self {
            Self::Euclidean => "euclidean",
            Self::EuclideanSquared => "euclidean_squared",
            Self::Manhattan => "manhattan",
            Self::Chebyshev => "chebyshev",
        }
    }

    pub fn from_name(name: &str) -> Option<DistanceMetric> {
        Self::ALL.into_iter().find(|metric| metric.name() == name)
    }

    /// `compute(float, float, float)`.
    pub fn compute3(self, delta_x: f32, delta_y: f32, delta_z: f32) -> f32 {
        match self {
            Self::Euclidean => mth::length3(delta_x, delta_y, delta_z),
            Self::EuclideanSquared => mth::length_squared3(delta_x, delta_y, delta_z),
            Self::Manhattan => delta_x.abs() + delta_y.abs() + delta_z.abs(),
            Self::Chebyshev => jmax(jmax(delta_x.abs(), delta_y.abs()), delta_z.abs()),
        }
    }

    /// `compute(float, float)`.
    pub fn compute2(self, delta_x: f32, delta_z: f32) -> f32 {
        match self {
            Self::Euclidean => mth::length2(delta_x, delta_z),
            Self::EuclideanSquared => mth::length_squared2(delta_x, delta_z),
            Self::Manhattan => delta_x.abs() + delta_z.abs(),
            Self::Chebyshev => jmax(delta_x.abs(), delta_z.abs()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistanceToPointFunction {
    /// `Vec3i point` (x, y, z)
    pub point: [i32; 3],
    pub metric: DistanceMetric,
}

impl DfOp for DistanceToPointFunction {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(DistanceToPointSampler {
            x: self.point[0],
            y: self.point[1],
            z: self.point[2],
            metric: self.metric,
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
        Interval::of(0.0, f32::INFINITY)
    }
    fn domain_axes(&self) -> u8 {
        7
    }
}

/// `DistanceToPointFunction.Sampler`.
struct DistanceToPointSampler {
    x: i32,
    y: i32,
    z: i32,
    metric: DistanceMetric,
}

impl DensitySampler for DistanceToPointSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        sample_volume_naive(context, out, volume, self);
    }
    fn sample_value(
        &self,
        _context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.metric.compute3(
            self.x.wrapping_sub(block_x) as f32,
            self.y.wrapping_sub(block_y) as f32,
            self.z.wrapping_sub(block_z) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_3d() {
        assert_eq!(DistanceMetric::Euclidean.compute3(3.0, 4.0, 12.0), 13.0);
        assert_eq!(
            DistanceMetric::EuclideanSquared.compute3(3.0, -4.0, 12.0),
            169.0
        );
        assert_eq!(DistanceMetric::Manhattan.compute3(3.0, -4.0, 12.0), 19.0);
        assert_eq!(DistanceMetric::Chebyshev.compute3(3.0, -14.0, 12.0), 14.0);
    }

    #[test]
    fn metrics_2d() {
        assert_eq!(DistanceMetric::Euclidean.compute2(-3.0, 4.0), 5.0);
        assert_eq!(DistanceMetric::EuclideanSquared.compute2(-3.0, 4.0), 25.0);
        assert_eq!(DistanceMetric::Manhattan.compute2(-3.0, 4.0), 7.0);
        assert_eq!(DistanceMetric::Chebyshev.compute2(-3.0, 4.0), 4.0);
    }

    #[test]
    fn names_round_trip() {
        for metric in DistanceMetric::ALL {
            assert_eq!(DistanceMetric::from_name(metric.name()), Some(metric));
        }
        assert_eq!(DistanceMetric::from_name("euclid"), None);
    }

    #[test]
    fn sampler_offsets_from_point_and_volume_matches_value() {
        let sampler = DistanceToPointSampler {
            x: 10,
            y: 20,
            z: 30,
            metric: DistanceMetric::Manhattan,
        };
        let ctx = SamplerContext::empty_uncached();
        assert_eq!(sampler.sample_value(&ctx, 7, 24, 30), 3.0 + 4.0);
        let volume = DensityVolume::new(3, 2, 2, 8, 19, 29, 2, 1, 1);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&ctx, &mut out, &volume);
        let mut index = 0;
        for z in 0..2 {
            for x in 0..3 {
                for y in 0..2 {
                    let expected = sampler.sample_value(
                        &ctx,
                        volume.block_x(x),
                        volume.block_y(y),
                        volume.block_z(z),
                    );
                    assert_eq!(out.get(index), expected);
                    index += 1;
                }
            }
        }
        assert_eq!(out.get(0), 2.0 + 1.0 + 1.0);
    }
}
