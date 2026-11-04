//! `SplineFunction`: a multipoint cubic spline whose coordinates are density functions.
use std::collections::HashMap;
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::cubic_spline::CubicSpline;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, ScopedDensityBuffer,
    SharedSampler,
};
use crate::util::Interval;

/// `SplineFunction.Coordinate` is just the wrapped `DensityFunction` here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SplineFunction {
    pub spline: CubicSpline<DensityFunction>,
}

impl DfOp for SplineFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(Sampler::new(ctx, &self.spline))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let mut changed = false;
        let spline = self.spline.map_coordinates(&mut |function| {
            let new_function = rule.rewrite(function);
            changed |= !new_function.ptr_eq(function);
            new_function
        });
        if changed {
            DensityFunction::new(DfNode::Spline(SplineFunction { spline }))
        } else {
            this.clone()
        }
    }
    fn range(&self) -> Interval {
        self.spline.range(&|function| function.range())
    }
    fn domain_axes(&self) -> u8 {
        let mut axes = 0;
        self.spline
            .for_each_coordinate(&mut |function| axes |= function.domain_axes());
        axes
    }
}

/// `SplineFunction.SamplerCoordinate`: a compiled coordinate and its slot in the per-sample value cache.
#[derive(Clone)]
struct SamplerCoordinate {
    sampler: SharedSampler,
    index: usize,
}

/// `SplineFunction.Sampler`. Equal coordinate functions share one compiled sampler and one cache slot.
pub struct Sampler {
    spline: CubicSpline<SamplerCoordinate>,
    coordinate_count: usize,
}

impl Sampler {
    pub fn new(ctx: &dyn CompileContext, spline: &CubicSpline<DensityFunction>) -> Sampler {
        let mut coordinates: HashMap<DensityFunction, SamplerCoordinate> = HashMap::new();
        let spline = spline.map_coordinates(&mut |function| {
            let index = coordinates.len();
            coordinates
                .entry(function.clone())
                .or_insert_with(|| SamplerCoordinate {
                    sampler: function.compile_sampler(ctx),
                    index,
                })
                .clone()
        });
        Sampler {
            spline,
            coordinate_count: coordinates.len(),
        }
    }
}

impl DensitySampler for Sampler {
    /// `BufferSplineInput`: each coordinate is sampled over the whole volume on first use.
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        let mut buffers: Vec<Option<ScopedDensityBuffer>> =
            (0..self.coordinate_count).map(|_| None).collect();
        for i in 0..out.size() {
            let value = self.spline.sample(&mut |coordinate| {
                buffers[coordinate.index]
                    .get_or_insert_with(|| {
                        let mut buffer = context.acquire_buffer(volume);
                        coordinate
                            .sampler
                            .sample_volume(context, &mut buffer, volume);
                        buffer
                    })
                    .get(i)
            });
            out.set(i, value);
        }
    }

    /// `PointSplineInput`: coordinate values are cached per sample (NaN marks "not yet sampled").
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let mut cached_values = vec![f32::NAN; self.coordinate_count];
        self.spline.sample(&mut |coordinate| {
            let cached_value = cached_values[coordinate.index];
            if !cached_value.is_nan() {
                return cached_value;
            }
            let value = coordinate
                .sampler
                .sample_value(context, block_x, block_y, block_z);
            cached_values[coordinate.index] = value;
            value
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::densityfunction::cubic_spline::Multipoint;
    use crate::densityfunction::op::slice::test_support::{NoContext, fn_sampler, opaque};

    fn spline_function(spline: CubicSpline<DensityFunction>) -> DensityFunction {
        DensityFunction::new(DfNode::Spline(SplineFunction { spline }))
    }

    /// Two points (0 -> 1, 1 -> 3) with slopes 0 and 4 over coordinate `c`.
    fn hermite(c: DensityFunction) -> CubicSpline<DensityFunction> {
        CubicSpline::Multipoint(Multipoint::new(
            c,
            vec![0.0, 1.0],
            vec![CubicSpline::constant(1.0), CubicSpline::constant(3.0)],
            vec![0.0, 4.0],
        ))
    }

    #[test]
    fn point_sample_matches_hand_evaluated_hermite_segment() {
        let coordinate = opaque(
            Interval::of(-1.0, 2.0),
            1,
            fn_sampler(|x, _, _| x as f32 * 0.25),
        );
        let sampler = spline_function(hermite(coordinate)).compile_sampler(&NoContext);
        let context = SamplerContext::empty_uncached();
        // x = 2 -> input 0.5, t = 0.5: lerp(0.5, 1, 3) = 2; a = 0 * 1 - 2 = -2; b = -4 * 1 + 2 = -2;
        // lerp(0.5, a, b) = -2; result = 2 + 0.5 * 0.5 * -2 = 1.5.
        assert_eq!(sampler.sample_value(&context, 2, 0, 0), 1.5);
        // Below the first location: linear extension with slope 0; above the last: 3 + 4 * (1.5 - 1).
        assert_eq!(sampler.sample_value(&context, -4, 0, 0), 1.0);
        assert_eq!(sampler.sample_value(&context, 6, 0, 0), 5.0);
    }

    #[test]
    fn equal_coordinates_are_sampled_once_per_point() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let coordinate = opaque(
            Interval::of(-1.0, 2.0),
            1,
            fn_sampler(move |x, _, _| {
                counted.fetch_add(1, Ordering::Relaxed);
                x as f32 * 0.5
            }),
        );
        // Outer spline on `coordinate` whose last branch is another spline on the same coordinate.
        let nested = CubicSpline::Multipoint(Multipoint::new(
            coordinate.clone(),
            vec![0.0, 1.0],
            vec![CubicSpline::constant(0.0), hermite(coordinate)],
            vec![0.0, 0.0],
        ));
        let sampler = spline_function(nested).compile_sampler(&NoContext);
        // x = 3 -> input 1.5, past the last location of both splines: inner gives 3 + 4 * 0.5 = 5.
        assert_eq!(
            sampler.sample_value(&SamplerContext::empty_uncached(), 3, 0, 0),
            5.0
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn volume_path_matches_point_path() {
        let axis = |axes, f: fn(i32, i32, i32) -> f32| {
            opaque(Interval::of(-2.0, 2.0), axes, fn_sampler(f))
        };
        let function = spline_function(CubicSpline::Multipoint(Multipoint::new(
            axis(1, |x, _, _| x as f32 * 0.5),
            vec![-1.0, 0.0, 1.0],
            vec![
                hermite(axis(2, |_, y, _| y as f32 * 0.25)),
                CubicSpline::constant(2.0),
                hermite(axis(4, |_, _, z| z as f32 * 0.5)),
            ],
            vec![0.5, 0.0, -1.0],
        )));
        let sampler = function.compile_sampler(&NoContext);
        let context = SamplerContext::empty_uncached();
        let volume = DensityVolume::new(5, 3, 4, -4, -2, -3, 1, 2, 1);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&context, &mut out, &volume);
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                for y in 0..volume.size_y {
                    let expected = sampler.sample_value(
                        &context,
                        volume.block_x(x),
                        volume.block_y(y),
                        volume.block_z(z),
                    );
                    assert_eq!(
                        out.get(volume.index_unchecked(x, y, z)).to_bits(),
                        expected.to_bits()
                    );
                }
            }
        }
    }

    #[test]
    fn domain_axes_or_all_coordinates_and_range_uses_coordinate_ranges() {
        let function = spline_function(CubicSpline::Multipoint(Multipoint::new(
            opaque(Interval::of(0.0, 1.0), 1, fn_sampler(|_, _, _| 0.0)),
            vec![0.0, 1.0],
            vec![
                CubicSpline::constant(1.0),
                hermite(opaque(Interval::of(0.0, 1.0), 4, fn_sampler(|_, _, _| 0.0))),
            ],
            vec![0.0, 0.0],
        )));
        assert_eq!(function.domain_axes(), 5);
        // Inputs stay within [0, 1], so no extension. The inner spline (values 1 and 3, slopes 0 and 4) dips to
        // 1 + 0.25 * -2 = 0.5 inside its segment; the outer slopes are 0, so the total range is [0.5, 3].
        assert_eq!(function.range(), Interval::of(0.5, 3.0));
    }

    #[test]
    fn rewrite_children_preserves_identity_when_no_coordinate_changes() {
        let function = spline_function(hermite(DensityFunction::constant(0.5)));
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let rewritten =
            function.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(0.75));
        assert_eq!(
            rewritten,
            spline_function(hermite(DensityFunction::constant(0.75)))
        );
    }
}
