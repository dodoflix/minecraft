//! `net.minecraft.util.random.{Weighted, WeightedList}`.

use serde_json::Value;

use mc_core::random::RandomSource;

use crate::data::LoadError;
use crate::densityfunction::codec::{Fields, LoadResult, parse_int, parse_list};

/// `Weighted<T>`: a value with a non-negative weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Weighted<E> {
    pub value: E,
    pub weight: i32,
}

impl<E> Weighted<E> {
    /// `Weighted.codec(elementCodec)`: `{ "data": <element>, "weight": <int >= 0> }`.
    pub fn from_json(
        value: &Value,
        parse: impl FnOnce(&Value) -> LoadResult<E>,
    ) -> LoadResult<Weighted<E>> {
        let fields = Fields::new(value)?;
        let data = fields.required("data", parse)?;
        let weight = fields.required("weight", |v| {
            let weight = parse_int(v)?;
            if weight < 0 {
                return Err(LoadError::new(format!(
                    "Value must be non-negative: {weight}"
                )));
            }
            Ok(weight)
        })?;
        Ok(Weighted {
            value: data,
            weight,
        })
    }
}

/// `WeightedList<E>`. The flat/compact selectors of vanilla are the same cumulative-weight lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct WeightedList<E> {
    total_weight: i32,
    items: Vec<Weighted<E>>,
}

impl<E> WeightedList<E> {
    /// `WeightedList.of(items)` (`WeightedRandom.getTotalWeight` overflow is an error).
    pub fn of(items: Vec<Weighted<E>>) -> LoadResult<WeightedList<E>> {
        let total: i64 = items.iter().map(|item| i64::from(item.weight)).sum();
        let total_weight = i32::try_from(total)
            .map_err(|_| LoadError::new("Sum of weights must be <= 2147483647"))?;
        Ok(WeightedList {
            total_weight,
            items,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.total_weight == 0
    }

    /// `unwrap()`.
    pub fn unwrap(&self) -> &[Weighted<E>] {
        &self.items
    }

    /// `getRandom(random)`.
    pub fn get_random(&self, random: &mut dyn RandomSource) -> Option<&E> {
        if self.is_empty() {
            return None;
        }
        Some(self.select(random.next_int_bounded(self.total_weight)))
    }

    /// `getRandomOrThrow(random)`: panics (vanilla `IllegalStateException`) on an empty list.
    pub fn get_random_or_throw(&self, random: &mut dyn RandomSource) -> &E {
        assert!(!self.is_empty(), "Weighted list has no elements");
        self.select(random.next_int_bounded(self.total_weight))
    }

    fn select(&self, mut selection: i32) -> &E {
        for entry in &self.items {
            selection -= entry.weight;
            if selection < 0 {
                return &entry.value;
            }
        }
        unreachable!("{selection} exceeded total weight")
    }

    /// `WeightedList.codec(elementCodec)`: a list of `Weighted` entries.
    pub fn from_json(
        value: &Value,
        mut parse: impl FnMut(&Value) -> LoadResult<E>,
    ) -> LoadResult<WeightedList<E>> {
        let items = parse_list(value, |entry| Weighted::from_json(entry, &mut parse))?;
        WeightedList::of(items)
    }

    /// `WeightedList.nonEmptyCodec(elementCodec)`.
    pub fn non_empty_from_json(
        value: &Value,
        parse: impl FnMut(&Value) -> LoadResult<E>,
    ) -> LoadResult<WeightedList<E>> {
        let list = WeightedList::from_json(value, parse)?;
        if list.is_empty() {
            return Err(LoadError::new(
                "Weighted list must contain at least one entry with non-zero weight",
            ));
        }
        Ok(list)
    }
}
