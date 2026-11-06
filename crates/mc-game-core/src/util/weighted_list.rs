//! `net.minecraft.util.random.{Weighted, WeightedList}`. The flat and compact selectors of vanilla are the same
//! cumulative-weight lookup after one `nextInt(totalWeight)`.

use mc_core::random::RandomSource;
use mc_dfu::extra::NON_NEGATIVE_INT;
use mc_dfu::{Codec, DataResult, record};

/// `Weighted<T>`: a value with a non-negative weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Weighted<T> {
    pub value: T,
    pub weight: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeightedList<E> {
    total_weight: i32,
    items: Vec<Weighted<E>>,
}

impl<E> Default for WeightedList<E> {
    fn default() -> Self {
        Self {
            total_weight: 0,
            items: Vec::new(),
        }
    }
}

impl<E> WeightedList<E> {
    /// `WeightedList.of()`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// `WeightedList.of(value)`: a single entry of weight 1.
    pub fn of_value(value: E) -> Self {
        Self {
            total_weight: 1,
            items: vec![Weighted { value, weight: 1 }],
        }
    }

    /// `WeightedList.of(items)`; `None` when `WeightedRandom.getTotalWeight` overflows an int.
    pub fn of(items: Vec<Weighted<E>>) -> Option<Self> {
        let total: i64 = items.iter().map(|i| i64::from(i.weight)).sum();
        Some(Self {
            total_weight: i32::try_from(total).ok()?,
            items,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.total_weight == 0
    }

    pub fn unwrap(&self) -> &[Weighted<E>] {
        &self.items
    }

    /// `getRandom(random)`.
    pub fn get_random(&self, random: &mut dyn RandomSource) -> Option<&E> {
        if self.is_empty() {
            return None;
        }
        let mut selection = random.next_int_bounded(self.total_weight);
        for entry in &self.items {
            selection -= entry.weight;
            if selection < 0 {
                return Some(&entry.value);
            }
        }
        None
    }

    /// `map(mapper)`.
    pub fn map<T>(self, mut mapper: impl FnMut(E) -> T) -> WeightedList<T> {
        WeightedList {
            total_weight: self.total_weight,
            items: self
                .items
                .into_iter()
                .map(|w| Weighted {
                    value: mapper(w.value),
                    weight: w.weight,
                })
                .collect(),
        }
    }
}

impl<E: Clone + Send + Sync + 'static> WeightedList<E> {
    /// `WeightedList.codec(elementCodec)`: a list of `{data, weight}`.
    pub fn codec(element: &Codec<E>) -> Codec<WeightedList<E>> {
        let weighted = record!(Weighted<E> { value: element.field_of("data"), weight: NON_NEGATIVE_INT.field_of("weight") }).codec();
        weighted.list_of().flat_xmap(
            |items| match WeightedList::of(items) {
                Some(list) => DataResult::success(list),
                None => DataResult::error_str("Sum of weights must be <= 2147483647"),
            },
            |list: &WeightedList<E>| DataResult::success(list.items.clone()),
        )
    }
}
