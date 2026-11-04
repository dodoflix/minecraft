//! Vanilla's `DoubleList` coordinate lists (`CubePointRange`, `DoubleArrayList`, `OffsetDoubleList`,
//! the `NonOverlappingMerger` concatenation) and the `IndexMerger` family.
//!
//! Only the `CubePointRange` identity matters to shape algorithms (`instanceof`), so every other kind of
//! list is materialised eagerly into an `Arc<[f64]>`; the element values are computed with exactly the
//! same floating point operations as the lazy Java views.

use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Coords {
    /// `CubePointRange(parts)`: `parts + 1` points, `i / parts`.
    Cube(i32),
    Array(Arc<[f64]>),
}

impl Coords {
    /// `new CubePointRange(parts)`; panics for `parts <= 0` like vanilla.
    pub fn cube(parts: i32) -> Coords {
        assert!(parts > 0, "Need at least 1 part");
        Coords::Cube(parts)
    }

    pub fn array(values: impl Into<Arc<[f64]>>) -> Coords {
        Coords::Array(values.into())
    }

    #[inline]
    pub fn size(&self) -> usize {
        match self {
            Coords::Cube(parts) => *parts as usize + 1,
            Coords::Array(a) => a.len(),
        }
    }

    #[inline]
    pub fn get_double(&self, index: usize) -> f64 {
        match self {
            Coords::Cube(parts) => index as i32 as f64 / *parts as f64,
            Coords::Array(a) => a[index],
        }
    }

    pub fn is_cube_point_range(&self) -> bool {
        matches!(self, Coords::Cube(_))
    }

    pub fn to_vec(&self) -> Vec<f64> {
        (0..self.size()).map(|i| self.get_double(i)).collect()
    }

    /// `OffsetDoubleList(this, offset)`.
    pub fn offset(&self, offset: f64) -> Coords {
        Coords::Array((0..self.size()).map(|i| self.get_double(i) + offset).collect())
    }

    /// `Objects.equals(first, second)` on two `DoubleList`s: sizes equal and elements equal under
    /// primitive `==` (so `0.0 == -0.0`), regardless of list kind.
    pub fn list_equals(&self, other: &Coords) -> bool {
        self.size() == other.size() && (0..self.size()).all(|i| self.get_double(i) == other.get_double(i))
    }
}

impl From<Vec<f64>> for Coords {
    fn from(v: Vec<f64>) -> Coords {
        Coords::Array(v.into())
    }
}

impl From<&[f64]> for Coords {
    fn from(v: &[f64]) -> Coords {
        Coords::Array(v.into())
    }
}

/// Strict structural equality: same kind and bit-identical values (for tests / dumps).
impl PartialEq for Coords {
    fn eq(&self, other: &Coords) -> bool {
        match (self, other) {
            (Coords::Cube(a), Coords::Cube(b)) => a == b,
            (Coords::Array(a), Coords::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.to_bits() == y.to_bits())
            }
            _ => false,
        }
    }
}
impl Eq for Coords {}

pub(crate) fn gcd(a: i32, b: i32) -> i32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `Shapes.lcm`
pub(crate) fn lcm(first: i32, second: i32) -> i64 {
    first as i64 * (second / gcd(first, second)) as i64
}

#[derive(Clone, Debug)]
pub enum IndexMerger {
    DiscreteCube { result: i32, first_div: i32, second_div: i32 },
    NonOverlapping { lower: Coords, upper: Coords, swap: bool },
    Identical(Coords),
    Indirect { result: Vec<f64>, first_indices: Vec<i32>, second_indices: Vec<i32>, result_length: usize },
}

impl IndexMerger {
    /// `new DiscreteCubeMerger(firstSize, secondSize)`
    pub fn discrete_cube(first_size: i32, second_size: i32) -> IndexMerger {
        let gcd = gcd(first_size, second_size);
        IndexMerger::DiscreteCube {
            result: lcm(first_size, second_size) as i32,
            first_div: first_size / gcd,
            second_div: second_size / gcd,
        }
    }

    /// `new IndirectMerger(first, second, firstOnlyMatters, secondOnlyMatters)`
    pub fn indirect(first: &Coords, second: &Coords, first_only_matters: bool, second_only_matters: bool) -> IndexMerger {
        let mut last_value = f64::NAN;
        let first_size = first.size();
        let second_size = second.size();
        let capacity = first_size + second_size;
        let mut result = vec![0.0; capacity];
        let mut first_indices = vec![0i32; capacity];
        let mut second_indices = vec![0i32; capacity];
        let can_skip_first = !first_only_matters;
        let can_skip_second = !second_only_matters;
        let mut result_index = 0usize;
        let mut first_index = 0usize;
        let mut second_index = 0usize;

        let result_length = loop {
            let ran_out_of_first = first_index >= first_size;
            let ran_out_of_second = second_index >= second_size;
            if ran_out_of_first && ran_out_of_second {
                break result_index.max(1);
            }

            let chose_first = !ran_out_of_first
                && (ran_out_of_second || first.get_double(first_index) < second.get_double(second_index) + 1.0E-7);
            if chose_first {
                first_index += 1;
                if can_skip_first && (second_index == 0 || ran_out_of_second) {
                    continue;
                }
            } else {
                second_index += 1;
                if can_skip_second && (first_index == 0 || ran_out_of_first) {
                    continue;
                }
            }

            let current_first_index = first_index as i32 - 1;
            let current_second_index = second_index as i32 - 1;
            let next_value = if chose_first {
                first.get_double(current_first_index as usize)
            } else {
                second.get_double(current_second_index as usize)
            };
            // Written as vanilla's `!(lastValue >= nextValue - 1.0E-7)`: NaN must take this branch.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(last_value >= next_value - 1.0E-7) {
                first_indices[result_index] = current_first_index;
                second_indices[result_index] = current_second_index;
                result[result_index] = next_value;
                result_index += 1;
                last_value = next_value;
            } else {
                first_indices[result_index - 1] = current_first_index;
                second_indices[result_index - 1] = current_second_index;
            }
        };
        IndexMerger::Indirect { result, first_indices, second_indices, result_length }
    }

    /// `IndexMerger.size()`
    pub fn size(&self) -> usize {
        match self {
            IndexMerger::DiscreteCube { result, .. } => *result as usize + 1,
            IndexMerger::NonOverlapping { lower, upper, .. } => lower.size() + upper.size(),
            IndexMerger::Identical(coords) => coords.size(),
            IndexMerger::Indirect { result_length, .. } => *result_length,
        }
    }

    /// `IndexMerger.getList()`
    pub fn get_list(&self) -> Coords {
        match self {
            IndexMerger::DiscreteCube { result, .. } => Coords::Cube(*result),
            IndexMerger::NonOverlapping { lower, upper, .. } => {
                Coords::Array((0..lower.size()).map(|i| lower.get_double(i)).chain((0..upper.size()).map(|i| upper.get_double(i))).collect())
            }
            IndexMerger::Identical(coords) => coords.clone(),
            IndexMerger::Indirect { result, result_length, .. } => {
                if *result_length <= 1 {
                    Coords::array(vec![0.0])
                } else {
                    Coords::array(&result[..*result_length])
                }
            }
        }
    }

    pub fn is_discrete_cube(&self) -> bool {
        matches!(self, IndexMerger::DiscreteCube { .. })
    }

    /// `IndexMerger.forMergedIndexes`: calls `consumer(first_index, second_index, result_index)` until it
    /// returns `false`; returns whether all calls returned `true`.
    pub fn for_merged_indexes(&self, consumer: &mut impl FnMut(i32, i32, i32) -> bool) -> bool {
        match self {
            IndexMerger::DiscreteCube { result, first_div, second_div } => {
                for i in 0..*result {
                    if !consumer(i / second_div, i / first_div, i) {
                        return false;
                    }
                }
                true
            }
            IndexMerger::NonOverlapping { lower, upper, swap } => {
                let (lower_size, upper_size) = (lower.size() as i32, upper.size() as i32);
                if *swap {
                    Self::for_non_swapped(lower_size, upper_size, &mut |f, s, r| consumer(s, f, r))
                } else {
                    Self::for_non_swapped(lower_size, upper_size, consumer)
                }
            }
            IndexMerger::Identical(coords) => {
                for i in 0..coords.size() as i32 - 1 {
                    if !consumer(i, i, i) {
                        return false;
                    }
                }
                true
            }
            IndexMerger::Indirect { first_indices, second_indices, result_length, .. } => {
                for i in 0..*result_length - 1 {
                    if !consumer(first_indices[i], second_indices[i], i as i32) {
                        return false;
                    }
                }
                true
            }
        }
    }

    fn for_non_swapped(lower_size: i32, upper_size: i32, consumer: &mut impl FnMut(i32, i32, i32) -> bool) -> bool {
        for i in 0..lower_size {
            if !consumer(i, -1, i) {
                return false;
            }
        }
        for i in 0..upper_size - 1 {
            if !consumer(lower_size - 1, i, lower_size + i) {
                return false;
            }
        }
        true
    }
}
