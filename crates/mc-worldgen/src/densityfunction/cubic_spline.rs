//! net.minecraft.util.CubicSpline (float splines whose coordinates are generic).
//!
//! `I` is the coordinate type: [`crate::densityfunction::DensityFunction`] for the declarative form,
//! compiled sampler coordinates for the sampling form. Anything needing the coordinates' value or range
//! passes closures.

use crate::util::{Interval, jmax, jmin, mth};

#[derive(Clone, Debug)]
pub enum CubicSpline<I> {
    Constant(f32),
    Multipoint(Multipoint<I>),
}

#[derive(Clone, Debug)]
pub struct Multipoint<I> {
    pub coordinate: I,
    pub locations: Vec<f32>,
    pub values: Vec<CubicSpline<I>>,
    pub derivatives: Vec<f32>,
}

impl<I> Multipoint<I> {
    /// `new Multipoint(...)`: panics (IllegalArgumentException) on empty or mismatched sizes.
    pub fn new(
        coordinate: I,
        locations: Vec<f32>,
        values: Vec<CubicSpline<I>>,
        derivatives: Vec<f32>,
    ) -> Multipoint<I> {
        assert!(
            locations.len() == values.len() && locations.len() == derivatives.len(),
            "All lengths must be equal, got: {} {} {}",
            locations.len(),
            values.len(),
            derivatives.len()
        );
        assert!(
            !locations.is_empty(),
            "Cannot create a multipoint spline with no points"
        );
        Multipoint {
            coordinate,
            locations,
            values,
            derivatives,
        }
    }
}

fn linear_extend(
    input: f32,
    locations: &[f32],
    value: f32,
    derivatives: &[f32],
    index: usize,
) -> f32 {
    let derivative = derivatives[index];
    if derivative == 0.0 {
        value
    } else {
        value + derivative * (input - locations[index])
    }
}

fn find_interval_start(locations: &[f32], input: f32) -> i32 {
    mth::binary_search(0, locations.len() as i32, |i| input < locations[i as usize]) - 1
}

impl<I> CubicSpline<I> {
    pub fn constant(value: f32) -> CubicSpline<I> {
        CubicSpline::Constant(value)
    }

    /// `forEachCoordinate`: visits this spline's coordinate, then (recursively) each value's, in order.
    pub fn for_each_coordinate(&self, f: &mut dyn FnMut(&I)) {
        if let CubicSpline::Multipoint(m) = self {
            f(&m.coordinate);
            for v in &m.values {
                v.for_each_coordinate(f);
            }
        }
    }

    /// `mapCoordinates`: same shape, coordinates mapped in `forEachCoordinate` order.
    pub fn map_coordinates<R>(&self, f: &mut dyn FnMut(&I) -> R) -> CubicSpline<R> {
        match self {
            CubicSpline::Constant(v) => CubicSpline::Constant(*v),
            CubicSpline::Multipoint(m) => {
                let coordinate = f(&m.coordinate);
                let values = m.values.iter().map(|v| v.map_coordinates(f)).collect();
                CubicSpline::Multipoint(Multipoint {
                    coordinate,
                    locations: m.locations.clone(),
                    values,
                    derivatives: m.derivatives.clone(),
                })
            }
        }
    }

    /// `CubicSpline.range()`, given the range of each coordinate.
    pub fn range(&self, coordinate_range: &dyn Fn(&I) -> Interval) -> Interval {
        match self {
            CubicSpline::Constant(v) => Interval::of_exact(*v),
            CubicSpline::Multipoint(m) => m.range(coordinate_range),
        }
    }

    /// `CubicSpline.sample`: `apply` returns the value of a coordinate at the sample point.
    pub fn sample(&self, apply: &mut dyn FnMut(&I) -> f32) -> f32 {
        match self {
            CubicSpline::Constant(v) => *v,
            CubicSpline::Multipoint(m) => m.sample(apply),
        }
    }
}

impl<I> Multipoint<I> {
    pub fn range(&self, coordinate_range: &dyn Fn(&I) -> Interval) -> Interval {
        let last_index = self.locations.len() - 1;
        let mut min_value = f32::INFINITY;
        let mut max_value = f32::NEG_INFINITY;
        let input_range = coordinate_range(&self.coordinate);
        if input_range.is_nai() {
            return input_range;
        }
        if input_range.min() < self.locations[0] {
            let first_range = self.values[0].range(coordinate_range);
            let edge1 = linear_extend(
                input_range.min(),
                &self.locations,
                first_range.min(),
                &self.derivatives,
                0,
            );
            let edge2 = linear_extend(
                input_range.min(),
                &self.locations,
                first_range.max(),
                &self.derivatives,
                0,
            );
            min_value = jmin(min_value, jmin(edge1, edge2));
            max_value = jmax(max_value, jmax(edge1, edge2));
        }
        if input_range.max() > self.locations[last_index] {
            let last_range = self.values[last_index].range(coordinate_range);
            let edge1 = linear_extend(
                input_range.max(),
                &self.locations,
                last_range.min(),
                &self.derivatives,
                last_index,
            );
            let edge2 = linear_extend(
                input_range.max(),
                &self.locations,
                last_range.max(),
                &self.derivatives,
                last_index,
            );
            min_value = jmin(min_value, jmin(edge1, edge2));
            max_value = jmax(max_value, jmax(edge1, edge2));
        }
        let value_ranges: Vec<Interval> = self
            .values
            .iter()
            .map(|v| v.range(coordinate_range))
            .collect();
        for range in &value_ranges {
            min_value = jmin(min_value, range.min());
            max_value = jmax(max_value, range.max());
        }
        for i in 0..last_index {
            let x1 = self.locations[i];
            let x2 = self.locations[i + 1];
            let x_diff = x2 - x1;
            let range1 = value_ranges[i];
            let range2 = value_ranges[i + 1];
            let (min1, max1) = (range1.min(), range1.max());
            let (min2, max2) = (range2.min(), range2.max());
            let d1 = self.derivatives[i];
            let d2 = self.derivatives[i + 1];
            if d1 != 0.0 || d2 != 0.0 {
                let p1 = d1 * x_diff;
                let p2 = d2 * x_diff;
                let min_lerp1 = jmin(min1, min2);
                let max_lerp1 = jmax(max1, max2);
                let min_a = p1 - max2 + min1;
                let max_a = p1 - min2 + max1;
                let min_b = -p2 + min2 - max1;
                let max_b = -p2 + max2 - min1;
                let min_lerp2 = jmin(min_a, min_b);
                let max_lerp2 = jmax(max_a, max_b);
                min_value = jmin(min_value, min_lerp1 + 0.25 * min_lerp2);
                max_value = jmax(max_value, max_lerp1 + 0.25 * max_lerp2);
            }
        }
        Interval::of(min_value, max_value)
    }

    pub fn sample(&self, apply: &mut dyn FnMut(&I) -> f32) -> f32 {
        let input = apply(&self.coordinate);
        let start = find_interval_start(&self.locations, input);
        let last_index = self.locations.len() - 1;
        if start < 0 {
            let v = self.values[0].sample(apply);
            linear_extend(input, &self.locations, v, &self.derivatives, 0)
        } else if start as usize == last_index {
            let v = self.values[last_index].sample(apply);
            linear_extend(input, &self.locations, v, &self.derivatives, last_index)
        } else {
            let start = start as usize;
            let x1 = self.locations[start];
            let x2 = self.locations[start + 1];
            let t = (input - x1) / (x2 - x1);
            let d1 = self.derivatives[start];
            let d2 = self.derivatives[start + 1];
            let y1 = self.values[start].sample(apply);
            let y2 = self.values[start + 1].sample(apply);
            let a = d1 * (x2 - x1) - (y2 - y1);
            let b = -d2 * (x2 - x1) + (y2 - y1);
            mth::lerp(t, y1, y2) + t * (1.0 - t) * mth::lerp(t, a, b)
        }
    }
}

impl<I: PartialEq> PartialEq for CubicSpline<I> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (CubicSpline::Constant(a), CubicSpline::Constant(b)) => a.to_bits() == b.to_bits(),
            (CubicSpline::Multipoint(a), CubicSpline::Multipoint(b)) => {
                a.coordinate == b.coordinate
                    && a.locations
                        .iter()
                        .map(|f| f.to_bits())
                        .eq(b.locations.iter().map(|f| f.to_bits()))
                    && a.derivatives
                        .iter()
                        .map(|f| f.to_bits())
                        .eq(b.derivatives.iter().map(|f| f.to_bits()))
                    && a.values == b.values
            }
            _ => false,
        }
    }
}
impl<I: Eq> Eq for CubicSpline<I> {}

impl<I: std::hash::Hash> std::hash::Hash for CubicSpline<I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            CubicSpline::Constant(v) => {
                0u8.hash(state);
                v.to_bits().hash(state);
            }
            CubicSpline::Multipoint(m) => {
                1u8.hash(state);
                m.coordinate.hash(state);
                m.locations.iter().for_each(|f| f.to_bits().hash(state));
                m.derivatives.iter().for_each(|f| f.to_bits().hash(state));
                m.values.hash(state);
            }
        }
    }
}
